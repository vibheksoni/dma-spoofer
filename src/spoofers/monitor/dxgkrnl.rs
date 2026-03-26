use anyhow::{bail, Result};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

use crate::core::Dma;

const KERNEL_PID: u32 = 4;

const WIN10_2004_BUILD: u32 = 19041;
const WIN11_21H2_BUILD: u32 = 22000;
const EDID_SIZE: usize = 128;

#[derive(Debug, Clone, Copy)]
struct DxgkrnlEdidLayout {
    dxgglobal_slot_rva: Option<u64>,
    dxgglobal_edid_cache_offset: u64,
    direct_edid_cache_ptr_rva: Option<u64>,
    entry_start_offset: u64,
    entry_size: u64,
    max_entries: usize,
    luid_low_offset: u64,
    luid_high_offset: u64,
    target_id_offset: u64,
    capabilities_origin_offset: u64,
    edid_offset: u64,
}

const WIN10_LAYOUT: DxgkrnlEdidLayout = DxgkrnlEdidLayout {
    dxgglobal_slot_rva: Some(0x0B4198),
    dxgglobal_edid_cache_offset: 0x3F0,
    direct_edid_cache_ptr_rva: None,
    entry_start_offset: 0x00,
    entry_size: 0x98,
    max_entries: 4,
    luid_low_offset: 0x08,
    luid_high_offset: 0x0C,
    target_id_offset: 0x10,
    capabilities_origin_offset: 0x14,
    edid_offset: 0x18,
};

const LEGACY_LAYOUT: DxgkrnlEdidLayout = DxgkrnlEdidLayout {
    dxgglobal_slot_rva: None,
    dxgglobal_edid_cache_offset: 0,
    direct_edid_cache_ptr_rva: Some(0x15F4D8),
    entry_start_offset: 0x10,
    entry_size: 152,
    max_entries: 4,
    luid_low_offset: 0x00,
    luid_high_offset: 0x04,
    target_id_offset: 0x08,
    capabilities_origin_offset: 0x0C,
    edid_offset: 0x10,
};

#[derive(Debug, Clone)]
pub struct CachedEdid {
    pub slot: usize,
    pub luid_low: u32,
    pub luid_high: u32,
    pub target_id: u32,
    pub capabilities_origin: u32,
    pub edid: Vec<u8>,
    pub serial_text: Option<String>,
    pub serial_binary: u32,
    pub manufacturer: String,
}

pub struct DxgkrnlEdidSpoofer<'a> {
    dma: &'a Dma<'a>,
    rng: StdRng,
    layout: DxgkrnlEdidLayout,
    edid_cache_ptr: u64,
}

impl<'a> DxgkrnlEdidSpoofer<'a> {
    pub fn new(dma: &'a Dma<'a>, seed: u64) -> Result<Self> {
        let dxgkrnl_base = Self::find_dxgkrnl_base(dma)?;
        println!("[*] dxgkrnl.sys base: 0x{:X}", dxgkrnl_base);

        let build_number = Self::detect_build_number(dma).unwrap_or(0);
        let layout = Self::layout_for_build(build_number);
        if build_number > 0 {
            println!("[*] dxgkrnl EDID layout build: {}", build_number);
        }

        let edid_cache_ptr = Self::resolve_edid_cache_ptr(dma, dxgkrnl_base, layout)?;

        if edid_cache_ptr == 0 {
            bail!("EDIDCACHE pointer is null - no monitors connected or cache not initialized");
        }

        println!("[*] EDIDCACHE: 0x{:X}", edid_cache_ptr);

        Ok(Self {
            dma,
            rng: StdRng::seed_from_u64(seed),
            layout,
            edid_cache_ptr,
        })
    }

    fn find_dxgkrnl_base(dma: &Dma) -> Result<u64> {
        let module_info = dma.get_module(4, "dxgkrnl.sys")?;
        Ok(module_info.base)
    }

    fn detect_build_number(dma: &Dma) -> Result<u32> {
        if let Ok(build_number) = dma
            .vmm()
            .get_config(memprocfs::CONFIG_OPT_WIN_VERSION_BUILD)
        {
            if build_number > 0 {
                return Ok(build_number as u32);
            }
        }

        let build_number = dma.read_u32(4, 0xFFFFF78000000260)?;
        Ok(build_number & 0xFFFF)
    }

    fn layout_for_build(build_number: u32) -> DxgkrnlEdidLayout {
        match build_number {
            WIN10_2004_BUILD..WIN11_21H2_BUILD => WIN10_LAYOUT,
            _ => LEGACY_LAYOUT,
        }
    }

    fn resolve_edid_cache_ptr(
        dma: &Dma,
        dxgkrnl_base: u64,
        layout: DxgkrnlEdidLayout,
    ) -> Result<u64> {
        if let Some(dxgglobal_slot_rva) = layout.dxgglobal_slot_rva {
            let dxgglobal_slot = dxgkrnl_base + dxgglobal_slot_rva;
            let dxgglobal = dma.read_u64(KERNEL_PID, dxgglobal_slot)?;
            if dxgglobal == 0 {
                bail!("DXGGLOBAL pointer is null");
            }

            println!("[*] DXGGLOBAL: 0x{:X}", dxgglobal);

            let edid_cache_ptr =
                dma.read_u64(KERNEL_PID, dxgglobal + layout.dxgglobal_edid_cache_offset)?;
            return Ok(edid_cache_ptr);
        }

        if let Some(edid_cache_ptr_rva) = layout.direct_edid_cache_ptr_rva {
            let edid_cache_ptr_addr = dxgkrnl_base + edid_cache_ptr_rva;
            return dma.read_u64(KERNEL_PID, edid_cache_ptr_addr);
        }

        bail!("No EDIDCACHE resolver available for this build")
    }

    pub fn list(&self) -> Result<()> {
        let cached_edids = self.enumerate_cached_edids()?;

        if cached_edids.is_empty() {
            println!("[!] No EDIDs found in dxgkrnl cache");
            return Ok(());
        }

        println!(
            "[*] Found {} cached EDID(s) in dxgkrnl:",
            cached_edids.len()
        );
        println!();

        for edid in &cached_edids {
            println!("    Slot #{}:", edid.slot);
            println!(
                "        LUID:         {:08X}:{:08X}",
                edid.luid_high, edid.luid_low
            );
            println!("        TargetId:     {}", edid.target_id);
            println!("        Manufacturer: {}", edid.manufacturer);
            println!("        Serial (bin): {}", edid.serial_binary);
            if let Some(ref text) = edid.serial_text {
                println!("        Serial (txt): {}", text);
            }
            println!();
        }

        Ok(())
    }

    fn enumerate_cached_edids(&self) -> Result<Vec<CachedEdid>> {
        let mut cached = Vec::new();

        for slot in 0..self.layout.max_entries {
            let entry_base = self.edid_cache_ptr
                + self.layout.entry_start_offset
                + (slot as u64 * self.layout.entry_size);

            let luid_low = self
                .dma
                .read_u32(KERNEL_PID, entry_base + self.layout.luid_low_offset)?;
            let luid_high = self
                .dma
                .read_u32(KERNEL_PID, entry_base + self.layout.luid_high_offset)?;
            let target_id = self
                .dma
                .read_u32(KERNEL_PID, entry_base + self.layout.target_id_offset)?;
            let capabilities_origin = self.dma.read_u32(
                KERNEL_PID,
                entry_base + self.layout.capabilities_origin_offset,
            )?;

            if luid_low == 0 && luid_high == 0 {
                continue;
            }

            let edid_addr = entry_base + self.layout.edid_offset;
            let edid_data = self.dma.read(KERNEL_PID, edid_addr, EDID_SIZE)?;

            if !self.is_valid_edid_header(&edid_data) {
                continue;
            }

            let serial_binary =
                u32::from_le_bytes([edid_data[12], edid_data[13], edid_data[14], edid_data[15]]);

            let serial_text = self.extract_text_serial(&edid_data);
            let manufacturer = self.decode_manufacturer_id(&edid_data);

            cached.push(CachedEdid {
                slot,
                luid_low,
                luid_high,
                target_id,
                capabilities_origin,
                edid: edid_data,
                serial_text,
                serial_binary,
                manufacturer,
            });
        }

        Ok(cached)
    }

    pub fn spoof(&mut self) -> Result<()> {
        let cached_edids = self.enumerate_cached_edids()?;

        if cached_edids.is_empty() {
            println!("[!] No EDIDs found in dxgkrnl cache to spoof");
            return Ok(());
        }

        println!(
            "[*] Spoofing {} cached EDID(s) in dxgkrnl...",
            cached_edids.len()
        );
        let mut spoofed_count = 0;

        for edid_info in cached_edids {
            match self.spoof_slot(&edid_info) {
                Ok(new_serial) => {
                    spoofed_count += 1;
                    let old_serial = edid_info
                        .serial_text
                        .clone()
                        .unwrap_or_else(|| edid_info.serial_binary.to_string());
                    println!(
                        "    [+] Slot {} ({}) - Serial: {} -> {}",
                        edid_info.slot, edid_info.manufacturer, old_serial, new_serial
                    );
                }
                Err(e) => {
                    println!("    [!] Failed to spoof slot {}: {}", edid_info.slot, e);
                }
            }
        }

        println!();
        println!("[+] Spoofed {} EDID(s) in dxgkrnl cache", spoofed_count);

        Ok(())
    }

    fn spoof_slot(&mut self, edid_info: &CachedEdid) -> Result<String> {
        let mut spoofed_edid = edid_info.edid.clone();

        let new_serial_bin: u32 = self.rng.gen();
        let serial_bytes = new_serial_bin.to_le_bytes();
        spoofed_edid[12] = serial_bytes[0];
        spoofed_edid[13] = serial_bytes[1];
        spoofed_edid[14] = serial_bytes[2];
        spoofed_edid[15] = serial_bytes[3];

        let new_serial_text = self.generate_text_serial();
        self.update_text_serial(&mut spoofed_edid, &new_serial_text);

        spoofed_edid[16] = self.rng.gen_range(1..=53);

        self.update_checksum(&mut spoofed_edid);

        let entry_base = self.edid_cache_ptr
            + self.layout.entry_start_offset
            + (edid_info.slot as u64 * self.layout.entry_size);
        let edid_addr = entry_base + self.layout.edid_offset;

        self.dma.write(KERNEL_PID, edid_addr, &spoofed_edid)?;

        Ok(new_serial_text)
    }

    fn is_valid_edid_header(&self, edid: &[u8]) -> bool {
        if edid.len() < 8 {
            return false;
        }
        edid[0] == 0x00
            && edid[1] == 0xFF
            && edid[2] == 0xFF
            && edid[3] == 0xFF
            && edid[4] == 0xFF
            && edid[5] == 0xFF
            && edid[6] == 0xFF
            && edid[7] == 0x00
    }

    fn decode_manufacturer_id(&self, edid: &[u8]) -> String {
        if edid.len() < 10 {
            return "???".to_string();
        }
        let manufacturer_id = ((edid[8] as u16) << 8) | (edid[9] as u16);
        let char1 = (((manufacturer_id >> 10) & 0x1F) + 64) as u8 as char;
        let char2 = (((manufacturer_id >> 5) & 0x1F) + 64) as u8 as char;
        let char3 = ((manufacturer_id & 0x1F) + 64) as u8 as char;
        format!("{}{}{}", char1, char2, char3)
    }

    fn extract_text_serial(&self, edid: &[u8]) -> Option<String> {
        if edid.len() < 128 {
            return None;
        }

        for i in (54..126).step_by(18) {
            if i + 17 < edid.len() {
                if edid[i] == 0x00
                    && edid[i + 1] == 0x00
                    && edid[i + 2] == 0x00
                    && edid[i + 3] == 0xFF
                {
                    let mut serial_bytes = Vec::new();
                    for j in 5..18 {
                        if i + j < edid.len() {
                            let byte = edid[i + j];
                            if byte == 0x00 || byte == 0x0A {
                                break;
                            }
                            if byte.is_ascii() && byte != 0x20 {
                                serial_bytes.push(byte);
                            }
                        }
                    }
                    if !serial_bytes.is_empty() {
                        return Some(String::from_utf8_lossy(&serial_bytes).trim().to_string());
                    }
                }
            }
        }
        None
    }

    fn generate_text_serial(&mut self) -> String {
        const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
        (0..10)
            .map(|_| {
                let idx = self.rng.gen_range(0..CHARSET.len());
                CHARSET[idx] as char
            })
            .collect()
    }

    fn update_text_serial(&self, edid: &mut [u8], new_serial: &str) {
        if edid.len() < 128 {
            return;
        }

        for i in (54..126).step_by(18) {
            if i + 17 < edid.len() {
                if edid[i] == 0x00
                    && edid[i + 1] == 0x00
                    && edid[i + 2] == 0x00
                    && edid[i + 3] == 0xFF
                {
                    for j in 5..18 {
                        if i + j < edid.len() {
                            edid[i + j] = 0x0A;
                        }
                    }
                    let serial_bytes = new_serial.as_bytes();
                    let copy_len = serial_bytes.len().min(13);
                    for (j, &byte) in serial_bytes.iter().take(copy_len).enumerate() {
                        if i + 5 + j < edid.len() {
                            edid[i + 5 + j] = byte;
                        }
                    }
                    return;
                }
            }
        }
    }

    fn update_checksum(&self, edid: &mut [u8]) {
        if edid.len() < 128 {
            return;
        }
        let mut checksum: u8 = 0;
        for i in 0..127 {
            checksum = checksum.wrapping_add(edid[i]);
        }
        edid[127] = checksum.wrapping_neg();
    }
}
