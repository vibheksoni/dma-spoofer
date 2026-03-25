use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};

use crate::core::Dma;
use crate::hwid::{SeedConfig, SerialGenerator};

use super::resolver::{resolve_smbios_offsets, ResolvedSmbiosOffsets};
use super::tables::{
    SmbiosHeader, SmbiosTable, TYPE_BASEBOARD, TYPE_BIOS, TYPE_CHASSIS, TYPE_END, TYPE_MEMORY,
    TYPE_PROCESSOR, TYPE_SYSTEM,
};

pub struct SmbiosSpoofer<'a> {
    dma: &'a Dma<'a>,
    ntoskrnl_base: u64,
    offsets: ResolvedSmbiosOffsets,
}

#[derive(Debug, Clone)]
pub struct SmbiosInfo {
    pub physical_address: u64,
    pub length: u32,
    pub tables: Vec<SmbiosTable>,
}

impl<'a> SmbiosSpoofer<'a> {
    pub fn new(dma: &'a Dma<'a>) -> Result<Self> {
        let module = dma
            .get_module(4, "ntoskrnl.exe")
            .context("Failed to find ntoskrnl.exe")?;
        let offsets = resolve_smbios_offsets(dma, module.base, module.size)?;

        println!(
            "[+] Found ntoskrnl.exe @ 0x{:X} (size: 0x{:X})",
            module.base, module.size
        );
        if offsets.build_number > 0 {
            println!("[+] SMBIOS offset profile build: {}", offsets.build_number);
        }
        println!(
            "[+] SMBIOS table globals: physical=ntoskrnl.exe+0x{:X}, length=ntoskrnl.exe+0x{:X}",
            offsets.table_physical_address, offsets.table_length
        );

        Ok(Self {
            dma,
            ntoskrnl_base: module.base,
            offsets,
        })
    }

    fn get_table_physical_address(&self) -> Result<u64> {
        let addr = self.ntoskrnl_base + self.offsets.table_physical_address;
        let phys = self.dma.read_u64(4, addr)?;
        if phys == 0 {
            bail!("SMBIOS physical address is NULL");
        }
        Ok(phys)
    }

    fn get_table_length(&self) -> Result<u32> {
        let addr = self.ntoskrnl_base + self.offsets.table_length;
        self.dma.read_u32(4, addr)
    }

    fn read_smbios_data(&self, phys_addr: u64, length: u32) -> Result<Vec<u8>> {
        self.dma.read_phys(phys_addr, length as usize)
    }

    fn parse_strings(&self, data: &[u8], header_len: usize) -> Vec<String> {
        let mut strings = Vec::new();
        let string_area = &data[header_len..];

        let mut current = String::new();
        for &byte in string_area {
            if byte == 0 {
                if current.is_empty() {
                    break;
                }
                strings.push(current.clone());
                current.clear();
            } else {
                current.push(byte as char);
            }
        }

        strings
    }

    fn calc_table_size(&self, data: &[u8], header_len: u8) -> usize {
        let mut end = header_len as usize;

        while end + 1 < data.len() {
            if data[end] == 0 && data[end + 1] == 0 {
                return end + 2;
            }
            end += 1;
        }

        data.len()
    }

    pub fn enumerate(&self) -> Result<SmbiosInfo> {
        let phys_addr = self.get_table_physical_address()?;
        let length = self.get_table_length()?;

        println!("[*] SMBIOS Physical Address: 0x{:X}", phys_addr);
        println!("[*] SMBIOS Length: {} bytes", length);

        let data = self.read_smbios_data(phys_addr, length)?;
        let mut tables = Vec::new();
        let mut offset = 0usize;

        while offset + 4 < data.len() {
            let header = match SmbiosHeader::from_bytes(&data[offset..]) {
                Some(h) => h,
                None => break,
            };

            if header.table_type == TYPE_END && header.length == 4 {
                break;
            }

            if header.length < 4 {
                break;
            }

            let table_size = self.calc_table_size(&data[offset..], header.length);
            let strings =
                self.parse_strings(&data[offset..offset + table_size], header.length as usize);

            tables.push(SmbiosTable {
                header: header.clone(),
                offset: phys_addr + offset as u64,
                strings,
            });

            offset += table_size;
        }

        Ok(SmbiosInfo {
            physical_address: phys_addr,
            length,
            tables,
        })
    }

    pub fn list(&self) -> Result<()> {
        let info = self.enumerate()?;

        println!("\n[+] Found {} SMBIOS tables:", info.tables.len());

        for table in &info.tables {
            let type_id = table.header.table_type;

            if !matches!(
                type_id,
                TYPE_BIOS
                    | TYPE_SYSTEM
                    | TYPE_BASEBOARD
                    | TYPE_CHASSIS
                    | TYPE_PROCESSOR
                    | TYPE_MEMORY
            ) {
                continue;
            }

            println!(
                "\n    Type {}: {} (Handle: 0x{:04X})",
                type_id,
                table.type_name(),
                table.header.handle
            );

            for (i, s) in table.strings.iter().enumerate() {
                println!("        String {}: {}", i + 1, s);
            }
        }

        Ok(())
    }

    fn spoof_uuid(&self, table: &SmbiosTable, generator: &mut SerialGenerator) -> Result<()> {
        const UUID_OFFSET: u64 = 8;

        let uuid_str = generator.generate_uuid();
        let uuid_bytes: Vec<u8> = uuid_str
            .replace('-', "")
            .chars()
            .collect::<Vec<char>>()
            .chunks(2)
            .filter_map(|chunk| {
                let s: String = chunk.iter().collect();
                u8::from_str_radix(&s, 16).ok()
            })
            .collect();

        if uuid_bytes.len() == 16 {
            self.dma
                .write_phys(table.offset + UUID_OFFSET, &uuid_bytes)?;
        }

        Ok(())
    }

    fn spoof_string(
        &self,
        table: &SmbiosTable,
        string_idx: u8,
        generator: &mut SerialGenerator,
    ) -> Result<()> {
        if string_idx == 0 {
            return Ok(());
        }

        let idx = (string_idx - 1) as usize;
        if idx >= table.strings.len() {
            return Ok(());
        }

        let original = &table.strings[idx];
        let len = original.len();

        if len == 0 {
            return Ok(());
        }

        let mut string_offset = table.header.length as u64;
        for i in 0..idx {
            string_offset += table.strings[i].len() as u64 + 1;
        }

        let spoofed_str = generator.generate_alphanumeric(len);

        self.dma
            .write_phys(table.offset + string_offset, spoofed_str.as_bytes())?;

        Ok(())
    }

    pub fn spoof(&self) -> Result<u32> {
        let seed_path = Path::new("hwid_seed.json");
        let config = SeedConfig::load(seed_path).unwrap_or_else(|| {
            let seed = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64;
            SeedConfig::new(seed)
        });

        let mut generator = SerialGenerator::from_config(config);
        let info = self.enumerate()?;
        let mut spoofed_count = 0u32;

        for table in &info.tables {
            match table.header.table_type {
                TYPE_BIOS => {
                    self.spoof_string(table, 1, &mut generator)?;
                    self.spoof_string(table, 2, &mut generator)?;
                    self.spoof_string(table, 3, &mut generator)?;
                    spoofed_count += 1;
                }
                TYPE_SYSTEM => {
                    self.spoof_string(table, 1, &mut generator)?;
                    self.spoof_string(table, 2, &mut generator)?;
                    self.spoof_string(table, 3, &mut generator)?;
                    self.spoof_string(table, 4, &mut generator)?;
                    self.spoof_uuid(table, &mut generator)?;
                    self.spoof_string(table, 5, &mut generator)?;
                    self.spoof_string(table, 6, &mut generator)?;
                    spoofed_count += 1;
                }
                TYPE_BASEBOARD => {
                    self.spoof_string(table, 1, &mut generator)?;
                    self.spoof_string(table, 2, &mut generator)?;
                    self.spoof_string(table, 3, &mut generator)?;
                    self.spoof_string(table, 4, &mut generator)?;
                    self.spoof_string(table, 5, &mut generator)?;
                    spoofed_count += 1;
                }
                TYPE_CHASSIS => {
                    self.spoof_string(table, 1, &mut generator)?;
                    self.spoof_string(table, 2, &mut generator)?;
                    self.spoof_string(table, 3, &mut generator)?;
                    self.spoof_string(table, 4, &mut generator)?;
                    spoofed_count += 1;
                }
                TYPE_PROCESSOR => {
                    self.spoof_string(table, 2, &mut generator)?;
                    self.spoof_string(table, 3, &mut generator)?;
                    self.spoof_string(table, 4, &mut generator)?;
                    self.spoof_string(table, 5, &mut generator)?;
                    self.spoof_string(table, 6, &mut generator)?;
                    spoofed_count += 1;
                }
                TYPE_MEMORY => {
                    self.spoof_string(table, 1, &mut generator)?;
                    self.spoof_string(table, 2, &mut generator)?;
                    self.spoof_string(table, 3, &mut generator)?;
                    self.spoof_string(table, 4, &mut generator)?;
                    self.spoof_string(table, 5, &mut generator)?;
                    self.spoof_string(table, 6, &mut generator)?;
                    spoofed_count += 1;
                }
                _ => {}
            }
        }

        if let Err(e) = generator.to_config().save(seed_path) {
            println!("[!] Failed to save seed config: {}", e);
        }

        Ok(spoofed_count)
    }
}
