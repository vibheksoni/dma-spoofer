use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Result};

use crate::core::Dma;
use crate::hwid::{SeedConfig, SerialGenerator};

use super::offsets::*;
use super::resolver::{resolve_volume_context, ResolvedVolumeContext};
use super::types::{MountedDevice, VolumeGuid};

const KERNEL_PID: u32 = 4;
const MIN_KERNEL_POINTER: u64 = 0xFFFF000000000000;

pub struct VolumeSpoofer<'a> {
    dma: &'a Dma<'a>,
    device_extension: u64,
    resolved: ResolvedVolumeContext,
    mounted_devices: Vec<MountedDevice>,
}

impl<'a> VolumeSpoofer<'a> {
    pub fn new(dma: &'a Dma<'a>) -> Result<Self> {
        let module = dma.get_module(KERNEL_PID, "mountmgr.sys")?;
        let resolved = resolve_volume_context(dma)?;

        println!(
            "[+] mountmgr.sys @ 0x{:X} (size: 0x{:X})",
            module.base, module.size
        );
        if resolved.build_number > 0 {
            println!("[+] Volume layout build: {}", resolved.build_number);
        }
        println!("[+] mountmgr DeviceObject @ 0x{:X}", resolved.device_object);

        let device_extension = dma.read_u64(
            KERNEL_PID,
            resolved.device_object + DEVICE_OBJECT_DEVICE_EXTENSION,
        )?;
        if !is_kernel_pointer(device_extension) {
            return Err(anyhow!("Invalid DeviceExtension: 0x{:X}", device_extension));
        }
        println!("[+] DeviceExtension @ 0x{:X}", device_extension);

        let mut spoofer = Self {
            dma,
            device_extension,
            resolved,
            mounted_devices: Vec::new(),
        };

        spoofer.enumerate()?;

        Ok(spoofer)
    }

    fn enumerate(&mut self) -> Result<()> {
        self.mounted_devices.clear();

        let list_head = self.device_extension + self.resolved.layout.extension_mounted_devices_list;
        let first_entry = self
            .dma
            .read_u64(KERNEL_PID, list_head + LIST_ENTRY_FLINK)?;

        if first_entry == 0 || first_entry == list_head {
            println!("[!] Mounted devices list is empty");
            return Ok(());
        }

        let mut current = first_entry;
        let mut count = 0;
        let max_entries = 256;

        while current != list_head && count < max_entries {
            if current == 0 || current < 0xFFFF000000000000 {
                break;
            }

            match self.read_mounted_device(current) {
                Ok(mut device) => {
                    self.enumerate_symbolic_links(&mut device)?;
                    self.mounted_devices.push(device);
                }
                Err(e) => {
                    println!("[!] Failed to read device @ 0x{:X}: {}", current, e);
                }
            }

            let next = self.dma.read_u64(KERNEL_PID, current + LIST_ENTRY_FLINK)?;
            if next == current {
                break;
            }
            current = next;
            count += 1;
        }

        Ok(())
    }

    fn read_mounted_device(&self, entry_addr: u64) -> Result<MountedDevice> {
        let device_name_addr = entry_addr + self.resolved.layout.mounted_device_device_name;
        let mut device_name = self.read_unicode_string(device_name_addr)?;
        if device_name.is_empty() && self.resolved.layout.mounted_device_device_name != 0x50 {
            device_name = self.read_unicode_string(entry_addr + 0x50)?;
        }

        let unique_id =
            if let Some(unique_id_offset) = self.resolved.layout.mounted_device_unique_id {
                let unique_id_ptr = self
                    .dma
                    .read_u64(KERNEL_PID, entry_addr + unique_id_offset)?;

                if is_kernel_pointer(unique_id_ptr) {
                    let id_len = self.dma.read_u16(KERNEL_PID, unique_id_ptr)?;
                    if id_len > 0 && id_len < 256 {
                        let id_buf_ptr = self.dma.read_u64(KERNEL_PID, unique_id_ptr + 8)?;
                        if id_buf_ptr != 0 {
                            self.dma
                                .read(KERNEL_PID, id_buf_ptr, id_len as usize)
                                .unwrap_or_default()
                        } else {
                            Vec::new()
                        }
                    } else {
                        Vec::new()
                    }
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            };

        Ok(MountedDevice::new(entry_addr, device_name, unique_id))
    }

    fn enumerate_symbolic_links(&self, device: &mut MountedDevice) -> Result<()> {
        let mut candidate_offsets = vec![self.resolved.layout.mounted_device_symbolic_links];
        if !candidate_offsets.contains(&0x30) {
            candidate_offsets.push(0x30);
        }

        let mut seen_entries = Vec::new();

        for list_offset in candidate_offsets {
            let list_head = device.entry_addr + list_offset;
            let first_entry = self
                .dma
                .read_u64(KERNEL_PID, list_head + LIST_ENTRY_FLINK)?;

            if first_entry == 0 || first_entry == list_head {
                continue;
            }

            let mut current = first_entry;
            let mut count = 0;
            let max_entries = 64;

            while current != list_head && count < max_entries {
                if current == 0 || current < MIN_KERNEL_POINTER {
                    break;
                }

                if seen_entries.contains(&current) {
                    break;
                }
                seen_entries.push(current);

                match self.read_symbolic_link(current) {
                    Ok(Some(guid)) => {
                        device.add_volume_guid(guid);
                    }
                    Ok(None) => {}
                    Err(_) => {}
                }

                let next = self.dma.read_u64(KERNEL_PID, current + LIST_ENTRY_FLINK)?;
                if next == current {
                    break;
                }
                current = next;
                count += 1;
            }
        }

        Ok(())
    }

    fn read_symbolic_link(&self, entry_addr: u64) -> Result<Option<VolumeGuid>> {
        if let Some(guid) = self.read_direct_symbolic_link(entry_addr)? {
            return Ok(Some(guid));
        }

        self.read_backlinked_symbolic_link(entry_addr)
    }

    fn read_unicode_string(&self, addr: u64) -> Result<String> {
        let length = self
            .dma
            .read_u16(KERNEL_PID, addr + UNICODE_STRING_LENGTH)?;
        let buffer = self
            .dma
            .read_u64(KERNEL_PID, addr + UNICODE_STRING_BUFFER)?;

        if length == 0 || buffer == 0 || buffer < 0xFFFF000000000000 {
            return Ok(String::new());
        }

        let bytes = self.dma.read(KERNEL_PID, buffer, length as usize)?;
        let utf16: Vec<u16> = bytes
            .chunks(2)
            .map(|c| u16::from_le_bytes([c[0], c.get(1).copied().unwrap_or(0)]))
            .collect();

        Ok(String::from_utf16_lossy(&utf16))
    }

    pub fn list(&self) -> Result<()> {
        println!("\n[+] Volume GUIDs:");

        if self.mounted_devices.is_empty() {
            println!("    No mounted devices found");
            return Ok(());
        }

        let mut guid_count = 0;

        for device in &self.mounted_devices {
            if device.volume_guids.is_empty() {
                continue;
            }

            println!("\n    Device: {}", device.device_name);

            for guid in &device.volume_guids {
                let status = if guid.is_active { "active" } else { "inactive" };
                println!("        [{}] {}", status, guid.guid);
                println!("            Path: {}", guid.full_path);
                println!("            BufferAddr: 0x{:X}", guid.name_buffer_addr);
                guid_count += 1;
            }
        }

        if guid_count == 0 {
            println!("    No volume GUIDs found");
        } else {
            println!("\n    Total: {} volume GUIDs", guid_count);
        }

        Ok(())
    }

    pub fn spoof(&self) -> Result<()> {
        let seed_path = Path::new("hwid_seed.json");
        let config = SeedConfig::load(seed_path).unwrap_or_else(|| {
            let seed = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64;
            SeedConfig::new(seed)
        });

        let mut generator = SerialGenerator::from_config(config);
        let mut spoofed_count = 0;

        println!("\n[*] Spoofing volume GUIDs...");

        for device in &self.mounted_devices {
            for guid in &device.volume_guids {
                let new_guid_full = generator.generate_guid();
                let new_guid = new_guid_full
                    .trim_start_matches('{')
                    .trim_end_matches('}')
                    .to_lowercase();
                println!("\n    Old: {}", guid.guid);
                println!("    New: {}", new_guid);

                match self.spoof_guid(guid, &new_guid) {
                    Ok(()) => {
                        spoofed_count += 1;
                        println!("    Status: OK");
                    }
                    Err(e) => {
                        println!("    Status: FAILED - {}", e);
                    }
                }
            }
        }

        if let Err(e) = generator.to_config().save(seed_path) {
            println!("[!] Failed to save seed config: {}", e);
        }

        if spoofed_count == 0 {
            println!("\n[!] No volume GUIDs were spoofed");
        } else {
            println!("\n[+] Spoofed {} volume GUIDs", spoofed_count);
            println!("\n[!] Note: Registry values in HKLM\\SYSTEM\\MountedDevices are NOT patched");
            println!("    These are persistent and may need separate handling");
        }

        Ok(())
    }

    fn spoof_guid(&self, guid: &VolumeGuid, new_guid: &str) -> Result<()> {
        if new_guid.len() != VOLUME_GUID_CHAR_COUNT {
            return Err(anyhow::anyhow!(
                "Invalid GUID length: {} (expected {})",
                new_guid.len(),
                VOLUME_GUID_CHAR_COUNT
            ));
        }

        let guid_offset = VOLUME_GUID_START_OFFSET * 2;
        let write_addr = guid.name_buffer_addr + guid_offset as u64;

        let new_guid_utf16: Vec<u8> = new_guid
            .encode_utf16()
            .flat_map(|c| c.to_le_bytes())
            .collect();

        self.dma.write(KERNEL_PID, write_addr, &new_guid_utf16)?;

        let verify = self
            .dma
            .read(KERNEL_PID, write_addr, new_guid_utf16.len())?;
        if verify != new_guid_utf16 {
            return Err(anyhow::anyhow!("Verification failed"));
        }

        Ok(())
    }

    pub fn refresh(&mut self) -> Result<()> {
        println!("[*] Refreshing volume list...");
        self.enumerate()?;
        println!("[+] Found {} mounted devices", self.mounted_devices.len());
        Ok(())
    }

    pub fn volume_count(&self) -> usize {
        self.mounted_devices
            .iter()
            .map(|d| d.volume_guids.len())
            .sum()
    }
}

fn is_kernel_pointer(address: u64) -> bool {
    address >= MIN_KERNEL_POINTER
}

fn read_symbolic_link_name(
    dma: &Dma<'_>,
    entry_addr: u64,
    name_offset: u64,
) -> Result<Option<(u64, String)>> {
    let name_len = dma.read_u16(KERNEL_PID, entry_addr + name_offset + UNICODE_STRING_LENGTH)?;
    let name_buf = dma.read_u64(KERNEL_PID, entry_addr + name_offset + UNICODE_STRING_BUFFER)?;

    if name_len == 0 || name_buf == 0 || !is_kernel_pointer(name_buf) {
        return Ok(None);
    }

    let name_bytes = dma.read(KERNEL_PID, name_buf, name_len as usize)?;
    let full_path = String::from_utf16_lossy(
        &name_bytes
            .chunks(2)
            .map(|c| u16::from_le_bytes([c[0], c.get(1).copied().unwrap_or(0)]))
            .collect::<Vec<_>>(),
    );

    Ok(Some((name_buf, full_path)))
}

impl<'a> VolumeSpoofer<'a> {
    fn read_direct_symbolic_link(&self, entry_addr: u64) -> Result<Option<VolumeGuid>> {
        let Some((name_buf, full_path)) = read_symbolic_link_name(self.dma, entry_addr, 0x10)?
        else {
            return Ok(None);
        };

        let Some(guid) = extract_volume_guid(&full_path) else {
            return Ok(None);
        };

        let is_active = self.dma.read_u8(KERNEL_PID, entry_addr + 0x20).unwrap_or(1) != 0;
        Ok(Some(VolumeGuid::new(
            entry_addr, name_buf, guid, full_path, is_active,
        )))
    }

    fn read_backlinked_symbolic_link(&self, entry_addr: u64) -> Result<Option<VolumeGuid>> {
        let device_ptr = self
            .dma
            .read_u64(KERNEL_PID, entry_addr + self.resolved.layout.symbolic_link_device)?;
        if !is_kernel_pointer(device_ptr) {
            return Ok(None);
        }

        let Some((name_buf, full_path)) = read_symbolic_link_name(
            self.dma,
            entry_addr,
            self.resolved.layout.symbolic_link_name,
        )?
        else {
            return Ok(None);
        };

        let Some(guid) = extract_volume_guid(&full_path) else {
            return Ok(None);
        };

        Ok(Some(VolumeGuid::new(
            entry_addr, name_buf, guid, full_path, true,
        )))
    }
}

fn extract_volume_guid(path: &str) -> Option<String> {
    let volume_prefix = path.find(VOLUME_GUID_PREFIX)?;
    let guid_start = volume_prefix + VOLUME_GUID_START_OFFSET;
    let guid_end = guid_start + VOLUME_GUID_CHAR_COUNT;

    if path.len() < guid_end {
        return None;
    }

    let guid = &path[guid_start..guid_end];
    if !guid
        .chars()
        .enumerate()
        .all(|(index, ch)| matches_guid_char(index, ch))
    {
        return None;
    }

    Some(guid.to_lowercase())
}

fn matches_guid_char(index: usize, ch: char) -> bool {
    matches!(index, 8 | 13 | 18 | 23) && ch == '-'
        || !matches!(index, 8 | 13 | 18 | 23) && ch.is_ascii_hexdigit()
}
