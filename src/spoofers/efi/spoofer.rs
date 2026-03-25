use anyhow::{anyhow, Result};

use super::shellcode::{
    generate_inline_hook_shellcode, generate_jump_to_hook, generate_random_platform_data,
    generate_trampoline_shellcode, string_to_utf16le,
};
use crate::core::Dma;
use crate::utils::codecave::find_best_codecave;

const IMAGE_DOS_SIGNATURE: u16 = 0x5A4D;
const IMAGE_NT_SIGNATURE: u32 = 0x00004550;
const MIN_HOOK_SIZE: usize = 14;
const HOOK_VERIFY_SIZE: usize = 14;

pub struct EfiSpoofer<'a> {
    dma: &'a Dma<'a>,
    ntoskrnl_base: u64,
    ntoskrnl_size: u32,
    hal_efi_table_ptr: u64,
    hal_get_env_var_addr: u64,
    inline_hook_addr: u64,
    hook_len: usize,
    original_bytes: Vec<u8>,
    hook_addr: Option<u64>,
    trampoline_addr: Option<u64>,
    spoofed_data_addr: Option<u64>,
    var_name_addr: Option<u64>,
}

impl<'a> EfiSpoofer<'a> {
    pub fn new(dma: &'a Dma<'a>) -> Result<Self> {
        let module = dma.get_module(4, "ntoskrnl.exe")?;
        let ntoskrnl_base = module.base;
        let ntoskrnl_size = module.size;

        println!(
            "[+] ntoskrnl.exe base: 0x{:X} size: 0x{:X}",
            ntoskrnl_base, ntoskrnl_size
        );

        let hal_efi_table_ptr = Self::find_hal_efi_table(dma, ntoskrnl_base, ntoskrnl_size)?;
        println!(
            "[+] HalEfiRuntimeServicesTable ptr: 0x{:X}",
            hal_efi_table_ptr
        );

        let table_addr = dma.read_u64(4, hal_efi_table_ptr)?;
        if table_addr == 0 {
            return Err(anyhow!(
                "HalEfiRuntimeServicesTable is NULL - not an EFI system?"
            ));
        }
        println!("[+] HalEfiRuntimeServicesTable: 0x{:X}", table_addr);

        let hal_get_env_var_addr =
            Self::find_hal_get_environment_variable_ex(dma, ntoskrnl_base, ntoskrnl_size)?;
        println!(
            "[+] HalGetEnvironmentVariableEx: 0x{:X}",
            hal_get_env_var_addr
        );

        let hook_len = Self::determine_hook_length(dma, hal_get_env_var_addr)?;
        let inline_hook_addr = hal_get_env_var_addr;
        println!("[+] Inline hook location: 0x{:X}", inline_hook_addr);
        println!("[+] Hook length: {} bytes", hook_len);

        Ok(Self {
            dma,
            ntoskrnl_base,
            ntoskrnl_size,
            hal_efi_table_ptr,
            hal_get_env_var_addr,
            inline_hook_addr,
            hook_len,
            original_bytes: Vec::new(),
            hook_addr: None,
            trampoline_addr: None,
            spoofed_data_addr: None,
            var_name_addr: None,
        })
    }

    fn find_hal_efi_table(dma: &Dma, base: u64, size: u32) -> Result<u64> {
        println!("[*] Scanning for HalEfiRuntimeServicesTable...");

        let data = dma.read(4, base, size as usize)?;

        for i in 0..data.len().saturating_sub(15) {
            if data[i] == 0x48 && data[i + 1] == 0x8B && data[i + 2] == 0x05 {
                let has_test = (i + 7..std::cmp::min(i + 20, data.len().saturating_sub(3)))
                    .any(|j| data[j] == 0x48 && data[j + 1] == 0x85 && data[j + 2] == 0xC0);

                if has_test {
                    let rip_offset = i32::from_le_bytes(data[i + 3..i + 7].try_into().unwrap());
                    let rip = base + i as u64 + 7;
                    let target = (rip as i64 + rip_offset as i64) as u64;

                    if let Ok(ptr) = dma.read_u64(4, target) {
                        if ptr > 0xFFFFF80000000000 && ptr < 0xFFFFFFFFFFFFFFFF {
                            if let Ok(func) = dma.read_u64(4, ptr + 24) {
                                if func > 0xFFFFF80000000000 {
                                    return Ok(target);
                                }
                            }
                        }
                    }
                }
            }
        }

        Err(anyhow!("Could not find HalEfiRuntimeServicesTable"))
    }

    fn find_hal_get_environment_variable_ex(dma: &Dma, base: u64, size: u32) -> Result<u64> {
        println!("[*] Resolving HalGetEnvironmentVariableEx...");
        let data = dma.read(4, base, size as usize)?;

        if let Ok(rva) = Self::find_export_rva(&data, "HalGetEnvironmentVariableEx") {
            return Ok(base + rva as u64);
        }

        let pattern: [u8; 16] = [
            0x40, 0x55, 0x53, 0x56, 0x57, 0x41, 0x54, 0x41, 0x55, 0x41, 0x56, 0x41, 0x57, 0x48,
            0x83, 0xEC,
        ];

        for i in 0..data.len().saturating_sub(pattern.len()) {
            if data[i..i + pattern.len()] == pattern {
                return Ok(base + i as u64);
            }
        }

        Err(anyhow!("Could not resolve HalGetEnvironmentVariableEx"))
    }

    fn determine_hook_length(dma: &Dma, func_addr: u64) -> Result<usize> {
        let data = dma.read(4, func_addr, 0x40)?;
        let mut offset = 0usize;

        while offset < data.len() && offset < 0x20 {
            let len = Self::instruction_length(&data, offset)?;
            offset += len;

            if offset >= MIN_HOOK_SIZE {
                return Ok(offset);
            }
        }

        Err(anyhow!("Could not determine EFI hook length"))
    }

    fn find_export_rva(pe_data: &[u8], export_name: &str) -> Result<u32> {
        if pe_data.len() < 0x200 {
            return Err(anyhow!("PE data too small"));
        }

        let dos_sig = u16::from_le_bytes([pe_data[0], pe_data[1]]);
        if dos_sig != IMAGE_DOS_SIGNATURE {
            return Err(anyhow!("Invalid DOS signature"));
        }

        let e_lfanew =
            u32::from_le_bytes([pe_data[0x3C], pe_data[0x3D], pe_data[0x3E], pe_data[0x3F]])
                as usize;
        if e_lfanew + 0x18 > pe_data.len() {
            return Err(anyhow!("Invalid PE header offset"));
        }

        let nt_sig = u32::from_le_bytes([
            pe_data[e_lfanew],
            pe_data[e_lfanew + 1],
            pe_data[e_lfanew + 2],
            pe_data[e_lfanew + 3],
        ]);
        if nt_sig != IMAGE_NT_SIGNATURE {
            return Err(anyhow!("Invalid NT signature"));
        }

        let optional_header_offset = e_lfanew + 0x18;
        let export_dir_offset = optional_header_offset + 0x70;
        if export_dir_offset + 8 > pe_data.len() {
            return Err(anyhow!("Export directory offset out of bounds"));
        }

        let export_rva = u32::from_le_bytes([
            pe_data[export_dir_offset],
            pe_data[export_dir_offset + 1],
            pe_data[export_dir_offset + 2],
            pe_data[export_dir_offset + 3],
        ]) as usize;
        if export_rva == 0 || export_rva + 0x28 > pe_data.len() {
            return Err(anyhow!("No export directory"));
        }

        let num_names = u32::from_le_bytes([
            pe_data[export_rva + 0x18],
            pe_data[export_rva + 0x19],
            pe_data[export_rva + 0x1A],
            pe_data[export_rva + 0x1B],
        ]) as usize;
        let addr_table_rva = u32::from_le_bytes([
            pe_data[export_rva + 0x1C],
            pe_data[export_rva + 0x1D],
            pe_data[export_rva + 0x1E],
            pe_data[export_rva + 0x1F],
        ]) as usize;
        let name_ptr_table_rva = u32::from_le_bytes([
            pe_data[export_rva + 0x20],
            pe_data[export_rva + 0x21],
            pe_data[export_rva + 0x22],
            pe_data[export_rva + 0x23],
        ]) as usize;
        let ordinal_table_rva = u32::from_le_bytes([
            pe_data[export_rva + 0x24],
            pe_data[export_rva + 0x25],
            pe_data[export_rva + 0x26],
            pe_data[export_rva + 0x27],
        ]) as usize;

        for i in 0..num_names {
            let name_rva_offset = name_ptr_table_rva + (i * 4);
            if name_rva_offset + 4 > pe_data.len() {
                continue;
            }

            let name_rva = u32::from_le_bytes([
                pe_data[name_rva_offset],
                pe_data[name_rva_offset + 1],
                pe_data[name_rva_offset + 2],
                pe_data[name_rva_offset + 3],
            ]) as usize;
            if name_rva >= pe_data.len() {
                continue;
            }

            let mut name_end = name_rva;
            while name_end < pe_data.len() && pe_data[name_end] != 0 {
                name_end += 1;
            }

            let name = std::str::from_utf8(&pe_data[name_rva..name_end]).unwrap_or("");
            if name != export_name {
                continue;
            }

            let ordinal_offset = ordinal_table_rva + (i * 2);
            if ordinal_offset + 2 > pe_data.len() {
                return Err(anyhow!("Ordinal table out of bounds"));
            }

            let ordinal =
                u16::from_le_bytes([pe_data[ordinal_offset], pe_data[ordinal_offset + 1]]) as usize;
            let func_rva_offset = addr_table_rva + (ordinal * 4);
            if func_rva_offset + 4 > pe_data.len() {
                return Err(anyhow!("Function address table out of bounds"));
            }

            return Ok(u32::from_le_bytes([
                pe_data[func_rva_offset],
                pe_data[func_rva_offset + 1],
                pe_data[func_rva_offset + 2],
                pe_data[func_rva_offset + 3],
            ]));
        }

        Err(anyhow!("Export '{}' not found", export_name))
    }

    fn instruction_length(data: &[u8], offset: usize) -> Result<usize> {
        if offset >= data.len() {
            return Err(anyhow!("Hook decode offset out of bounds"));
        }

        let opcode = data[offset];

        match opcode {
            0x50..=0x5F => Ok(1),
            0x41 => {
                if offset + 1 >= data.len() {
                    return Err(anyhow!("Truncated REX-prefixed instruction"));
                }

                match data[offset + 1] {
                    0x50..=0x5F => Ok(2),
                    _ => Ok(2),
                }
            }
            0x48 => {
                if offset + 2 >= data.len() {
                    return Err(anyhow!("Truncated 0x48-prefixed instruction"));
                }

                match data[offset + 1] {
                    0x83 => Ok(4),
                    0x81 => Ok(7),
                    0x89 | 0x8B | 0x8D => Ok(Self::calc_modrm_length(data[offset + 2], 2)),
                    0xB8..=0xBF => Ok(10),
                    _ => Ok(3),
                }
            }
            _ => Ok(1),
        }
    }

    fn calc_modrm_length(modrm: u8, prefix_len: usize) -> usize {
        let mod_bits = (modrm >> 6) & 0x03;
        let rm = modrm & 0x07;

        let mut len = prefix_len + 1;

        if rm == 4 && mod_bits != 3 {
            len += 1;
        }

        match mod_bits {
            0 => {
                if rm == 5 {
                    len += 4;
                }
            }
            1 => len += 1,
            2 => len += 4,
            _ => {}
        }

        len
    }

    pub fn list(&self) -> Result<()> {
        let table_addr = self.dma.read_u64(4, self.hal_efi_table_ptr)?;

        println!("\n[*] EFI Runtime Services Table @ 0x{:X}", table_addr);
        println!("{:-<60}", "");

        let get_time = self.dma.read_u64(4, table_addr + 16)?;
        let get_var = self.dma.read_u64(4, table_addr + 24)?;
        let get_next = self.dma.read_u64(4, table_addr + 32)?;
        let set_var = self.dma.read_u64(4, table_addr + 40)?;

        println!("  [2] GetTime:              0x{:X}", get_time);
        println!("  [3] GetVariable:          0x{:X}", get_var);
        println!("  [4] GetNextVariableName:  0x{:X}", get_next);
        println!("  [5] SetVariable:          0x{:X}", set_var);
        println!("{:-<60}", "");

        println!("\n[*] Inline Hook Info:");
        println!(
            "  HalGetEnvironmentVariableEx: 0x{:X}",
            self.hal_get_env_var_addr
        );
        println!(
            "  Hook location:               0x{:X}",
            self.inline_hook_addr
        );
        println!("  Hook length:                 {} bytes", self.hook_len);

        if self.hook_addr.is_some() {
            println!("[!] Hook is currently ACTIVE");
        }

        Ok(())
    }

    pub fn spoof(&mut self) -> Result<()> {
        let vmm = self.dma.vmm();

        println!("[*] Finding codecave for hook...");
        let codecave = find_best_codecave(vmm, 512)?;

        println!(
            "[+] Using codecave at 0x{:X} ({} bytes)",
            codecave.address, codecave.size
        );

        let spoofed_data = generate_random_platform_data();
        let spoofed_data_len = spoofed_data.len() as u32;

        let var_name = string_to_utf16le("PlatformData");
        let var_name_len = (var_name.len() / 2) as u16;

        let trampoline_addr = codecave.address;
        let spoofed_data_addr = codecave.address + 0x80;
        let var_name_addr = codecave.address + 0x100;
        let hook_code_addr = codecave.address + 0x140;

        let original_bytes = self.dma.read(4, self.inline_hook_addr, self.hook_len)?;
        self.original_bytes = original_bytes;
        println!(
            "[+] Saved original bytes: {:02X?}",
            &self.original_bytes[..self.original_bytes.len().min(9)]
        );

        let trampoline = generate_trampoline_shellcode(
            &self.original_bytes,
            self.inline_hook_addr + self.hook_len as u64,
        );
        self.dma.write(4, trampoline_addr, &trampoline)?;
        println!(
            "[+] Trampoline ({} bytes) written to 0x{:X}",
            trampoline.len(),
            trampoline_addr
        );

        self.dma.write(4, spoofed_data_addr, &spoofed_data)?;
        println!("[+] Spoofed data written to 0x{:X}", spoofed_data_addr);

        self.dma.write(4, var_name_addr, &var_name)?;
        println!("[+] Variable name written to 0x{:X}", var_name_addr);

        let hook_code = generate_inline_hook_shellcode(
            trampoline_addr,
            spoofed_data_addr,
            spoofed_data_len,
            var_name_addr,
            var_name_len,
        );

        self.dma.write(4, hook_code_addr, &hook_code)?;
        println!(
            "[+] Hook shellcode ({} bytes) written to 0x{:X}",
            hook_code.len(),
            hook_code_addr
        );

        let jump_stub = generate_jump_to_hook(hook_code_addr);

        println!(
            "[*] Installing inline hook at 0x{:X}...",
            self.inline_hook_addr
        );
        self.dma
            .write(4, self.inline_hook_addr, &jump_stub[..HOOK_VERIFY_SIZE])?;

        let verify = self.dma.read(4, self.inline_hook_addr, HOOK_VERIFY_SIZE)?;
        if verify != jump_stub[..HOOK_VERIFY_SIZE] {
            return Err(anyhow!("Failed to install inline hook"));
        }

        self.hook_addr = Some(hook_code_addr);
        self.trampoline_addr = Some(trampoline_addr);
        self.spoofed_data_addr = Some(spoofed_data_addr);
        self.var_name_addr = Some(var_name_addr);

        println!("[+] Inline hook installed!");
        println!("[+] PlatformData will return spoofed data");
        println!("[+] CFG bypass: YES (no indirect call modification)");

        Ok(())
    }

    pub fn remove_hook(&mut self) -> Result<()> {
        if self.hook_addr.is_none() {
            println!("[*] No hook installed");
            return Ok(());
        }

        println!(
            "[*] Restoring original bytes at 0x{:X}...",
            self.inline_hook_addr
        );
        self.dma
            .write(4, self.inline_hook_addr, &self.original_bytes)?;

        let restored = self
            .dma
            .read(4, self.inline_hook_addr, self.original_bytes.len())?;
        if restored != self.original_bytes {
            return Err(anyhow!("Failed to restore original bytes"));
        }

        self.hook_addr = None;
        self.trampoline_addr = None;
        self.spoofed_data_addr = None;
        self.var_name_addr = None;

        println!("[+] Hook removed, original code restored");

        Ok(())
    }

    pub fn is_efi_available(&self) -> bool {
        self.dma
            .read_u64(4, self.hal_efi_table_ptr)
            .map(|ptr| ptr != 0)
            .unwrap_or(false)
    }
}
