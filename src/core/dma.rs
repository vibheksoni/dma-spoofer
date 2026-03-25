use anyhow::Result;
use memprocfs::{LeechCore, Vmm, FLAG_NOCACHE};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceType {
    Fpga,
    Vmware,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionStatus {
    Connected,
    Disconnected,
    Stale,
}

#[derive(Debug, Clone)]
pub struct ModuleInfo {
    pub base: u64,
    pub size: u32,
}

#[derive(Debug, Clone)]
pub struct KernelDriver {
    pub va: u64,
    pub device_object: u64,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct FpgaInfo {
    pub id: u64,
    pub version_major: u64,
    pub version_minor: u64,
}

#[derive(Debug, Clone)]
pub struct VmwareInfo {
    pub vm_pid: u32,
    pub vm_name: String,
    pub memory_size_mb: u64,
}

pub struct Dma<'a> {
    vmm: Vmm<'a>,
    device_type: DeviceType,
    vmware_pid: Option<u32>,
}

impl<'a> Dma<'a> {
    pub fn new() -> Result<Self> {
        println!("    Loading vmm.dll...");

        let args: Vec<&str> = vec!["-device", "fpga", "-waitinitialize"];

        println!("    Connecting to FPGA...");
        let vmm = Vmm::new("vmm.dll", &args)
            .map_err(|e| anyhow::anyhow!("VMMDLL_Initialize failed: {}", e))?;

        Ok(Self {
            vmm,
            device_type: DeviceType::Fpga,
            vmware_pid: None,
        })
    }

    pub fn new_vmware() -> Result<Self> {
        Self::new_vmware_with_pid(None)
    }

    pub fn new_vmware_with_pid(pid: Option<u32>) -> Result<Self> {
        println!("    Loading vmm.dll...");

        let args: Vec<String> = match pid {
            Some(p) => {
                println!("    Connecting to VMware VM (PID {})...", p);
                vec!["-device".to_string(), format!("vmware://id={}", p)]
            }
            None => {
                println!("    Connecting to VMware VM (auto-detect)...");
                vec!["-device".to_string(), "vmware".to_string()]
            }
        };

        let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

        let vmm = Vmm::new("vmm.dll", &args_str)
            .map_err(|e| anyhow::anyhow!("VMMDLL_Initialize failed: {}", e))?;

        let max_addr = vmm
            .get_config(memprocfs::CONFIG_OPT_CORE_MAX_NATIVE_ADDRESS)
            .unwrap_or(0);
        let memory_size_mb = max_addr / (1024 * 1024);

        println!("    Connected! Memory size: {} MB", memory_size_mb);

        Ok(Self {
            vmm,
            device_type: DeviceType::Vmware,
            vmware_pid: pid,
        })
    }

    pub fn device_type(&self) -> DeviceType {
        self.device_type
    }

    pub fn is_vmware(&self) -> bool {
        self.device_type == DeviceType::Vmware
    }

    pub fn reconnect(&mut self) -> Result<()> {
        match self.device_type {
            DeviceType::Fpga => {
                let args: Vec<&str> = vec!["-device", "fpga", "-waitinitialize"];
                self.vmm = Vmm::new("vmm.dll", &args)
                    .map_err(|e| anyhow::anyhow!("Reconnect failed: {}", e))?;
            }
            DeviceType::Vmware => {
                let args: Vec<String> = match self.vmware_pid {
                    Some(p) => vec!["-device".to_string(), format!("vmware://id={}", p)],
                    None => vec!["-device".to_string(), "vmware".to_string()],
                };
                let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
                self.vmm = Vmm::new("vmm.dll", &args_str)
                    .map_err(|e| anyhow::anyhow!("Reconnect failed: {}", e))?;
            }
        }
        println!("    Reconnected successfully!");
        Ok(())
    }

    pub fn get_vmware_info(&self) -> Option<VmwareInfo> {
        if self.device_type != DeviceType::Vmware {
            return None;
        }

        let max_addr = self
            .vmm
            .get_config(memprocfs::CONFIG_OPT_CORE_MAX_NATIVE_ADDRESS)
            .unwrap_or(0);

        Some(VmwareInfo {
            vm_pid: self.vmware_pid.unwrap_or(0),
            vm_name: "VMware VM".to_string(),
            memory_size_mb: max_addr / (1024 * 1024),
        })
    }

    pub fn vmm(&self) -> &Vmm<'a> {
        &self.vmm
    }

    pub fn read(&self, pid: u32, addr: u64, size: usize) -> Result<Vec<u8>> {
        let process = self
            .vmm
            .process_from_pid(pid)
            .map_err(|e| anyhow::anyhow!("Failed to get process {}: {}", pid, e))?;

        process
            .mem_read_ex(addr, size, FLAG_NOCACHE)
            .map_err(|e| anyhow::anyhow!("Read failed at 0x{:X}: {}", addr, e))
    }

    pub fn write(&self, pid: u32, addr: u64, data: &[u8]) -> Result<()> {
        let process = self
            .vmm
            .process_from_pid(pid)
            .map_err(|e| anyhow::anyhow!("Failed to get process {}: {}", pid, e))?;

        process
            .mem_write(addr, data)
            .map_err(|e| anyhow::anyhow!("Write failed at 0x{:X}: {}", addr, e))
    }

    pub fn read_phys(&self, addr: u64, size: usize) -> Result<Vec<u8>> {
        self.vmm
            .mem_read_ex(addr, size, FLAG_NOCACHE)
            .map_err(|e| anyhow::anyhow!("Physical read failed at 0x{:X}: {}", addr, e))
    }

    pub fn write_phys(&self, addr: u64, data: &[u8]) -> Result<()> {
        self.vmm
            .mem_write(addr, data)
            .map_err(|e| anyhow::anyhow!("Physical write failed at 0x{:X}: {}", addr, e))
    }

    pub fn read_u8(&self, pid: u32, addr: u64) -> Result<u8> {
        let buf = self.read(pid, addr, 1)?;
        Ok(buf[0])
    }

    pub fn read_u16(&self, pid: u32, addr: u64) -> Result<u16> {
        let buf = self.read(pid, addr, 2)?;
        Ok(u16::from_le_bytes(buf[..2].try_into()?))
    }

    pub fn read_u32(&self, pid: u32, addr: u64) -> Result<u32> {
        let buf = self.read(pid, addr, 4)?;
        Ok(u32::from_le_bytes(buf[..4].try_into()?))
    }

    pub fn read_u64(&self, pid: u32, addr: u64) -> Result<u64> {
        let buf = self.read(pid, addr, 8)?;
        Ok(u64::from_le_bytes(buf[..8].try_into()?))
    }

    pub fn get_module(&self, pid: u32, name: &str) -> Result<ModuleInfo> {
        let process = self
            .vmm
            .process_from_pid(pid)
            .map_err(|e| anyhow::anyhow!("Failed to get process {}: {}", pid, e))?;

        let base = process
            .get_module_base(name)
            .map_err(|e| anyhow::anyhow!("Module '{}' not found: {}", name, e))?;

        let modules = process
            .map_module(false, false)
            .map_err(|e| anyhow::anyhow!("Failed to get modules: {}", e))?;

        let size = modules
            .iter()
            .find(|m| m.name.eq_ignore_ascii_case(name))
            .map(|m| m.image_size)
            .unwrap_or(0);

        Ok(ModuleInfo { base, size })
    }

    pub fn get_kernel_drivers(&self) -> Result<Vec<KernelDriver>> {
        let drivers = self
            .vmm
            .map_kdriver()
            .map_err(|e| anyhow::anyhow!("Failed to get kernel drivers: {}", e))?;

        Ok(drivers
            .iter()
            .map(|d| KernelDriver {
                va: d.va,
                device_object: d.va_device_object,
                name: d.name.clone(),
            })
            .collect())
    }

    pub fn get_fpga_info(&self) -> Option<FpgaInfo> {
        if let Ok(lc) = self.vmm.get_leechcore() {
            let id = lc.get_option(LeechCore::LC_OPT_FPGA_FPGA_ID).unwrap_or(0);
            let major = lc
                .get_option(LeechCore::LC_OPT_FPGA_VERSION_MAJOR)
                .unwrap_or(0);
            let minor = lc
                .get_option(LeechCore::LC_OPT_FPGA_VERSION_MINOR)
                .unwrap_or(0);

            if id > 0 {
                return Some(FpgaInfo {
                    id,
                    version_major: major,
                    version_minor: minor,
                });
            }
        }
        None
    }

    pub fn read_bytes(&self, addr: u64, size: usize) -> Result<Vec<u8>> {
        self.read(4, addr, size)
    }

    pub fn write_bytes(&self, addr: u64, data: &[u8]) -> Result<()> {
        self.write(4, addr, data)
    }

    pub fn read_u32_k(&self, addr: u64) -> Result<u32> {
        self.read_u32(4, addr)
    }

    pub fn read_u64_k(&self, addr: u64) -> Result<u64> {
        self.read_u64(4, addr)
    }

    pub fn is_connected(&self) -> bool {
        self.read_phys(0x1000, 4).is_ok()
    }

    pub fn is_memory_volatile(&self) -> bool {
        if let Ok(lc) = self.vmm.get_leechcore() {
            lc.get_option(LeechCore::LC_OPT_CORE_VOLATILE)
                .unwrap_or(0)
                == 1
        } else {
            false
        }
    }

    pub fn health_check(&self) -> ConnectionStatus {
        if !self.is_connected() {
            return ConnectionStatus::Disconnected;
        }
        if self.device_type == DeviceType::Vmware && !self.is_memory_volatile() {
            return ConnectionStatus::Stale;
        }
        ConnectionStatus::Connected
    }

    pub fn ensure_connected(&mut self) -> Result<()> {
        match self.health_check() {
            ConnectionStatus::Connected => Ok(()),
            ConnectionStatus::Disconnected | ConnectionStatus::Stale => {
                println!("    [!] Connection lost, attempting reconnect...");
                self.reconnect()
            }
        }
    }
}
