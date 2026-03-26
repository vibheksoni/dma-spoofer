use crate::core::Dma;
use crate::spoofers::arp::offsets::*;
use crate::spoofers::arp::types::{ArpEntry, Compartment, NeighborState};
use anyhow::{anyhow, Result};

pub struct ArpSpoofer<'a> {
    dma: &'a Dma<'a>,
    tcpip_base: u64,
    tcpip_size: u32,
    layout: ArpLayout,
    compartment_set: u64,
    entries: Vec<ArpEntry>,
}

impl<'a> ArpSpoofer<'a> {
    pub fn new(dma: &'a Dma<'a>) -> Result<Self> {
        let module = dma.get_module(4, "tcpip.sys")?;
        let tcpip_base = module.base;
        let tcpip_size = module.size;

        println!(
            "[+] tcpip.sys base: 0x{:X} size: 0x{:X}",
            tcpip_base, tcpip_size
        );

        let build_number = Self::detect_build_number(dma).unwrap_or(0);
        let layout = layout_for_build(build_number);
        if build_number > 0 {
            println!("[+] ARP layout build: {}", build_number);
        }
        let compartment_set = Self::find_compartment_set(dma, tcpip_base, tcpip_size, layout)?;
        println!("[+] TcpCompartmentSet: 0x{:X}", compartment_set);

        Ok(Self {
            dma,
            tcpip_base,
            tcpip_size,
            layout,
            compartment_set,
            entries: Vec::new(),
        })
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

    fn find_compartment_set(dma: &Dma, base: u64, size: u32, layout: ArpLayout) -> Result<u64> {
        println!("[*] Scanning for TcpCompartmentSet...");

        let data = dma.read(4, base, size as usize)?;

        if let Some(global_rva) = layout.compartment_set_global_rva {
            let target = base + global_rva;
            if let Some(set_ptr) = Self::validate_compartment_set_value(dma, target, layout)? {
                return Ok(set_ptr);
            }
            if let Some(set_ptr) =
                Self::validate_compartment_set_ptr(dma, base, size, target, layout)?
            {
                return Ok(set_ptr);
            }
        }

        for i in 0..data.len().saturating_sub(26) {
            if data[i] != 0x4C
                || data[i + 1] != 0x8D
                || data[i + 2] != 0x05
                || data[i + 7] != 0x48
                || data[i + 8] != 0x8D
                || data[i + 9] != 0x15
                || data[i + 14] != 0x48
                || data[i + 15] != 0x8D
                || data[i + 16] != 0x0D
                || data[i + 21] != 0xE8
            {
                continue;
            }

            let target = Self::rip_relative_target(base, i as u64 + 14, &data[i + 17..i + 21]);
            if let Some(set_ptr) =
                Self::validate_compartment_set_ptr(dma, base, size, target, layout)?
            {
                return Ok(set_ptr);
            }
        }

        for i in (0..data.len().saturating_sub(8)).step_by(8) {
            let set_ptr = u64::from_le_bytes(data[i..i + 8].try_into().unwrap());
            if !Self::is_kernel_pointer(set_ptr) {
                continue;
            }

            if let Some(set_ptr) = Self::validate_compartment_set_value(dma, set_ptr, layout)? {
                return Ok(set_ptr);
            }
        }

        for i in 0..data.len().saturating_sub(7) {
            if data[i] != 0x48 || data[i + 1] != 0x8D {
                continue;
            }

            let modrm = data[i + 2];
            if modrm != 0x05
                && modrm != 0x0D
                && modrm != 0x15
                && modrm != 0x1D
                && modrm != 0x25
                && modrm != 0x2D
                && modrm != 0x35
                && modrm != 0x3D
            {
                continue;
            }

            let target = Self::rip_relative_target(base, i as u64, &data[i + 3..i + 7]);
            if let Some(set_ptr) =
                Self::validate_compartment_set_ptr(dma, base, size, target, layout)?
            {
                return Ok(set_ptr);
            }
        }

        Err(anyhow!("Could not find TcpCompartmentSet in tcpip.sys"))
    }

    fn rip_relative_target(base: u64, instruction_offset: u64, displacement: &[u8]) -> u64 {
        let rip_offset = i32::from_le_bytes(displacement.try_into().unwrap());
        let rip = base + instruction_offset + 7;
        (rip as i64 + rip_offset as i64) as u64
    }

    fn validate_compartment_set_ptr(
        dma: &Dma,
        base: u64,
        size: u32,
        target: u64,
        layout: ArpLayout,
    ) -> Result<Option<u64>> {
        if target <= base || target >= base + size as u64 {
            return Ok(None);
        }

        let set_ptr = match dma.read_u64(4, target) {
            Ok(value) => value,
            Err(_) => return Ok(None),
        };
        if !Self::is_kernel_pointer(set_ptr) {
            return Ok(None);
        }

        Self::validate_compartment_set_value(dma, set_ptr, layout)
    }

    fn validate_compartment_set_value(
        dma: &Dma,
        set_ptr: u64,
        layout: ArpLayout,
    ) -> Result<Option<u64>> {
        let count = match dma.read_u32(4, set_ptr + layout.compartment_set_count) {
            Ok(value) => value,
            Err(_) => return Ok(None),
        };
        if count == 0 || count > 4096 {
            return Ok(None);
        }

        let table_ptr = match dma.read_u64(4, set_ptr + layout.compartment_set_table) {
            Ok(value) => value,
            Err(_) => return Ok(None),
        };
        if !Self::is_kernel_pointer(table_ptr) {
            return Ok(None);
        }

        let mut valid_compartments = 0u32;

        for i in 0..count {
            let bucket_ptr = table_ptr + (i as u64 * 16);
            let list_head = match dma.read_u64(4, bucket_ptr) {
                Ok(value) => value,
                Err(_) => continue,
            };

            if list_head == 0 || list_head == bucket_ptr {
                continue;
            }
            if !Self::is_kernel_pointer(list_head) {
                continue;
            }

            let compartment_addr = list_head.saturating_sub(layout.compartment_entry_offset);
            if compartment_addr == 0 {
                continue;
            }

            let compartment_id = match dma.read_u32(4, compartment_addr + layout.compartment_id) {
                Ok(value) => value,
                Err(_) => continue,
            };
            let compartment_refcount = match dma.read_u32(4, compartment_addr + 0x20) {
                Ok(value) => value,
                Err(_) => continue,
            };
            let next_entry = match dma.read_u64(4, list_head) {
                Ok(value) => value,
                Err(_) => continue,
            };
            if compartment_id > 0
                && compartment_id < 0x10000
                && compartment_refcount > 0
                && (next_entry == bucket_ptr || Self::is_kernel_pointer(next_entry))
            {
                valid_compartments += 1;
                break;
            }
        }

        if valid_compartments == 0 {
            return Ok(None);
        }

        Ok(Some(set_ptr))
    }

    fn is_kernel_pointer(value: u64) -> bool {
        value >= 0xFFFF800000000000
    }

    pub fn enumerate(&mut self) -> Result<Vec<ArpEntry>> {
        self.entries.clear();

        let compartments = self.enumerate_compartments()?;
        println!("[*] Found {} compartment(s)", compartments.len());

        for compartment in &compartments {
            println!(
                "[*] Compartment {}: {} neighbors at 0x{:X}",
                compartment.id, compartment.neighbor_count, compartment.neighbor_table_addr
            );

            if compartment.neighbor_count > 0 && compartment.neighbor_count < 1000 {
                match self.enumerate_neighbors(compartment) {
                    Ok(neighbors) => {
                        self.entries.extend(neighbors);
                    }
                    Err(e) => {
                        println!("[-] Failed to enumerate neighbors: {}", e);
                    }
                }
            }
        }

        Ok(self.entries.clone())
    }

    fn enumerate_compartments(&self) -> Result<Vec<Compartment>> {
        let mut compartments = Vec::new();

        let count_addr = self.compartment_set + self.layout.compartment_set_count;
        let table_ptr_addr = self.compartment_set + self.layout.compartment_set_table;

        let count = self.dma.read_u32_k(count_addr)?;
        let table_ptr = self.dma.read_u64_k(table_ptr_addr)?;

        println!("[*] Compartment count: {}, table: 0x{:X}", count, table_ptr);

        if count == 0 || count > 4096 || table_ptr == 0 {
            return Ok(compartments);
        }

        for i in 0..count {
            let bucket_ptr = table_ptr + (i as u64 * 16);
            let list_head = self.dma.read_u64_k(bucket_ptr)?;

            if list_head == 0 || list_head == bucket_ptr {
                continue;
            }

            let mut current = list_head;
            let mut iter_count = 0;

            while current != 0 && current != bucket_ptr && iter_count < 100 {
                let compartment_addr = current.saturating_sub(self.layout.compartment_entry_offset);

                if compartment_addr == 0 {
                    break;
                }

                let id = self
                    .dma
                    .read_u32_k(compartment_addr + self.layout.compartment_id)?;
                let neighbor_table = compartment_addr + self.layout.compartment_neighbor_table;
                let neighbor_count = self
                    .dma
                    .read_u32_k(neighbor_table + self.layout.hash_table_num_entries)?;

                compartments.push(Compartment::new(
                    compartment_addr,
                    id,
                    neighbor_table,
                    neighbor_count,
                ));

                current = self.dma.read_u64_k(current)?;
                iter_count += 1;
            }
        }

        Ok(compartments)
    }

    fn enumerate_neighbors(&self, compartment: &Compartment) -> Result<Vec<ArpEntry>> {
        let mut neighbors = Vec::new();

        let table_size = self
            .dma
            .read_u32_k(compartment.neighbor_table_addr + self.layout.hash_table_size)?;
        let directory = self
            .dma
            .read_u64_k(compartment.neighbor_table_addr + self.layout.hash_table_directory)?;

        if table_size == 0 || table_size > 4096 || directory == 0 {
            return Ok(neighbors);
        }

        println!(
            "[*] Hash table: size={}, directory=0x{:X}",
            table_size, directory
        );

        for i in 0..table_size {
            let bucket_addr = directory + (i as u64 * 16);
            let list_head = self.dma.read_u64_k(bucket_addr)?;

            if list_head == 0 || list_head == bucket_addr {
                continue;
            }

            let mut current = list_head;
            let mut iter_count = 0;

            while current != 0 && current != bucket_addr && iter_count < 100 {
                let neighbor_addr = current.saturating_sub(self.layout.neighbor_entry_offset);

                if neighbor_addr == 0 {
                    break;
                }

                match self.read_neighbor(neighbor_addr) {
                    Ok(Some(entry)) => {
                        neighbors.push(entry);
                    }
                    Ok(None) => {}
                    Err(e) => {
                        println!(
                            "[-] Failed to read neighbor at 0x{:X}: {}",
                            neighbor_addr, e
                        );
                    }
                }

                current = self.dma.read_u64_k(current)?;
                iter_count += 1;
            }
        }

        Ok(neighbors)
    }

    fn read_neighbor(&self, addr: u64) -> Result<Option<ArpEntry>> {
        let interface_ptr = self.dma.read_u64_k(addr + self.layout.neighbor_interface)?;
        let state_raw = self.dma.read_u32_k(addr + self.layout.neighbor_state)?;
        let refcount = self.dma.read_u32_k(addr + self.layout.neighbor_refcount)?;

        if interface_ptr == 0 || refcount == 0 {
            return Ok(None);
        }

        let state = NeighborState::from_raw(state_raw);

        if matches!(
            state,
            NeighborState::Unreachable | NeighborState::Incomplete
        ) {
            return Ok(None);
        }

        let mac_addr_location = addr + self.layout.neighbor_dl_address;
        let mac_bytes = self.dma.read_bytes(mac_addr_location, DL_ADDRESS_SIZE)?;

        let mut mac_address = [0u8; 6];
        mac_address.copy_from_slice(&mac_bytes);

        if mac_address == [0u8; 6] || mac_address == [0xFF; 6] {
            return Ok(None);
        }

        Ok(Some(ArpEntry::new(
            addr,
            interface_ptr,
            state,
            mac_address,
            mac_addr_location,
        )))
    }

    pub fn list(&self) -> &[ArpEntry] {
        &self.entries
    }

    pub fn print_entries(&self) {
        println!("\n[*] ARP Cache ({} entries):", self.entries.len());
        println!("{:-<70}", "");
        println!(
            "{:<18} {:<14} {:<16}",
            "MAC Address", "State", "Neighbor Addr"
        );
        println!("{:-<70}", "");

        for entry in &self.entries {
            println!(
                "{:<18} {:<14} 0x{:X}",
                entry.mac_string(),
                entry.state.as_str(),
                entry.neighbor_addr
            );
        }
        println!("{:-<70}", "");
    }

    pub fn spoof_mac(&self, old_mac: &[u8; 6], new_mac: &[u8; 6]) -> Result<u32> {
        let mut spoofed = 0;

        for entry in &self.entries {
            if &entry.mac_address == old_mac {
                println!(
                    "[*] Spoofing {} -> {} at 0x{:X}",
                    entry.mac_string(),
                    format!(
                        "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
                        new_mac[0], new_mac[1], new_mac[2], new_mac[3], new_mac[4], new_mac[5]
                    ),
                    entry.mac_addr_location
                );

                self.dma.write_bytes(entry.mac_addr_location, new_mac)?;
                spoofed += 1;
            }
        }

        Ok(spoofed)
    }

    pub fn spoof_all(&self) -> Result<u32> {
        let mut spoofed = 0;

        for entry in &self.entries {
            let mut new_mac = [0u8; 6];
            new_mac[0] = (entry.mac_address[0] & 0xFC) | 0x02;
            for i in 1..6 {
                new_mac[i] = rand::random();
            }

            println!(
                "[*] Spoofing {} -> {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
                entry.mac_string(),
                new_mac[0],
                new_mac[1],
                new_mac[2],
                new_mac[3],
                new_mac[4],
                new_mac[5]
            );

            self.dma.write_bytes(entry.mac_addr_location, &new_mac)?;
            spoofed += 1;
        }

        Ok(spoofed)
    }

    pub fn refresh(&mut self) -> Result<()> {
        self.enumerate()?;
        Ok(())
    }
}
