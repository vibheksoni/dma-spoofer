use anyhow::{bail, Context, Result};

use crate::core::Dma;

use super::offsets;
use super::uuid::GpuUuid;

#[derive(Debug, Clone)]
pub struct UuidCandidate {
    pub offset: u64,
    pub uuid: GpuUuid,
    pub confidence: UuidConfidence,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UuidConfidence {
    Exact,
    High,
    Medium,
    Low,
}

pub struct NvidiaSpoofer<'a> {
    dma: &'a Dma<'a>,
    driver_base: u64,
    driver_size: u64,
}

pub struct GpuDevice {
    pub index: u32,
    pub address: u64,
    pub uuid: GpuUuid,
    pub uuid_offset: Option<u64>,
}

impl<'a> NvidiaSpoofer<'a> {
    pub fn new(dma: &'a Dma<'a>) -> Result<Self> {
        let module = dma
            .get_module(4, "nvlddmkm.sys")
            .context("Failed to find nvlddmkm.sys - is NVIDIA driver loaded?")?;

        println!(
            "[+] Found nvlddmkm.sys @ 0x{:X} (size: 0x{:X})",
            module.base, module.size
        );

        Ok(Self {
            dma,
            driver_base: module.base,
            driver_size: module.size as u64,
        })
    }

    pub fn driver_base(&self) -> u64 {
        self.driver_base
    }

    pub fn driver_size(&self) -> u64 {
        self.driver_size
    }

    pub fn enumerate(&self) -> Result<Vec<GpuDevice>> {
        if let Ok(devices) = self.enumerate_from_gpu_manager_array() {
            if !devices.is_empty() {
                println!("[+] Found {} GPU(s) via GPU manager array", devices.len());
                return Ok(devices);
            }
        }

        if let Ok(devices) = self.enumerate_from_device_list() {
            if !devices.is_empty() {
                println!("[+] Found {} GPU(s) via device list", devices.len());
                return Ok(devices);
            }
        }

        println!("[*] Falling back to legacy enumeration method...");
        let mut devices = Vec::new();

        if let Some(device_ctx) = self.get_first_device_context()? {
            if let Ok(gpu_mgr) = self.get_gpu_manager(device_ctx) {
                println!("[*] GPU Manager: 0x{:X}", gpu_mgr);

                if let Ok(gpu_count) = self.get_gpu_count(gpu_mgr) {
                    println!("[*] GPU count from manager: {}", gpu_count);

                    for idx in 0..gpu_count.min(32) {
                        if let Ok(addr) = self.get_gpu_object(gpu_mgr, idx) {
                            if self.is_valid_kernel_ptr(addr) {
                                println!("[*] GPU {} object: 0x{:X}", idx, addr);
                                devices.push(GpuDevice {
                                    index: idx,
                                    address: addr,
                                    uuid: GpuUuid::new_uninit(),
                                    uuid_offset: None,
                                });
                            }
                        }
                    }
                }
            }
        }

        Ok(devices)
    }

    pub fn find_uuid_by_signature(
        &self,
        gpu_addr: u64,
        known_uuid: &[u8; 16],
    ) -> Result<Option<(u64, u64)>> {
        println!("[*] Scanning GPU object @ 0x{:X}...", gpu_addr);

        let gpu_data = self.dma.read(4, gpu_addr, offsets::GPU_UUID_SEARCH_SIZE)?;

        println!("[*] Checking known offsets...");
        for &offset in offsets::KNOWN_UUID_OFFSETS {
            let idx = offset as usize;
            if idx + 16 <= gpu_data.len() {
                if &gpu_data[idx..idx + 16] == known_uuid {
                    println!("[+] Found at known offset 0x{:X}", offset);
                    return Ok(Some((gpu_addr, offset)));
                }
                if idx > 0 && gpu_data[idx - 1] == 1 && &gpu_data[idx..idx + 16] == known_uuid {
                    println!(
                        "[+] Found at known offset 0x{:X} (with isInitialized flag)",
                        offset
                    );
                    return Ok(Some((gpu_addr, offset)));
                }
            }
        }

        println!("[*] Full scanning GPU object memory...");
        for i in 0..gpu_data.len().saturating_sub(16) {
            if &gpu_data[i..i + 16] == known_uuid {
                println!("[+] Found at offset 0x{:X}", i);
                return Ok(Some((gpu_addr, i as u64)));
            }
        }

        println!("[*] UUID not in GPU object, scanning child structures...");
        let mut checked_ptrs = std::collections::HashSet::new();

        for i in (0..gpu_data.len().saturating_sub(8)).step_by(8) {
            let ptr = u64::from_le_bytes(gpu_data[i..i + 8].try_into().unwrap_or([0; 8]));

            if !self.is_valid_kernel_ptr(ptr) || checked_ptrs.contains(&ptr) {
                continue;
            }
            checked_ptrs.insert(ptr);

            if let Ok(child_data) = self.dma.read(4, ptr, 0x1000) {
                for j in 0..child_data.len().saturating_sub(16) {
                    if &child_data[j..j + 16] == known_uuid {
                        println!(
                            "[+] Found via pointer at GPU+0x{:X} -> 0x{:X} +0x{:X}",
                            i, ptr, j
                        );
                        return Ok(Some((ptr, j as u64)));
                    }
                }
            }
        }

        println!("[*] Scanning deeper (level 2 pointers)...");
        for i in (0..gpu_data.len().saturating_sub(8)).step_by(8) {
            let ptr1 = u64::from_le_bytes(gpu_data[i..i + 8].try_into().unwrap_or([0; 8]));

            if !self.is_valid_kernel_ptr(ptr1) {
                continue;
            }

            if let Ok(child_data) = self.dma.read(4, ptr1, 0x800) {
                for j in (0..child_data.len().saturating_sub(8)).step_by(8) {
                    let ptr2 =
                        u64::from_le_bytes(child_data[j..j + 8].try_into().unwrap_or([0; 8]));

                    if !self.is_valid_kernel_ptr(ptr2) || checked_ptrs.contains(&ptr2) {
                        continue;
                    }
                    checked_ptrs.insert(ptr2);

                    if let Ok(data2) = self.dma.read(4, ptr2, 0x400) {
                        for k in 0..data2.len().saturating_sub(16) {
                            if &data2[k..k + 16] == known_uuid {
                                println!(
                                    "[+] Found via GPU+0x{:X} -> +0x{:X} -> 0x{:X} +0x{:X}",
                                    i, j, ptr2, k
                                );
                                return Ok(Some((ptr2, k as u64)));
                            }
                        }
                    }
                }
            }
        }

        Ok(None)
    }

    pub fn check_known_offsets(&self, gpu_addr: u64) -> Result<Vec<(u64, GpuUuid)>> {
        let gpu_data = self.dma.read(4, gpu_addr, offsets::GPU_UUID_SEARCH_SIZE)?;
        let mut results = Vec::new();

        for &offset in offsets::KNOWN_UUID_OFFSETS {
            let idx = offset as usize;

            if idx > 0 && idx + 16 <= gpu_data.len() {
                let is_init = gpu_data[idx - 1];
                if is_init == 1 {
                    let uuid_bytes: [u8; 16] =
                        gpu_data[idx..idx + 16].try_into().unwrap_or([0; 16]);
                    if !self.all_zero(&uuid_bytes) && !self.looks_like_pointer(&uuid_bytes) {
                        results.push((offset, GpuUuid::from_bytes(uuid_bytes)));
                    }
                }
            }

            if idx + 17 <= gpu_data.len() {
                let is_init = gpu_data[idx];
                if is_init == 1 {
                    let uuid_bytes: [u8; 16] =
                        gpu_data[idx + 1..idx + 17].try_into().unwrap_or([0; 16]);
                    if !self.all_zero(&uuid_bytes) && !self.looks_like_pointer(&uuid_bytes) {
                        results.push((offset, GpuUuid::from_bytes(uuid_bytes)));
                    }
                }
            }
        }

        Ok(results)
    }

    pub fn find_uuid_candidates(&self, gpu_addr: u64) -> Result<Vec<UuidCandidate>> {
        let data = self.dma.read(4, gpu_addr, offsets::GPU_UUID_SEARCH_SIZE)?;
        let mut candidates = Vec::new();

        for i in 0..data.len().saturating_sub(17) {
            let uuid_bytes: [u8; 16] = match data[i..i + 16].try_into() {
                Ok(arr) => arr,
                Err(_) => continue,
            };

            if let Some(confidence) = self.check_uuid_confidence(&data, i) {
                candidates.push(UuidCandidate {
                    offset: i as u64,
                    uuid: GpuUuid::from_bytes(uuid_bytes),
                    confidence,
                });
            }
        }

        candidates.sort_by(|a, b| {
            let ord_a = match a.confidence {
                UuidConfidence::Exact => 0,
                UuidConfidence::High => 1,
                UuidConfidence::Medium => 2,
                UuidConfidence::Low => 3,
            };
            let ord_b = match b.confidence {
                UuidConfidence::Exact => 0,
                UuidConfidence::High => 1,
                UuidConfidence::Medium => 2,
                UuidConfidence::Low => 3,
            };
            ord_a.cmp(&ord_b)
        });

        Ok(candidates)
    }

    fn check_uuid_confidence(&self, data: &[u8], offset: usize) -> Option<UuidConfidence> {
        let uuid_bytes = &data[offset..offset + 16];

        if self.all_zero(uuid_bytes) || self.all_same(uuid_bytes) {
            return None;
        }

        if self.looks_like_pointer(uuid_bytes) {
            return None;
        }

        let unique = self.unique_byte_count(uuid_bytes);
        if unique < 6 {
            return None;
        }

        if offset > 0 && data[offset - 1] == 1 {
            let before_flag = &data[offset.saturating_sub(17)..offset - 1];
            let zeros_before = before_flag.iter().filter(|&&b| b == 0).count();
            if zeros_before >= 8 {
                return Some(UuidConfidence::High);
            }
            return Some(UuidConfidence::Medium);
        }

        if unique >= 10 {
            let after_end = (offset + 32).min(data.len());
            let after = &data[offset + 16..after_end];
            let zeros_after = after.iter().filter(|&&b| b == 0 || b == 0xff).count();
            if zeros_after >= 8 {
                return Some(UuidConfidence::Medium);
            }
        }

        if unique >= 8 {
            return Some(UuidConfidence::Low);
        }

        None
    }

    fn all_zero(&self, data: &[u8]) -> bool {
        data.iter().all(|&b| b == 0)
    }

    fn all_same(&self, data: &[u8]) -> bool {
        if data.is_empty() {
            return true;
        }
        let first = data[0];
        data.iter().all(|&b| b == first)
    }

    fn looks_like_pointer(&self, data: &[u8]) -> bool {
        if data.len() < 8 {
            return false;
        }
        let val = u64::from_le_bytes(data[0..8].try_into().unwrap_or([0; 8]));
        val > 0xFFFF000000000000 && val < 0xFFFFFFFFFFFFFFFF
    }

    fn unique_byte_count(&self, data: &[u8]) -> usize {
        let mut seen = [false; 256];
        for &b in data {
            seen[b as usize] = true;
        }
        seen.iter().filter(|&&x| x).count()
    }

    pub fn read_uuid_at(&self, gpu_addr: u64, offset: u64) -> Result<GpuUuid> {
        let data = self
            .dma
            .read(4, gpu_addr + offset, offsets::GPU_UUID_SIZE)?;
        let mut uuid = [0u8; 16];
        uuid.copy_from_slice(&data);
        Ok(GpuUuid::from_bytes(uuid))
    }

    pub fn write_uuid_at(&self, gpu_addr: u64, offset: u64, uuid: &[u8; 16]) -> Result<()> {
        self.dma.write(4, gpu_addr + offset, uuid)?;
        Ok(())
    }

    pub fn read_uuid(&self, gpu_addr: u64) -> Result<GpuUuid> {
        let candidates = self.find_uuid_candidates(gpu_addr)?;
        if let Some(first) = candidates.first() {
            Ok(first.uuid.clone())
        } else {
            Ok(GpuUuid::new_uninit())
        }
    }

    pub fn write_uuid(&self, gpu_addr: u64, uuid: &[u8; 16]) -> Result<()> {
        let candidates = self.find_uuid_candidates(gpu_addr)?;
        if let Some(first) = candidates.first() {
            self.write_uuid_at(gpu_addr, first.offset, uuid)
        } else {
            bail!("Could not find UUID offset in GPU object")
        }
    }

    fn find_list_head(&self) -> Result<u64> {
        println!("[*] Scanning for ListHead global variable...");

        let pattern: &[u8] = &[0x48, 0x8B, 0x15];

        let scan_size = 0x800000;
        let data = self.dma.read(4, self.driver_base, scan_size)?;

        for i in 0..data.len().saturating_sub(7) {
            if &data[i..i + 3] != pattern {
                continue;
            }

            let rip_offset = i32::from_le_bytes(data[i + 3..i + 7].try_into().unwrap());
            let rip = self.driver_base + i as u64 + 7;
            let target_addr = (rip as i64 + rip_offset as i64) as u64;

            if target_addr < self.driver_base || target_addr >= self.driver_base + self.driver_size
            {
                continue;
            }

            if let Ok(flink) = self.dma.read_u64(4, target_addr) {
                if let Ok(blink) = self.dma.read_u64(4, target_addr + 8) {
                    if self.is_valid_kernel_ptr(flink) && self.is_valid_kernel_ptr(blink) {
                        if flink != target_addr && blink != target_addr {
                            println!("[+] Found potential ListHead @ 0x{:X} (Flink: 0x{:X}, Blink: 0x{:X})",
                                target_addr, flink, blink);
                            return Ok(target_addr);
                        }
                    }
                }
            }
        }

        bail!("Could not find ListHead via pattern scan")
    }

    fn enumerate_from_device_list(&self) -> Result<Vec<GpuDevice>> {
        const DEVICE_OBJECT_OFFSET: i64 = -0x4028;
        const DEVICE_EXTENSION_OFFSET: u64 = 0x40;

        println!("[*] Enumerating GPUs via device list...");

        let list_head_addr = self.find_list_head()?;

        let flink = self.dma.read_u64(4, list_head_addr)?;
        let blink = self.dma.read_u64(4, list_head_addr + 8)?;

        println!("[*] ListHead.Flink = 0x{:X}", flink);
        println!("[*] ListHead.Blink = 0x{:X}", blink);

        if flink == list_head_addr {
            bail!("ListHead is empty (Flink points to self)");
        }

        if !self.is_valid_kernel_ptr(flink) {
            bail!("Invalid Flink pointer: 0x{:X}", flink);
        }

        let mut devices = Vec::new();
        let mut current = flink;
        let mut visited = std::collections::HashSet::new();

        while current != list_head_addr && visited.len() < 32 {
            if visited.contains(&current) {
                println!("[!] Circular reference detected at 0x{:X}", current);
                break;
            }
            visited.insert(current);

            println!("[*] List entry @ 0x{:X}", current);

            let device_obj_addr = (current as i64 + DEVICE_OBJECT_OFFSET) as u64;
            println!("[*] DEVICE_OBJECT @ 0x{:X}", device_obj_addr);

            if self.is_valid_kernel_ptr(device_obj_addr) {
                if let Ok(device_extension_ptr) = self
                    .dma
                    .read_u64(4, device_obj_addr + DEVICE_EXTENSION_OFFSET)
                {
                    println!("[*] DeviceExtension @ 0x{:X}", device_extension_ptr);

                    if self.is_valid_kernel_ptr(device_extension_ptr) {
                        if let Ok(ext_data) = self.dma.read(4, device_extension_ptr, 0x200) {
                            for offset in (0..0x200).step_by(8) {
                                let ptr = u64::from_le_bytes(
                                    ext_data[offset..offset + 8].try_into().unwrap(),
                                );

                                if !self.is_valid_kernel_ptr(ptr) {
                                    continue;
                                }

                                if let Ok(test_data) = self.dma.read(4, ptr, 0x100) {
                                    let mut valid_ptr_count = 0;
                                    for j in (0..0x100).step_by(8) {
                                        let test_ptr = u64::from_le_bytes(
                                            test_data[j..j + 8].try_into().unwrap(),
                                        );
                                        if self.is_valid_kernel_ptr(test_ptr) {
                                            valid_ptr_count += 1;
                                        }
                                    }

                                    if valid_ptr_count >= 10 {
                                        println!("[+] Found potential GPU object @ 0x{:X} (via DeviceExtension+0x{:X}, {} valid ptrs)",
                                            ptr, offset, valid_ptr_count);

                                        if !devices.iter().any(|d: &GpuDevice| d.address == ptr) {
                                            devices.push(GpuDevice {
                                                index: devices.len() as u32,
                                                address: ptr,
                                                uuid: GpuUuid::new_uninit(),
                                                uuid_offset: None,
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            match self.dma.read_u64(4, current) {
                Ok(next_flink) => {
                    println!("[*] Next Flink: 0x{:X}", next_flink);
                    if !self.is_valid_kernel_ptr(next_flink) {
                        break;
                    }
                    current = next_flink;
                }
                Err(e) => {
                    println!("[!] Failed to read next Flink: {}", e);
                    break;
                }
            }
        }

        if devices.is_empty() {
            bail!("No GPU objects found via device list");
        }

        Ok(devices)
    }

    fn find_gpu_manager_array(&self) -> Result<u64> {
        println!("[*] Scanning for GPU manager array...");

        let scan_size = 0x800000;
        let data = self.dma.read(4, self.driver_base, scan_size)?;

        for i in 0..data.len().saturating_sub(11) {
            let (disp_offset, rip_offset_add) = if i + 16 <= data.len()
                && data[i] == 0x33
                && data[i + 1] == 0xC9
                && data[i + 2] == 0x4C
                && data[i + 3] == 0x8D
                && data[i + 4] == 0x05
                && data[i + 9] == 0x0F
                && data[i + 10] == 0x1F
                && data[i + 11] == 0x00
                && data[i + 12] == 0x49
                && data[i + 13] == 0x8B
                && data[i + 14] == 0x1C
                && data[i + 15] == 0xC8
            {
                (5usize, 9u64)
            } else if data[i] == 0x4C
                && data[i + 1] == 0x8D
                && data[i + 2] == 0x05
                && data[i + 7] == 0x49
                && data[i + 8] == 0x8B
                && data[i + 9] == 0x1C
                && data[i + 10] == 0xC8
            {
                (3usize, 7u64)
            } else {
                continue;
            };

            let rip_offset = i32::from_le_bytes(
                data[i + disp_offset..i + disp_offset + 4]
                    .try_into()
                    .unwrap(),
            );
            let rip = self.driver_base + i as u64 + rip_offset_add;
            let array_addr = (rip as i64 + rip_offset as i64) as u64;

            if array_addr < self.driver_base || array_addr >= self.driver_base + self.driver_size {
                continue;
            }

            println!(
                "[+] Found GPU manager array @ 0x{:X} (pattern at offset 0x{:X})",
                array_addr, i
            );
            return Ok(array_addr);
        }

        bail!("Could not find GPU manager array via pattern scan")
    }

    fn enumerate_from_gpu_manager_array(&self) -> Result<Vec<GpuDevice>> {
        let array_addr = self.find_gpu_manager_array()?;
        let mut devices = Vec::new();

        for i in 0..32 {
            let gpu_mgr_ptr = self.dma.read_u64(4, array_addr + (i * 8))?;

            if !self.is_valid_kernel_ptr(gpu_mgr_ptr) {
                continue;
            }

            println!("[+] Found GPU manager @ 0x{:X} (index {})", gpu_mgr_ptr, i);

            let mgr_data = self.dma.read(4, gpu_mgr_ptr, 0x1000)?;

            for offset in (0..0x1000).step_by(8) {
                if offset + 8 > mgr_data.len() {
                    break;
                }

                let ptr = u64::from_le_bytes(mgr_data[offset..offset + 8].try_into().unwrap());

                if !self.is_valid_kernel_ptr(ptr) {
                    continue;
                }

                if let Ok(test_data) = self.dma.read(4, ptr, 0x100) {
                    let mut valid_ptr_count = 0;
                    for j in (0..0x100).step_by(8) {
                        if j + 8 > test_data.len() {
                            break;
                        }
                        let test_ptr = u64::from_le_bytes(test_data[j..j + 8].try_into().unwrap());
                        if self.is_valid_kernel_ptr(test_ptr) {
                            valid_ptr_count += 1;
                        }
                    }

                    if valid_ptr_count >= 8 {
                        println!(
                            "[*] Potential GPU object @ 0x{:X} (offset 0x{:X}, {} valid ptrs)",
                            ptr, offset, valid_ptr_count
                        );

                        if !devices.iter().any(|d: &GpuDevice| d.address == ptr) {
                            devices.push(GpuDevice {
                                index: devices.len() as u32,
                                address: ptr,
                                uuid: GpuUuid::new_uninit(),
                                uuid_offset: None,
                            });
                        }
                    }
                }
            }
        }

        Ok(devices)
    }

    fn get_driver_context(&self) -> Result<u64> {
        let addr = self.driver_base + offsets::DRIVER_CONTEXT_GLOBAL;
        let ctx = self.dma.read_u64(4, addr)?;
        if ctx == 0 {
            bail!("Driver context is NULL");
        }
        Ok(ctx)
    }

    fn get_device_manager(&self) -> Result<u64> {
        let ctx = self.get_driver_context()?;
        let mgr = self.dma.read_u64(4, ctx + offsets::DEVICE_MGR_BASE)?;
        if mgr == 0 {
            bail!("Device manager is NULL");
        }
        Ok(mgr)
    }

    fn get_first_device_context(&self) -> Result<Option<u64>> {
        let mgr = self.get_device_manager()?;
        let count = self.dma.read_u32(4, mgr + offsets::DEV_MGR_DEVICE_COUNT)?;

        println!("[*] Device count: {}", count);

        if count == 0 || count > 32 {
            return Ok(None);
        }

        let ptr_offset = 16 * 16164u64;
        let ctx = self.dma.read_u64(4, mgr + ptr_offset)?;

        if self.is_valid_kernel_ptr(ctx) {
            println!("[*] First device context: 0x{:X}", ctx);
            return Ok(Some(ctx));
        }

        Ok(None)
    }

    fn get_gpu_manager(&self, device_ctx: u64) -> Result<u64> {
        let mgr = self
            .dma
            .read_u64(4, device_ctx + offsets::DEVICE_CTX_GPU_MGR)?;
        if mgr == 0 {
            bail!("GPU Manager is NULL");
        }
        Ok(mgr)
    }

    fn get_gpu_count(&self, gpu_mgr: u64) -> Result<u32> {
        self.dma.read_u32(4, gpu_mgr + offsets::GPU_MGR_GPU_COUNT)
    }

    fn get_gpu_object(&self, gpu_mgr: u64, index: u32) -> Result<u64> {
        let addr = gpu_mgr + offsets::GPU_MGR_GPU_ARRAY + (index as u64 * 8);
        self.dma.read_u64(4, addr)
    }

    fn is_valid_kernel_ptr(&self, ptr: u64) -> bool {
        ptr != 0 && ptr > 0xFFFF000000000000
    }
}
