use anyhow::{anyhow, bail, Context, Result};

use crate::core::Dma;

use super::offsets::{offsets_for_build, SmbiosOffsets};

const KERNEL_PID: u32 = 4;
const KUSER_SHARED_DATA: u64 = 0xFFFFF78000000000;
const NT_BUILD_NUMBER_OFFSET: u64 = 0x260;
const MIN_SMBIOS_TABLE_LENGTH: u32 = 0x10;
const MAX_SMBIOS_TABLE_LENGTH: u32 = 0x20_000;

#[derive(Debug, Clone, Copy)]
pub struct ResolvedSmbiosOffsets {
    pub build_number: u32,
    pub table_physical_address: u64,
    pub table_length: u64,
}

pub fn resolve_smbios_offsets(
    dma: &Dma<'_>,
    ntoskrnl_base: u64,
    ntoskrnl_size: u32,
) -> Result<ResolvedSmbiosOffsets> {
    let build_number = detect_build_number(dma).unwrap_or(0);

    if build_number > 0 {
        println!("[*] Windows build number: {}", build_number);
    }

    if let Some(candidate) = offsets_for_build(build_number) {
        if validate_offsets(dma, ntoskrnl_base, candidate).is_ok() {
            println!(
                "[+] SMBIOS globals resolved from known offsets for build {}",
                build_number
            );

            return Ok(ResolvedSmbiosOffsets {
                build_number,
                table_physical_address: candidate.table_physical_address,
                table_length: candidate.table_length,
            });
        }

        println!(
            "[!] Known SMBIOS offsets failed validation for build {}, falling back to signature scan",
            build_number
        );
    }

    let ntoskrnl = dma
        .read(KERNEL_PID, ntoskrnl_base, ntoskrnl_size as usize)
        .context("Failed to read ntoskrnl.exe for SMBIOS signature scan")?;

    let scanner = SmbiosPatternScanner::new(&ntoskrnl, ntoskrnl_base, ntoskrnl_size as u64);
    let scanned = scanner.find_offsets()?;

    validate_offsets(dma, ntoskrnl_base, scanned)?;

    println!("[+] SMBIOS globals resolved via ntoskrnl signature scan");

    Ok(ResolvedSmbiosOffsets {
        build_number,
        table_physical_address: scanned.table_physical_address,
        table_length: scanned.table_length,
    })
}

fn detect_build_number(dma: &Dma<'_>) -> Result<u32> {
    if let Ok(build_number) = dma
        .vmm()
        .get_config(memprocfs::CONFIG_OPT_WIN_VERSION_BUILD)
    {
        if build_number > 0 {
            return Ok(build_number as u32);
        }
    }

    let build_number = dma.read_u32(KERNEL_PID, KUSER_SHARED_DATA + NT_BUILD_NUMBER_OFFSET)?;
    Ok(build_number & 0xFFFF)
}

fn validate_offsets(dma: &Dma<'_>, ntoskrnl_base: u64, offsets: SmbiosOffsets) -> Result<()> {
    let physical_address =
        dma.read_u64(KERNEL_PID, ntoskrnl_base + offsets.table_physical_address)?;
    let table_length = dma.read_u32(KERNEL_PID, ntoskrnl_base + offsets.table_length)?;

    if physical_address == 0 {
        bail!("SMBIOS physical address is NULL");
    }

    if !(MIN_SMBIOS_TABLE_LENGTH..=MAX_SMBIOS_TABLE_LENGTH).contains(&table_length) {
        bail!("SMBIOS table length is out of range: {}", table_length);
    }

    let max_native_address = dma
        .vmm()
        .get_config(memprocfs::CONFIG_OPT_CORE_MAX_NATIVE_ADDRESS)
        .unwrap_or(0);

    if max_native_address > 0 && physical_address > max_native_address {
        bail!(
            "SMBIOS physical address 0x{:X} exceeds max native address 0x{:X}",
            physical_address,
            max_native_address
        );
    }

    let header = dma
        .read_phys(physical_address, 4)
        .with_context(|| format!("Failed to read SMBIOS header at 0x{:X}", physical_address))?;

    if header.len() < 4 || header[1] < 4 {
        bail!("SMBIOS header validation failed");
    }

    Ok(())
}

struct SmbiosPatternScanner<'a> {
    data: &'a [u8],
    base: u64,
    size: u64,
}

impl<'a> SmbiosPatternScanner<'a> {
    fn new(data: &'a [u8], base: u64, size: u64) -> Self {
        Self { data, base, size }
    }

    fn find_offsets(&self) -> Result<SmbiosOffsets> {
        let search_limit = self.data.len().saturating_sub(6);

        for offset in 0..search_limit {
            let Some(length_target) = self.read_rip_relative_u32_load(offset) else {
                continue;
            };

            if !self.is_module_address(length_target) {
                continue;
            }

            let Some(second_length_offset) =
                self.find_matching_u32_load(length_target, offset + 6, 0x20)
            else {
                continue;
            };

            let Some((physical_offset, physical_target)) =
                self.find_rip_relative_u64_load(second_length_offset + 6, 0x80)
            else {
                continue;
            };

            if !self.is_module_address(physical_target) || physical_target == length_target {
                continue;
            }

            if self
                .find_matching_u32_load(length_target, physical_offset + 7, 0x30)
                .is_none()
            {
                continue;
            }

            return Ok(SmbiosOffsets::new(
                physical_target - self.base,
                length_target - self.base,
            ));
        }

        Err(anyhow!("Could not locate SMBIOS globals in ntoskrnl.exe"))
    }

    fn find_matching_u32_load(&self, target: u64, start: usize, window: usize) -> Option<usize> {
        let end = start
            .saturating_add(window)
            .min(self.data.len().saturating_sub(6));

        (start..end).find(|&offset| self.read_rip_relative_u32_load(offset) == Some(target))
    }

    fn find_rip_relative_u64_load(&self, start: usize, window: usize) -> Option<(usize, u64)> {
        let end = start
            .saturating_add(window)
            .min(self.data.len().saturating_sub(7));

        for offset in start..end {
            if let Some(target) = self.read_rip_relative_u64_load(offset) {
                return Some((offset, target));
            }
        }

        None
    }

    fn read_rip_relative_u32_load(&self, offset: usize) -> Option<u64> {
        if offset + 6 > self.data.len() || self.data[offset] != 0x8B {
            return None;
        }

        let modrm = self.data[offset + 1];
        if modrm & 0xC7 != 0x05 {
            return None;
        }

        Some(self.relative_target(offset, 6, offset + 2))
    }

    fn read_rip_relative_u64_load(&self, offset: usize) -> Option<u64> {
        if offset + 7 > self.data.len()
            || self.data[offset] != 0x48
            || self.data[offset + 1] != 0x8B
        {
            return None;
        }

        let modrm = self.data[offset + 2];
        if modrm & 0xC7 != 0x05 {
            return None;
        }

        Some(self.relative_target(offset, 7, offset + 3))
    }

    fn relative_target(
        &self,
        offset: usize,
        instruction_length: usize,
        displacement_offset: usize,
    ) -> u64 {
        let displacement = i32::from_le_bytes(
            self.data[displacement_offset..displacement_offset + 4]
                .try_into()
                .unwrap(),
        );

        (self.base as i64)
            .wrapping_add(offset as i64)
            .wrapping_add(instruction_length as i64)
            .wrapping_add(displacement as i64) as u64
    }

    fn is_module_address(&self, address: u64) -> bool {
        address >= self.base && address < self.base + self.size
    }
}
