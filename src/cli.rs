use std::env;

use anyhow::Result;

use crate::core::{ConnectionStatus, DeviceType, Dma};
use crate::spoofers::arp::ArpSpoofer;
use crate::spoofers::boot::BootSpoofer;
use crate::spoofers::disk::DiskSpoofer;
use crate::spoofers::efi::EfiSpoofer;
use crate::spoofers::gpu::NvidiaSpoofer;
use crate::spoofers::monitor::{DxgkrnlEdidSpoofer, MonitorSpoofer};
use crate::spoofers::nic::NicSpoofer;
use crate::spoofers::registry::RegistryTraceSpoofer;
use crate::spoofers::smbios::SmbiosSpoofer;
use crate::spoofers::tpm::TpmSpoofer;
use crate::spoofers::usb::UsbSpoofer;
use crate::spoofers::volume::VolumeSpoofer;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliArgs {
    pub device: DeviceChoice,
    pub command: CliCommand,
    pub vmware_pid: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceChoice {
    Fpga,
    Vmware,
    Auto,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliCommand {
    Gui,
    ListModule(String),
    TestAll,
    SpoofModule(String),
    HealthCheck,
}

pub fn parse_args() -> CliArgs {
    let args: Vec<String> = env::args().collect();

    let mut device = DeviceChoice::Auto;
    let mut command = CliCommand::Gui;
    let mut vmware_pid: Option<u32> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--fpga" | "-f" => {
                device = DeviceChoice::Fpga;
            }
            "--vmware" | "-v" => {
                device = DeviceChoice::Vmware;
            }
            "--vmware-pid" => {
                if i + 1 < args.len() {
                    vmware_pid = args[i + 1].parse().ok();
                    i += 1;
                }
            }
            "--list" | "-l" => {
                if i + 1 < args.len() {
                    command = CliCommand::ListModule(args[i + 1].to_lowercase());
                    i += 1;
                }
            }
            "--spoof" | "-s" => {
                if i + 1 < args.len() {
                    command = CliCommand::SpoofModule(args[i + 1].to_lowercase());
                    i += 1;
                }
            }
            "--test-all" | "-t" => {
                command = CliCommand::TestAll;
            }
            "--health" | "-H" => {
                command = CliCommand::HealthCheck;
            }
            "--no-gui" | "-n" => {
                if command == CliCommand::Gui {
                    command = CliCommand::TestAll;
                }
            }
            "--help" | "-h" => {
                print_usage();
                std::process::exit(0);
            }
            _ => {}
        }
        i += 1;
    }

    CliArgs {
        device,
        command,
        vmware_pid,
    }
}

fn print_usage() {
    println!("DMA Spoofer CLI");
    println!();
    println!("USAGE:");
    println!("  dma-spoofer [OPTIONS] [COMMAND]");
    println!();
    println!("DEVICE OPTIONS:");
    println!("  -f, --fpga          Use FPGA DMA hardware");
    println!("  -v, --vmware        Use VMware VM (auto-detect)");
    println!("      --vmware-pid N  Connect to specific VM by PID");
    println!();
    println!("COMMANDS:");
    println!("  -l, --list <module> List module info (gpu, smbios, disk, nic, monitor,");
    println!("                      dxgkrnl, tpm, volume, registry, arp, efi, boot, usb, all)");
    println!("  -s, --spoof <module> Spoof module (gpu, smbios, disk, nic, monitor,");
    println!("                       dxgkrnl, tpm, volume, registry, arp, boot, usb)");
    println!("  -t, --test-all      Test all read modules and report status");
    println!("  -H, --health        Check connection health");
    println!("  -n, --no-gui        Run without GUI (default: test-all)");
    println!("  -h, --help          Show this help");
    println!();
    println!("EXAMPLES:");
    println!("  dma-spoofer --vmware --list gpu");
    println!("  dma-spoofer -v -l all");
    println!("  dma-spoofer -v --test-all");
    println!("  dma-spoofer --vmware-pid 12345 --health");
}

pub fn run_cli(args: &CliArgs) -> Result<()> {
    let dma = connect_device(args)?;

    match &args.command {
        CliCommand::Gui => unreachable!("GUI mode handled in main"),
        CliCommand::ListModule(module) => list_module(&dma, module),
        CliCommand::TestAll => test_all_modules(&dma),
        CliCommand::SpoofModule(module) => spoof_module(&dma, module),
        CliCommand::HealthCheck => health_check(&dma),
    }
}

fn connect_device(args: &CliArgs) -> Result<Dma<'static>> {
    match args.device {
        DeviceChoice::Fpga => {
            println!("[*] Connecting to FPGA...");
            Dma::new()
        }
        DeviceChoice::Vmware => {
            println!("[*] Connecting to VMware...");
            match args.vmware_pid {
                Some(pid) => {
                    println!("[*] Target VM PID: {}", pid);
                    Dma::new_vmware_with_pid(Some(pid))
                }
                None => Dma::new_vmware(),
            }
        }
        DeviceChoice::Auto => {
            if let Some(pid) = detect_vmware_vm() {
                println!("[*] Auto-detected VMware VM (PID {})", pid);
                Dma::new_vmware_with_pid(Some(pid))
            } else {
                println!("[*] No VMware VM detected, trying FPGA...");
                Dma::new()
            }
        }
    }
}

fn detect_vmware_vm() -> Option<u32> {
    use std::process::Command;

    let output = Command::new("tasklist")
        .args(["/FI", "IMAGENAME eq vmware-vmx.exe", "/FO", "CSV", "/NH"])
        .output()
        .ok()?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    for line in stdout.lines().skip(1) {
        let parts: Vec<&str> = line.split(',').collect();
        if parts.len() >= 2 {
            if let Ok(pid) = parts[1].trim().parse::<u32>() {
                return Some(pid);
            }
        }
    }

    None
}

fn list_module(dma: &Dma, module: &str) -> Result<()> {
    match module {
        "gpu" | "nvidia" => list_gpu(dma),
        "smbios" => list_smbios(dma),
        "disk" => list_disk(dma),
        "nic" | "network" => list_nic(dma),
        "monitor" | "edid" => list_monitor(dma),
        "dxgkrnl" | "dxg" => list_dxgkrnl(dma),
        "tpm" => list_tpm(dma),
        "volume" | "guid" => list_volume(dma),
        "registry" | "reg" => list_registry(dma),
        "arp" => list_arp(dma),
        "efi" => list_efi(dma),
        "boot" => list_boot(dma),
        "usb" => list_usb(dma),
        "all" => list_all(dma),
        _ => {
            println!("[!] Unknown module: {}", module);
            println!("    Available: gpu, smbios, disk, nic, monitor, dxgkrnl, tpm,");
            println!("               volume, registry, arp, efi, boot, usb, all");
            std::process::exit(1);
        }
    }
}

fn spoof_module(dma: &Dma, module: &str) -> Result<()> {
    match module {
        "gpu" | "nvidia" => {
            let spoofer = NvidiaSpoofer::new(dma)?;
            let devices = spoofer.enumerate()?;
            if devices.is_empty() {
                println!("[!] No GPUs found");
                return Ok(());
            }
            for device in &devices {
                let candidates = spoofer.find_uuid_candidates(device.address)?;
                if !candidates.is_empty() {
                    use crate::hwid::SerialGenerator;
                    use std::path::Path;
                    let config = crate::hwid::SeedConfig::load(Path::new("hwid_seed.json"))
                        .unwrap_or_else(|| crate::hwid::SeedConfig::new(
                            std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap()
                                .as_nanos() as u64
                        ));
                    let mut gen = SerialGenerator::from_config(config);
                    let uuid_str = gen.generate_uuid();
                    let uuid_bytes: [u8; 16] = parse_uuid_to_bytes(&uuid_str);
                    for candidate in &candidates {
                        spoofer.write_uuid_at(device.address, candidate.offset, &uuid_bytes)?;
                    }
                    println!("[+] GPU {} spoofed with UUID: {}", device.index, uuid_str);
                }
            }
            Ok(())
        }
        "smbios" => {
            let spoofer = SmbiosSpoofer::new(dma)?;
            let count = spoofer.spoof()?;
            println!("[+] Spoofed {} SMBIOS tables", count);
            Ok(())
        }
        "disk" => {
            let spoofer = DiskSpoofer::new(dma)?;
            spoofer.spoof()?;
            println!("[+] Disk serials spoofed");
            Ok(())
        }
        "nic" | "network" => {
            let mut spoofer = NicSpoofer::new(dma)?;
            spoofer.spoof_and_get_macs()?;
            println!("[+] NIC MAC addresses spoofed");
            Ok(())
        }
        "monitor" | "edid" => {
            let seed = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            let mut spoofer = MonitorSpoofer::new(dma.vmm(), seed);
            spoofer.spoof()?;
            println!("[+] Monitor EDID spoofed");
            Ok(())
        }
        "dxgkrnl" | "dxg" => {
            let seed = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            let mut spoofer = DxgkrnlEdidSpoofer::new(dma, seed)?;
            spoofer.spoof()?;
            println!("[+] dxgkrnl EDID spoofed");
            Ok(())
        }
        "tpm" => {
            let mut spoofer = TpmSpoofer::new(dma);
            spoofer.spoof_registry_only()?;
            println!("[+] TPM registry spoofed");
            Ok(())
        }
        "volume" | "guid" => {
            let spoofer = VolumeSpoofer::new(dma)?;
            spoofer.spoof()?;
            println!("[+] Volume GUIDs spoofed");
            Ok(())
        }
        "registry" | "reg" => {
            let mut spoofer = RegistryTraceSpoofer::new(dma.vmm())?;
            spoofer.spoof()?;
            println!("[+] Registry traces spoofed");
            Ok(())
        }
        "arp" => {
            let mut spoofer = ArpSpoofer::new(dma)?;
            spoofer.enumerate()?;
            let count = spoofer.spoof_all()?;
            println!("[+] Spoofed {} ARP entries", count);
            Ok(())
        }
        "boot" => {
            let mut spoofer = BootSpoofer::new(dma)?;
            spoofer.spoof()?;
            println!("[+] Boot identifiers spoofed");
            Ok(())
        }
        "usb" => {
            let mut spoofer = UsbSpoofer::new(dma)?;
            spoofer.spoof()?;
            println!("[+] USB serials spoofed");
            Ok(())
        }
        _ => {
            println!("[!] Unknown module: {}", module);
            println!("    Available: gpu, smbios, disk, nic, monitor, dxgkrnl,");
            println!("               tpm, volume, registry, arp, boot, usb");
            std::process::exit(1);
        }
    }
}

fn parse_uuid_to_bytes(s: &str) -> [u8; 16] {
    let hex: String = s.replace('-', "").chars().filter(|c| c.is_ascii_hexdigit()).collect();
    let mut bytes = [0u8; 16];
    for i in 0..16.min(hex.len() / 2) {
        bytes[i] = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap_or(0);
    }
    bytes
}

fn test_all_modules(dma: &Dma) -> Result<()> {
    println!("\n=== MODULE READ TEST ===\n");

    let results = vec![
        ("GPU/NVIDIA", test_gpu(dma)),
        ("SMBIOS", test_smbios(dma)),
        ("Disk", test_disk(dma)),
        ("NIC", test_nic(dma)),
        ("Monitor (Registry)", test_monitor(dma)),
        ("dxgkrnl EDID", test_dxgkrnl(dma)),
        ("TPM", test_tpm(dma)),
        ("Volume GUIDs", test_volume(dma)),
        ("Registry Traces", test_registry(dma)),
        ("ARP Cache", test_arp(dma)),
        ("EFI", test_efi(dma)),
        ("Boot", test_boot(dma)),
        ("USB", test_usb(dma)),
    ];

    println!("\n=== RESULTS ===");
    let mut working = 0;
    let mut failed = 0;

    for (name, result) in &results {
        match result {
            Ok(_) => {
                println!("[+] {:<20} WORKING", name);
                working += 1;
            }
            Err(e) => {
                println!("[-] {:<20} FAILED: {}", name, e);
                failed += 1;
            }
        }
    }

    println!("\n[{}] {} working, {} failed",
        if failed == 0 { "+" } else { "!" },
        working, failed
    );

    Ok(())
}

macro_rules! test_module {
    ($name:expr, $init:expr) => {
        (|| -> Result<()> {
            let spoofer = $init;
            let _ = format!("{:?}", spoofer);
            Ok(())
        })()
    };
}

fn test_gpu(dma: &Dma) -> Result<()> {
    let spoofer = NvidiaSpoofer::new(dma)?;
    let devices = spoofer.enumerate()?;
    if devices.is_empty() {
        println!("    GPU: No devices found (may be normal for VMs without GPU passthrough)");
    } else {
        println!("    GPU: Found {} device(s)", devices.len());
    }
    Ok(())
}

fn test_smbios(dma: &Dma) -> Result<()> {
    let spoofer = SmbiosSpoofer::new(dma)?;
    spoofer.list()?;
    Ok(())
}

fn test_disk(dma: &Dma) -> Result<()> {
    let spoofer = DiskSpoofer::new(dma)?;
    spoofer.list()?;
    Ok(())
}

fn test_nic(dma: &Dma) -> Result<()> {
    let mut spoofer = NicSpoofer::new(dma)?;
    spoofer.list()?;
    Ok(())
}

fn test_monitor(dma: &Dma) -> Result<()> {
    let spoofer = MonitorSpoofer::new(dma.vmm(), 0);
    spoofer.list()?;
    Ok(())
}

fn test_dxgkrnl(dma: &Dma) -> Result<()> {
    let spoofer = DxgkrnlEdidSpoofer::new(dma, 0)?;
    spoofer.list()?;
    Ok(())
}

fn test_tpm(dma: &Dma) -> Result<()> {
    let spoofer = TpmSpoofer::new(dma);
    spoofer.list()?;
    Ok(())
}

fn test_volume(dma: &Dma) -> Result<()> {
    let spoofer = VolumeSpoofer::new(dma)?;
    spoofer.list()?;
    Ok(())
}

fn test_registry(dma: &Dma) -> Result<()> {
    let spoofer = RegistryTraceSpoofer::new(dma.vmm())?;
    spoofer.list()?;
    Ok(())
}

fn test_arp(dma: &Dma) -> Result<()> {
    let mut spoofer = ArpSpoofer::new(dma)?;
    spoofer.enumerate()?;
    let count = spoofer.list().len();
    println!("    ARP: Found {} entries", count);
    Ok(())
}

fn test_efi(dma: &Dma) -> Result<()> {
    let spoofer = EfiSpoofer::new(dma)?;
    spoofer.list()?;
    Ok(())
}

fn test_boot(dma: &Dma) -> Result<()> {
    let spoofer = BootSpoofer::new(dma)?;
    spoofer.list()?;
    Ok(())
}

fn test_usb(dma: &Dma) -> Result<()> {
    let mut spoofer = UsbSpoofer::new(dma)?;
    spoofer.list()?;
    Ok(())
}

fn list_all(dma: &Dma) -> Result<()> {
    println!("\n=== ALL MODULES ===\n");

    let _ = list_gpu(dma);
    let _ = list_smbios(dma);
    let _ = list_disk(dma);
    let _ = list_nic(dma);
    let _ = list_monitor(dma);
    let _ = list_dxgkrnl(dma);
    let _ = list_tpm(dma);
    let _ = list_volume(dma);
    let _ = list_registry(dma);
    let _ = list_arp(dma);
    let _ = list_efi(dma);
    let _ = list_boot(dma);
    let _ = list_usb(dma);

    Ok(())
}

fn health_check(dma: &Dma) -> Result<()> {
    println!("\n=== CONNECTION HEALTH ===\n");

    println!("Device Type: {:?}", dma.device_type());
    println!("Is VMware: {}", dma.is_vmware());

    if dma.is_vmware() {
        if let Some(info) = dma.get_vmware_info() {
            println!("VM Memory: {} MB", info.memory_size_mb);
            println!("VM PID: {}", info.vm_pid);
        }
    } else if let Some(info) = dma.get_fpga_info() {
        println!("FPGA ID: {}", info.id);
        println!("FPGA Version: {}.{}", info.version_major, info.version_minor);
    }

    println!("\nConnection Status: {:?}", dma.health_check());
    println!("Memory Volatile: {}", dma.is_memory_volatile());
    println!("Can Read: {}", dma.is_connected());

    Ok(())
}

fn list_gpu(dma: &Dma) -> Result<()> {
    println!("\n[GPU UUIDs]");
    let spoofer = NvidiaSpoofer::new(dma)?;
    let devices = spoofer.enumerate()?;

    if devices.is_empty() {
        println!("  No GPUs found");
        return Ok(());
    }

    for device in &devices {
        println!("  GPU {} @ 0x{:X}", device.index, device.address);
        let candidates = spoofer.find_uuid_candidates(device.address)?;
        if candidates.is_empty() {
            println!("    No UUID candidates");
        } else {
            for candidate in &candidates {
                println!("    [0x{:04X}] {} {:?}", candidate.offset, candidate.uuid, candidate.confidence);
            }
        }
    }
    Ok(())
}

fn list_smbios(dma: &Dma) -> Result<()> {
    println!("\n[SMBIOS]");
    let spoofer = SmbiosSpoofer::new(dma)?;
    spoofer.list()?;
    Ok(())
}

fn list_disk(dma: &Dma) -> Result<()> {
    println!("\n[DISK]");
    let spoofer = DiskSpoofer::new(dma)?;
    spoofer.list()?;
    Ok(())
}

fn list_nic(dma: &Dma) -> Result<()> {
    println!("\n[NIC]");
    let mut spoofer = NicSpoofer::new(dma)?;
    spoofer.list()?;
    Ok(())
}

fn list_monitor(dma: &Dma) -> Result<()> {
    println!("\n[MONITOR EDID (Registry)]");
    let spoofer = MonitorSpoofer::new(dma.vmm(), 0);
    spoofer.list()?;
    Ok(())
}

fn list_dxgkrnl(dma: &Dma) -> Result<()> {
    println!("\n[MONITOR EDID (dxgkrnl)]");
    let spoofer = DxgkrnlEdidSpoofer::new(dma, 0)?;
    spoofer.list()?;
    Ok(())
}

fn list_tpm(dma: &Dma) -> Result<()> {
    println!("\n[TPM]");
    let spoofer = TpmSpoofer::new(dma);
    spoofer.list()?;
    Ok(())
}

fn list_volume(dma: &Dma) -> Result<()> {
    println!("\n[VOLUME GUIDs]");
    let spoofer = VolumeSpoofer::new(dma)?;
    spoofer.list()?;
    Ok(())
}

fn list_registry(dma: &Dma) -> Result<()> {
    println!("\n[REGISTRY TRACES]");
    let spoofer = RegistryTraceSpoofer::new(dma.vmm())?;
    spoofer.list()?;
    Ok(())
}

fn list_arp(dma: &Dma) -> Result<()> {
    println!("\n[ARP CACHE]");
    let mut spoofer = ArpSpoofer::new(dma)?;
    spoofer.enumerate()?;
    spoofer.print_entries();
    Ok(())
}

fn list_efi(dma: &Dma) -> Result<()> {
    println!("\n[EFI VARIABLES]");
    match EfiSpoofer::new(dma) {
        Ok(spoofer) => spoofer.list()?,
        Err(e) => println!("  EFI not available: {}", e),
    }
    Ok(())
}

fn list_boot(dma: &Dma) -> Result<()> {
    println!("\n[BOOT IDENTIFIERS]");
    let spoofer = BootSpoofer::new(dma)?;
    spoofer.list()?;
    Ok(())
}

fn list_usb(dma: &Dma) -> Result<()> {
    println!("\n[USB STORAGE]");
    match UsbSpoofer::new(dma) {
        Ok(mut spoofer) => spoofer.list()?,
        Err(e) => println!("  USB not available: {}", e),
    }
    Ok(())
}
