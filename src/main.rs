mod core;
mod hwid;
mod spoofers;
mod utils;

use std::io::{self, Write};

use anyhow::Result;

use crate::core::{DeviceType, Dma, DsePatcher, PatchGuardBypass, VmwareInfo};
use crate::hwid::{SeedConfig, SerialGenerator};
use crate::spoofers::arp::ArpSpoofer;
use crate::spoofers::boot::BootSpoofer;
use crate::spoofers::disk::DiskSpoofer;
use crate::spoofers::efi::EfiSpoofer;
use crate::spoofers::gpu::{GpuUuid, NvidiaSpoofer, UuidConfidence};
use crate::spoofers::monitor::{DxgkrnlEdidSpoofer, MonitorSpoofer};
use crate::spoofers::nic::{IntelWifiSpoofer, NicSpoofer};
use crate::spoofers::registry::RegistryTraceSpoofer;
use crate::spoofers::smbios::SmbiosSpoofer;
use crate::spoofers::tpm::TpmSpoofer;
use crate::spoofers::usb::UsbSpoofer;
use crate::spoofers::volume::VolumeSpoofer;
use crate::utils::{generate_random_bytes, RegistrySpoofer};

const RED: &str = "\x1b[38;2;255;0;0m";
const GRAY: &str = "\x1b[38;2;150;150;150m";
const WHITE: &str = "\x1b[38;2;255;255;255m";
const RESET: &str = "\x1b[0m";
const CLEAR: &str = "\x1b[2J\x1b[H";

fn clear_screen() {
    print!("{}", CLEAR);
    io::stdout().flush().unwrap();
}

fn main() -> Result<()> {
    enable_ansi_support();
    clear_screen();
    print_banner();

    let dma = select_device()?;
    match dma.device_type() {
        DeviceType::Fpga => print_fpga_info(&dma),
        DeviceType::Vmware => print_vmware_info(&dma),
    }

    loop {
        print_main_menu();
        let choice = read_input();

        match choice.trim() {
            "1" => {
                clear_screen();
                serial_viewer_menu(&dma)?;
                clear_screen();
                print_banner();
            }
            "2" => {
                clear_screen();
                serial_modifier_menu(&dma)?;
                clear_screen();
                print_banner();
            }
            "3" => {
                clear_screen();
                seed_manager_menu()?;
                clear_screen();
                print_banner();
            }
            "4" => {
                clear_screen();
                println!("\n{}[*] Exiting...{}", GRAY, RESET);
                break;
            }
            _ => println!("\n{}[!] Invalid option{}", RED, RESET),
        }
    }

    Ok(())
}

fn print_main_menu() {
    println!();
    println!("{}┌──────────────────────────────────────┐{}", RED, RESET);
    println!(
        "{}│{}           MAIN MENU                  {}│{}",
        RED, WHITE, RED, RESET
    );
    println!("{}├──────────────────────────────────────┤{}", RED, RESET);
    println!(
        "{}│{}  1. Serial Viewer                   {}│{}",
        RED, WHITE, RED, RESET
    );
    println!(
        "{}│{}  2. Serial Modifier                 {}│{}",
        RED, WHITE, RED, RESET
    );
    println!(
        "{}│{}  3. Seed Manager                    {}│{}",
        RED, WHITE, RED, RESET
    );
    println!(
        "{}│{}  4. Exit                            {}│{}",
        RED, WHITE, RED, RESET
    );
    println!("{}└──────────────────────────────────────┘{}", RED, RESET);
    print!("\n{}Select option:{} ", GRAY, RESET);
    io::stdout().flush().unwrap();
}

fn wait_for_enter() {
    print!("\n{}Press Enter to continue...{}", GRAY, RESET);
    io::stdout().flush().unwrap();
    let _ = read_input();
}

fn serial_viewer_menu(dma: &Dma) -> Result<()> {
    loop {
        print_viewer_menu();
        let choice = read_input();

        match choice.trim() {
            "1" => {
                list_gpu_uuids(dma)?;
                wait_for_enter();
                clear_screen();
            }
            "2" => {
                list_smbios(dma)?;
                wait_for_enter();
                clear_screen();
            }
            "3" => {
                list_disk(dma)?;
                wait_for_enter();
                clear_screen();
            }
            "4" => {
                list_nic(dma)?;
                wait_for_enter();
                clear_screen();
            }
            "5" => {
                list_monitors(dma)?;
                wait_for_enter();
                clear_screen();
            }
            "6" => {
                list_dxgkrnl_edid(dma)?;
                wait_for_enter();
                clear_screen();
            }
            "7" => {
                list_tpm(dma)?;
                wait_for_enter();
                clear_screen();
            }
            "8" => {
                list_volume_guids(dma)?;
                wait_for_enter();
                clear_screen();
            }
            "9" => {
                list_registry_traces(dma)?;
                wait_for_enter();
                clear_screen();
            }
            "10" => {
                list_arp_cache(dma)?;
                wait_for_enter();
                clear_screen();
            }
            "11" => {
                list_efi(dma)?;
                wait_for_enter();
                clear_screen();
            }
            "12" => {
                list_boot(dma)?;
                wait_for_enter();
                clear_screen();
            }
            "13" => {
                list_usb(dma)?;
                wait_for_enter();
                clear_screen();
            }
            "14" => {
                view_all_serials(dma)?;
                wait_for_enter();
                clear_screen();
            }
            "0" => break,
            _ => println!("\n{}[!] Invalid option{}", RED, RESET),
        }
    }
    Ok(())
}

fn print_viewer_menu() {
    println!();
    println!("{}┌──────────────────────────────────────┐{}", RED, RESET);
    println!(
        "{}│{}         SERIAL VIEWER                {}│{}",
        RED, WHITE, RED, RESET
    );
    println!("{}├──────────────────────────────────────┤{}", RED, RESET);
    println!(
        "{}│{}  1. GPU UUIDs                       {}│{}",
        RED, WHITE, RED, RESET
    );
    println!(
        "{}│{}  2. SMBIOS Tables                   {}│{}",
        RED, WHITE, RED, RESET
    );
    println!(
        "{}│{}  3. Disk Serials                    {}│{}",
        RED, WHITE, RED, RESET
    );
    println!(
        "{}│{}  4. NIC MAC Addresses               {}│{}",
        RED, WHITE, RED, RESET
    );
    println!(
        "{}│{}  5. Monitor EDID (Registry)         {}│{}",
        RED, WHITE, RED, RESET
    );
    println!(
        "{}│{}  6. Monitor EDID (dxgkrnl)          {}│{}",
        RED, WHITE, RED, RESET
    );
    println!(
        "{}│{}  7. TPM Registry                    {}│{}",
        RED, WHITE, RED, RESET
    );
    println!(
        "{}│{}  8. Volume GUIDs                    {}│{}",
        RED, WHITE, RED, RESET
    );
    println!(
        "{}│{}  9. Registry Traces                 {}│{}",
        RED, WHITE, RED, RESET
    );
    println!(
        "{}│{}  10. ARP Cache                      {}│{}",
        RED, WHITE, RED, RESET
    );
    println!(
        "{}│{}  11. EFI Variables                  {}│{}",
        RED, WHITE, RED, RESET
    );
    println!(
        "{}│{}  12. Boot Identifiers               {}│{}",
        RED, WHITE, RED, RESET
    );
    println!(
        "{}│{}  13. USB Storage Devices            {}│{}",
        RED, WHITE, RED, RESET
    );
    println!(
        "{}│{}  14. View All (Summary)             {}│{}",
        RED, WHITE, RED, RESET
    );
    println!("{}├──────────────────────────────────────┤{}", RED, RESET);
    println!(
        "{}│{}  0. Back to Main Menu               {}│{}",
        RED, GRAY, RED, RESET
    );
    println!("{}└──────────────────────────────────────┘{}", RED, RESET);
    print!("\n{}Select option:{} ", GRAY, RESET);
    io::stdout().flush().unwrap();
}

fn view_all_serials(dma: &Dma) -> Result<()> {
    println!("\n{}╔══════════════════════════════════════╗{}", RED, RESET);
    println!(
        "{}║{}      HARDWARE SERIAL SUMMARY         {}║{}",
        RED, WHITE, RED, RESET
    );
    println!("{}╚══════════════════════════════════════╝{}", RED, RESET);

    println!("\n{}[GPU]{}", WHITE, RESET);
    let _ = list_gpu_uuids(dma);

    println!("\n{}[SMBIOS]{}", WHITE, RESET);
    let _ = list_smbios(dma);

    println!("\n{}[DISK]{}", WHITE, RESET);
    let _ = list_disk(dma);

    println!("\n{}[NIC]{}", WHITE, RESET);
    let _ = list_nic(dma);

    println!("\n{}[VOLUME]{}", WHITE, RESET);
    let _ = list_volume_guids(dma);

    Ok(())
}

fn serial_modifier_menu(dma: &Dma) -> Result<()> {
    loop {
        print_modifier_menu();
        let choice = read_input();

        match choice.trim() {
            "1" => {
                spoof_gpu_uuids(dma)?;
                wait_for_enter();
                clear_screen();
            }
            "2" => {
                find_gpu_by_signature(dma)?;
                wait_for_enter();
                clear_screen();
            }
            "3" => {
                scan_physical_memory_for_uuid(dma)?;
                wait_for_enter();
                clear_screen();
            }
            "4" => {
                spoof_smbios(dma)?;
                wait_for_enter();
                clear_screen();
            }
            "5" => {
                spoof_disk(dma)?;
                wait_for_enter();
                clear_screen();
            }
            "6" => {
                spoof_nic(dma)?;
                wait_for_enter();
                clear_screen();
            }
            "7" => {
                spoof_monitors(dma)?;
                wait_for_enter();
                clear_screen();
            }
            "8" => {
                spoof_dxgkrnl_edid(dma)?;
                wait_for_enter();
                clear_screen();
            }
            "9" => {
                spoof_tpm(dma)?;
                wait_for_enter();
                clear_screen();
            }
            "10" => {
                spoof_volume_guids(dma)?;
                wait_for_enter();
                clear_screen();
            }
            "11" => {
                spoof_registry_traces(dma)?;
                wait_for_enter();
                clear_screen();
            }
            "12" => {
                spoof_arp_cache(dma)?;
                wait_for_enter();
                clear_screen();
            }
            "14" => {
                spoof_boot(dma)?;
                wait_for_enter();
                clear_screen();
            }
            "15" => {
                spoof_usb(dma)?;
                wait_for_enter();
                clear_screen();
            }
            "16" => {
                dse_status(dma)?;
                wait_for_enter();
                clear_screen();
            }
            "17" => {
                dse_disable(dma)?;
                wait_for_enter();
                clear_screen();
            }
            "18" => {
                dse_enable(dma)?;
                wait_for_enter();
                clear_screen();
            }
            "19" => {
                patchguard_bypass(dma)?;
                wait_for_enter();
                clear_screen();
            }
            "0" => break,
            _ => println!("\n{}[!] Invalid option{}", RED, RESET),
        }
    }
    Ok(())
}

fn print_modifier_menu() {
    println!();
    println!("{}┌──────────────────────────────────────┐{}", RED, RESET);
    println!(
        "{}│{}        SERIAL MODIFIER               {}│{}",
        RED, WHITE, RED, RESET
    );
    println!("{}├──────────────────────────────────────┤{}", RED, RESET);
    println!(
        "{}│{}  1. Spoof GPU UUID                  {}│{}",
        RED, WHITE, RED, RESET
    );
    println!(
        "{}│{}  2. Find GPU by Signature           {}│{}",
        RED, WHITE, RED, RESET
    );
    println!(
        "{}│{}  3. Scan Physical Memory for UUID   {}│{}",
        RED, WHITE, RED, RESET
    );
    println!(
        "{}│{}  4. Spoof SMBIOS                    {}│{}",
        RED, WHITE, RED, RESET
    );
    println!(
        "{}│{}  5. Spoof Disk Serials              {}│{}",
        RED, WHITE, RED, RESET
    );
    println!(
        "{}│{}  6. Spoof NIC MAC                   {}│{}",
        RED, WHITE, RED, RESET
    );
    println!(
        "{}│{}  7. Spoof Monitor EDID (Registry)   {}│{}",
        RED, WHITE, RED, RESET
    );
    println!(
        "{}│{}  8. Spoof Monitor EDID (dxgkrnl)    {}│{}",
        RED, WHITE, RED, RESET
    );
    println!(
        "{}│{}  9. Spoof TPM EK                    {}│{}",
        RED, WHITE, RED, RESET
    );
    println!(
        "{}│{}  10. Spoof Volume GUIDs             {}│{}",
        RED, WHITE, RED, RESET
    );
    println!(
        "{}│{}  11. Spoof Registry Traces          {}│{}",
        RED, WHITE, RED, RESET
    );
    println!(
        "{}│{}  12. Spoof ARP Cache                {}│{}",
        RED, WHITE, RED, RESET
    );
    println!(
        "{}│{}  14. Spoof Boot Time/ID             {}│{}",
        RED, WHITE, RED, RESET
    );
    println!(
        "{}│{}  15. Spoof USB Serials              {}│{}",
        RED, WHITE, RED, RESET
    );
    println!("{}├──────────────────────────────────────┤{}", RED, RESET);
    println!(
        "{}│{}  16. DSE Status                     {}│{}",
        RED, GRAY, RED, RESET
    );
    println!(
        "{}│{}  17. Disable DSE (Dangerous!)       {}│{}",
        RED, GRAY, RED, RESET
    );
    println!(
        "{}│{}  18. Enable DSE (Restore)           {}│{}",
        RED, GRAY, RED, RESET
    );
    println!(
        "{}│{}  19. Bypass PatchGuard (Dangerous!) {}│{}",
        RED, GRAY, RED, RESET
    );
    println!("{}├──────────────────────────────────────┤{}", RED, RESET);
    println!(
        "{}│{}  0. Back to Main Menu               {}│{}",
        RED, GRAY, RED, RESET
    );
    println!("{}└──────────────────────────────────────┘{}", RED, RESET);
    print!("\n{}Select option:{} ", GRAY, RESET);
    io::stdout().flush().unwrap();
}

fn seed_manager_menu() -> Result<()> {
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    clear_screen();
    loop {
        let seed_path = Path::new("hwid_seed.json");
        let config = SeedConfig::load(seed_path).unwrap_or_else(|| {
            let seed = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64;
            SeedConfig::new(seed)
        });

        let mut generator = SerialGenerator::from_config(config);

        println!();
        println!("{}┌──────────────────────────────────────┐{}", RED, RESET);
        println!(
            "{}│{}          SEED MANAGER                {}│{}",
            RED, WHITE, RED, RESET
        );
        println!("{}├──────────────────────────────────────┤{}", RED, RESET);
        println!(
            "{}│{}  Current Seed: {:<20} {}│{}",
            RED,
            GRAY,
            generator.seed(),
            RED,
            RESET
        );
        println!("{}├──────────────────────────────────────┤{}", RED, RESET);
        println!(
            "{}│{}  1. Generate New Random Seed        {}│{}",
            RED, WHITE, RED, RESET
        );
        println!(
            "{}│{}  2. Enter Custom Seed               {}│{}",
            RED, WHITE, RED, RESET
        );
        println!(
            "{}│{}  3. Preview Generated Serials       {}│{}",
            RED, WHITE, RED, RESET
        );
        println!(
            "{}│{}  4. View Saved Serials              {}│{}",
            RED, WHITE, RED, RESET
        );
        println!(
            "{}│{}  5. Export Seed Config              {}│{}",
            RED, WHITE, RED, RESET
        );
        println!("{}├──────────────────────────────────────┤{}", RED, RESET);
        println!(
            "{}│{}  0. Back to Main Menu               {}│{}",
            RED, GRAY, RED, RESET
        );
        println!("{}└──────────────────────────────────────┘{}", RED, RESET);
        print!("\n{}Select option:{} ", GRAY, RESET);
        io::stdout().flush().unwrap();

        let choice = read_input();

        match choice.trim() {
            "1" => {
                let new_seed = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos() as u64;
                generator.reseed(new_seed);
                println!("\n{}[+] New seed: {}{}", WHITE, new_seed, RESET);
                generator.to_config().save(seed_path)?;
                println!("{}[+] Saved to hwid_seed.json{}", WHITE, RESET);
                wait_for_enter();
                clear_screen();
            }
            "2" => {
                print!("\n{}Enter seed (u64):{} ", GRAY, RESET);
                io::stdout().flush().unwrap();
                let seed_str = read_input();
                if let Ok(new_seed) = seed_str.trim().parse::<u64>() {
                    generator.reseed(new_seed);
                    println!("{}[+] Seed set to: {}{}", WHITE, new_seed, RESET);
                    generator.to_config().save(seed_path)?;
                    println!("{}[+] Saved to hwid_seed.json{}", WHITE, RESET);
                } else {
                    println!("{}[!] Invalid seed format{}", RED, RESET);
                }
                wait_for_enter();
                clear_screen();
            }
            "3" => {
                println!("\n{}╔══════════════════════════════════════╗{}", RED, RESET);
                println!(
                    "{}║{}       PREVIEW GENERATED SERIALS      {}║{}",
                    RED, WHITE, RED, RESET
                );
                println!("{}╚══════════════════════════════════════╝{}", RED, RESET);
                println!("\n{}[DISK SERIALS]{}", WHITE, RESET);
                println!(
                    "{}  WD:      {}{}",
                    GRAY,
                    generator.generate_disk_serial("WD"),
                    RESET
                );
                println!(
                    "{}  Samsung: {}{}",
                    GRAY,
                    generator.generate_disk_serial("Samsung"),
                    RESET
                );
                println!(
                    "{}  NVMe:    {}{}",
                    GRAY,
                    generator.generate_disk_serial("NVMe"),
                    RESET
                );
                println!("\n{}[MAC ADDRESSES]{}", WHITE, RESET);
                println!(
                    "{}  Intel:   {}{}",
                    GRAY,
                    generator.generate_mac_with_oui("intel"),
                    RESET
                );
                println!(
                    "{}  Realtek: {}{}",
                    GRAY,
                    generator.generate_mac_with_oui("realtek"),
                    RESET
                );
                println!("\n{}[IDENTIFIERS]{}", WHITE, RESET);
                println!("{}  UUID:    {}{}", GRAY, generator.generate_uuid(), RESET);
                println!("{}  GUID:    {}{}", GRAY, generator.generate_guid(), RESET);
                println!("\n{}[SMBIOS]{}", WHITE, RESET);
                println!(
                    "{}  System:    {}{}",
                    GRAY,
                    generator.generate_smbios_serial("AMI", "ASUS", 1),
                    RESET
                );
                println!(
                    "{}  Baseboard: {}{}",
                    GRAY,
                    generator.generate_smbios_serial("AMI", "ASUS", 2),
                    RESET
                );
                println!("\n{}Note: Preview only, not saved{}", GRAY, RESET);
                wait_for_enter();
                clear_screen();
            }
            "4" => {
                println!("\n{}╔══════════════════════════════════════╗{}", RED, RESET);
                println!(
                    "{}║{}         SAVED SERIALS                {}║{}",
                    RED, WHITE, RED, RESET
                );
                println!("{}╚══════════════════════════════════════╝{}", RED, RESET);
                let gen = generator.generated();
                if let Some(disk) = &gen.disk_serial {
                    println!("{}  Disk:    {}{}", GRAY, disk, RESET);
                }
                if !gen.mac_addresses.is_empty() {
                    println!("{}  MACs:    {:?}{}", GRAY, gen.mac_addresses, RESET);
                }
                if let Some(gpu) = &gen.gpu_uuid {
                    println!("{}  GPU:     {}{}", GRAY, gpu, RESET);
                }
                if !gen.volume_guids.is_empty() {
                    println!("{}  Volumes: {:?}{}", GRAY, gen.volume_guids, RESET);
                }
                if gen.disk_serial.is_none()
                    && gen.mac_addresses.is_empty()
                    && gen.gpu_uuid.is_none()
                {
                    println!("{}  No serials generated yet{}", GRAY, RESET);
                }
                wait_for_enter();
                clear_screen();
            }
            "5" => {
                let export_path = "hwid_seed_export.json";
                generator.to_config().save(Path::new(export_path))?;
                println!("\n{}[+] Exported to {}{}", WHITE, export_path, RESET);
                wait_for_enter();
                clear_screen();
            }
            "0" => break,
            _ => println!("\n{}[!] Invalid option{}", RED, RESET),
        }
    }
    Ok(())
}

fn enable_ansi_support() {
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        let handle = io::stdout().as_raw_handle();
        unsafe {
            let mut mode: u32 = 0;
            winapi::um::consoleapi::GetConsoleMode(handle as *mut _, &mut mode);
            winapi::um::consoleapi::SetConsoleMode(handle as *mut _, mode | 0x0004);
        }
    }
}

fn print_banner() {
    println!();
    println!("{}╔══════════════════════════════════════╗{}", RED, RESET);
    println!(
        "{}║{}         DMA SPOOFER v1               {}║{}",
        RED, WHITE, RED, RESET
    );
    println!("{}╚══════════════════════════════════════╝{}", RED, RESET);
    println!();
}

fn print_menu() {
    println!();
    println!("{}┌──────────────────────────────────────┐{}", RED, RESET);
    println!(
        "{}│{}  1. List GPU UUIDs                   {}│{}",
        RED, WHITE, RED, RESET
    );
    println!(
        "{}│{}  2. Spoof GPU UUID                   {}│{}",
        RED, WHITE, RED, RESET
    );
    println!(
        "{}│{}  3. Find GPU UUID by Signature       {}│{}",
        RED, WHITE, RED, RESET
    );
    println!(
        "{}│{}  4. Scan Physical Memory for UUID    {}│{}",
        RED, WHITE, RED, RESET
    );
    println!(
        "{}│{}  5. List SMBIOS Tables               {}│{}",
        RED, WHITE, RED, RESET
    );
    println!(
        "{}│{}  6. Spoof SMBIOS                     {}│{}",
        RED, WHITE, RED, RESET
    );
    println!(
        "{}│{}  7. List Disk Info                   {}│{}",
        RED, WHITE, RED, RESET
    );
    println!(
        "{}│{}  8. Spoof Disk Serials               {}│{}",
        RED, WHITE, RED, RESET
    );
    println!(
        "{}│{}  9. List NIC Info                    {}│{}",
        RED, WHITE, RED, RESET
    );
    println!(
        "{}│{}  10. Spoof NIC MAC                   {}│{}",
        RED, WHITE, RED, RESET
    );
    println!(
        "{}│{}  11. List Monitor EDID (Registry)    {}│{}",
        RED, WHITE, RED, RESET
    );
    println!(
        "{}│{}  12. Spoof Monitor EDID (Partial)    {}│{}",
        RED, WHITE, RED, RESET
    );
    println!(
        "{}│{}  13. List dxgkrnl EDID Cache         {}│{}",
        RED, WHITE, RED, RESET
    );
    println!(
        "{}│{}  14. Spoof dxgkrnl EDID (Partial)    {}│{}",
        RED, WHITE, RED, RESET
    );
    println!(
        "{}│{}  15. List TPM Registry                {}│{}",
        RED, WHITE, RED, RESET
    );
    println!(
        "{}│{}  16. Spoof TPM EK                    {}│{}",
        RED, WHITE, RED, RESET
    );
    println!(
        "{}│{}  17. DSE Status                      {}│{}",
        RED, WHITE, RED, RESET
    );
    println!(
        "{}│{}  18. Disable DSE (Dangerous!)        {}│{}",
        RED, WHITE, RED, RESET
    );
    println!(
        "{}│{}  19. Enable DSE (Restore)            {}│{}",
        RED, WHITE, RED, RESET
    );
    println!(
        "{}│{}  20. Bypass PatchGuard (Dangerous!)  {}│{}",
        RED, WHITE, RED, RESET
    );
    println!(
        "{}│{}  21. List Volume GUIDs               {}│{}",
        RED, WHITE, RED, RESET
    );
    println!(
        "{}│{}  22. Spoof Volume GUIDs              {}│{}",
        RED, WHITE, RED, RESET
    );
    println!(
        "{}│{}  23. List Registry Traces            {}│{}",
        RED, WHITE, RED, RESET
    );
    println!(
        "{}│{}  24. Spoof Registry Traces           {}│{}",
        RED, WHITE, RED, RESET
    );
    println!(
        "{}│{}  25. List ARP Cache                  {}│{}",
        RED, WHITE, RED, RESET
    );
    println!(
        "{}│{}  26. Spoof ARP Cache                 {}│{}",
        RED, WHITE, RED, RESET
    );
    println!(
        "{}│{}  27. List EFI Variables              {}│{}",
        RED, WHITE, RED, RESET
    );
    println!(
        "{}│{}  28. Spoof EFI (PlatformData)        {}│{}",
        RED, WHITE, RED, RESET
    );
    println!(
        "{}│{}  29. List Boot Identifiers           {}│{}",
        RED, WHITE, RED, RESET
    );
    println!(
        "{}│{}  30. Spoof Boot Time/ID              {}│{}",
        RED, WHITE, RED, RESET
    );
    println!(
        "{}│{}  31. List USB Storage Devices        {}│{}",
        RED, WHITE, RED, RESET
    );
    println!(
        "{}│{}  32. Spoof USB Serials               {}│{}",
        RED, WHITE, RED, RESET
    );
    println!(
        "{}│{}  33. View/Change HWID Seed           {}│{}",
        RED, WHITE, RED, RESET
    );
    println!(
        "{}│{}  34. Exit                            {}│{}",
        RED, WHITE, RED, RESET
    );
    println!("{}└──────────────────────────────────────┘{}", RED, RESET);
    print!("\n{}Select option:{} ", GRAY, RESET);
    io::stdout().flush().unwrap();
}

fn read_input() -> String {
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    input
}

fn print_fpga_info(dma: &Dma) {
    if let Some(info) = dma.get_fpga_info() {
        println!(
            "{}[+] FPGA ID: {}, Version: {}.{}{}",
            WHITE, info.id, info.version_major, info.version_minor, RESET
        );
    }
}

fn print_vmware_info(dma: &Dma) {
    if let Some(info) = dma.get_vmware_info() {
        println!(
            "{}[+] VMware VM Connected (Memory: {} MB){}",
            WHITE, info.memory_size_mb, RESET
        );
    } else {
        println!("{}[+] VMware VM Connected{}", WHITE, RESET);
    }
}

fn select_device() -> Result<Dma<'static>> {
    println!();
    println!("{}┌──────────────────────────────────────┐{}", RED, RESET);
    println!(
        "{}│{}         SELECT DEVICE TYPE           {}│{}",
        RED, WHITE, RED, RESET
    );
    println!("{}├──────────────────────────────────────┤{}", RED, RESET);
    println!(
        "{}│{}  1. FPGA (DMA Hardware)             {}│{}",
        RED, WHITE, RED, RESET
    );
    println!(
        "{}│{}  2. VMware VM                      {}│{}",
        RED, WHITE, RED, RESET
    );
    println!("{}└──────────────────────────────────────┘{}", RED, RESET);
    print!("\n{}Select option:{} ", GRAY, RESET);
    io::stdout().flush().unwrap();

    let choice = read_input();

    match choice.trim() {
        "1" => {
            println!("\n{}[*] Initializing FPGA DMA...{}", GRAY, RESET);
            Dma::new()
        }
        "2" => {
            println!("\n{}[*] Initializing VMware DMA...{}", GRAY, RESET);
            println!("{}[*] Scanning for running VMs...{}", GRAY, RESET);

            let vm_pid = detect_vmware_vms();
            match vm_pid {
                Some(pid) => {
                    println!("{}[+] Found VM with PID: {}{}", WHITE, pid, RESET);
                    Dma::new_vmware_with_pid(Some(pid))
                }
                None => {
                    println!("{}[*] No specific VM detected, using auto-detect...{}", GRAY, RESET);
                    Dma::new_vmware()
                }
            }
        }
        _ => {
            println!("\n{}[*] Defaulting to FPGA...{}", GRAY, RESET);
            Dma::new()
        }
    }
}

fn detect_vmware_vms() -> Option<u32> {
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

fn list_gpu_uuids(dma: &Dma) -> Result<()> {
    println!("\n{}[*] Initializing NVIDIA module...{}", GRAY, RESET);
    let spoofer = NvidiaSpoofer::new(dma)?;

    println!("{}[*] Enumerating GPUs...{}", GRAY, RESET);
    let devices = spoofer.enumerate()?;

    if devices.is_empty() {
        println!("{}[!] No GPUs found{}", RED, RESET);
        return Ok(());
    }

    println!("\n{}[+] Found {} GPU(s):{}", WHITE, devices.len(), RESET);

    for device in &devices {
        println!(
            "\n{}    GPU {} @ 0x{:X}:{}",
            WHITE, device.index, device.address, RESET
        );

        let candidates = spoofer.find_uuid_candidates(device.address)?;
        if candidates.is_empty() {
            println!("{}        No UUID candidates found{}", GRAY, RESET);
        } else {
            println!(
                "{}        Found {} UUID candidate(s):{}",
                GRAY,
                candidates.len(),
                RESET
            );
            for (i, candidate) in candidates.iter().enumerate() {
                let conf_str = match candidate.confidence {
                    UuidConfidence::Exact => "[EXACT]",
                    UuidConfidence::High => "[HIGH]",
                    UuidConfidence::Medium => "[MED]",
                    UuidConfidence::Low => "[LOW]",
                };
                println!(
                    "{}        [{}] {} offset 0x{:04X}: {}{}",
                    WHITE, i, conf_str, candidate.offset, candidate.uuid, RESET
                );
            }
        }
    }

    Ok(())
}

fn find_gpu_by_signature(dma: &Dma) -> Result<()> {
    println!("\n{}[*] Initializing NVIDIA module...{}", GRAY, RESET);
    let spoofer = NvidiaSpoofer::new(dma)?;

    println!("{}[*] Enumerating GPUs...{}", GRAY, RESET);
    let devices = spoofer.enumerate()?;

    if devices.is_empty() {
        println!("{}[!] No GPUs found{}", RED, RESET);
        return Ok(());
    }

    println!(
        "\n{}Enter known UUID from nvidia-smi (e.g. GPU-0138a87f-815e-d978-b363-a6ec28a7d9f6):{}",
        GRAY, RESET
    );
    print!("{}UUID:{} ", GRAY, RESET);
    io::stdout().flush().unwrap();
    let uuid_str = read_input();
    let uuid_str = uuid_str.trim();

    let uuid_bytes = match parse_uuid_string(uuid_str) {
        Some(bytes) => bytes,
        None => {
            println!("{}[!] Invalid UUID format{}", RED, RESET);
            return Ok(());
        }
    };

    println!(
        "{}[*] Searching for UUID bytes: {:02x?}{}",
        GRAY, &uuid_bytes, RESET
    );

    for device in &devices {
        println!(
            "\n{}[*] Scanning GPU {} @ 0x{:X}...{}",
            WHITE, device.index, device.address, RESET
        );

        match spoofer.find_uuid_by_signature(device.address, &uuid_bytes)? {
            Some((base_addr, offset)) => {
                println!(
                    "{}[+] FOUND! UUID at 0x{:X} + 0x{:04X}{}",
                    WHITE, base_addr, offset, RESET
                );

                print!("\n{}Spoof this UUID? (y/n):{} ", GRAY, RESET);
                io::stdout().flush().unwrap();
                let choice = read_input();

                if choice.trim().to_lowercase() == "y" {
                    let new_uuid = generate_new_uuid();
                    println!("{}[*] New UUID: {}{}", GRAY, format_uuid(&new_uuid), RESET);
                    spoofer.write_uuid_at(base_addr, offset, &new_uuid)?;
                    println!("{}[+] UUID spoofed successfully!{}", WHITE, RESET);

                    let verify = spoofer.read_uuid_at(base_addr, offset)?;
                    println!("{}[*] Verification: {}{}", GRAY, verify, RESET);
                }
            }
            None => {
                println!(
                    "{}[!] UUID not found in GPU object or child structures{}",
                    RED, RESET
                );

                println!("\n{}[*] Dumping structure pointers...{}", GRAY, RESET);
                if let Ok(data) = dma.read(4, device.address, 0x100) {
                    println!("{}    First 16 pointers:{}", GRAY, RESET);
                    for i in 0..16 {
                        let off = i * 8;
                        if off + 8 <= data.len() {
                            let ptr = u64::from_le_bytes(data[off..off + 8].try_into().unwrap());
                            if ptr != 0 {
                                println!("{}    [0x{:02X}] = 0x{:016X}{}", GRAY, off, ptr, RESET);
                            }
                        }
                    }
                }

                println!(
                    "\n{}[*] Searching for UUID in extended memory (0x10000 bytes)...{}",
                    GRAY, RESET
                );
                if let Ok(data) = dma.read(4, device.address, 0x10000) {
                    let mut found = false;
                    for i in 0..data.len().saturating_sub(16) {
                        if &data[i..i + 16] == &uuid_bytes {
                            println!("{}[+] FOUND UUID at offset 0x{:X}!{}", WHITE, i, RESET);
                            found = true;
                        }
                    }
                    if !found {
                        println!("{}    UUID not found in 64KB scan{}", GRAY, RESET);
                    }
                }

                println!(
                    "\n{}[*] Checking known offsets for any initialized UUIDs...{}",
                    GRAY, RESET
                );
                let known = spoofer.check_known_offsets(device.address)?;
                if known.is_empty() {
                    println!(
                        "{}[!] No initialized UUIDs found at known offsets{}",
                        RED, RESET
                    );
                } else {
                    println!(
                        "{}[*] Found {} potential UUID(s) at known offsets:{}",
                        GRAY,
                        known.len(),
                        RESET
                    );
                    for (offset, uuid) in &known {
                        println!("{}    0x{:04X}: {}{}", WHITE, offset, uuid, RESET);
                    }
                }
            }
        }
    }

    Ok(())
}

fn parse_uuid_string(s: &str) -> Option<[u8; 16]> {
    let s = s.trim().to_uppercase();
    let s = s.strip_prefix("GPU-").unwrap_or(&s);
    let hex: String = s.chars().filter(|c| c.is_ascii_hexdigit()).collect();

    if hex.len() != 32 {
        return None;
    }

    let mut bytes = [0u8; 16];
    for i in 0..16 {
        bytes[i] = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(bytes)
}

fn spoof_gpu_uuids(dma: &Dma) -> Result<()> {
    println!("\n{}[*] Initializing NVIDIA module...{}", GRAY, RESET);
    let spoofer = NvidiaSpoofer::new(dma)?;

    println!("{}[*] Enumerating GPUs...{}", GRAY, RESET);
    let devices = spoofer.enumerate()?;

    if devices.is_empty() {
        println!("{}[!] No GPUs found{}", RED, RESET);
        return Ok(());
    }

    for device in &devices {
        println!(
            "\n{}[*] GPU {} @ 0x{:X}{}",
            WHITE, device.index, device.address, RESET
        );

        let candidates = spoofer.find_uuid_candidates(device.address)?;
        if candidates.is_empty() {
            println!(
                "{}[!] No UUID candidates found for GPU {}{}",
                RED, device.index, RESET
            );
            continue;
        }

        println!("{}[*] UUID candidates:{}", GRAY, RESET);
        for (i, candidate) in candidates.iter().enumerate() {
            println!(
                "{}    [{}] offset 0x{:04X}: {}{}",
                WHITE, i, candidate.offset, candidate.uuid, RESET
            );
        }

        print!(
            "\n{}Select UUID to spoof (0-{}) or 'a' for all, 's' to skip:{} ",
            GRAY,
            candidates.len() - 1,
            RESET
        );
        io::stdout().flush().unwrap();
        let choice = read_input();
        let choice = choice.trim();

        if choice == "s" {
            println!("{}[*] Skipping GPU {}{}", GRAY, device.index, RESET);
            continue;
        }

        let new_uuid = generate_new_uuid();
        println!("{}[*] New UUID: {}{}", GRAY, format_uuid(&new_uuid), RESET);

        if choice == "a" {
            for candidate in &candidates {
                spoofer.write_uuid_at(device.address, candidate.offset, &new_uuid)?;
                println!(
                    "{}    [+] Patched offset 0x{:04X}{}",
                    WHITE, candidate.offset, RESET
                );
            }
        } else if let Ok(idx) = choice.parse::<usize>() {
            if idx < candidates.len() {
                let candidate = &candidates[idx];
                spoofer.write_uuid_at(device.address, candidate.offset, &new_uuid)?;
                println!(
                    "{}[+] Patched offset 0x{:04X}{}",
                    WHITE, candidate.offset, RESET
                );
            } else {
                println!("{}[!] Invalid selection{}", RED, RESET);
                continue;
            }
        } else {
            println!("{}[!] Invalid selection{}", RED, RESET);
            continue;
        }

        println!("\n{}[*] Verifying...{}", GRAY, RESET);
        let updated_candidates = spoofer.find_uuid_candidates(device.address)?;
        for candidate in &updated_candidates {
            println!(
                "{}    offset 0x{:04X}: {}{}",
                WHITE, candidate.offset, candidate.uuid, RESET
            );
        }
    }

    println!("\n{}[+] GPU spoof complete!{}", WHITE, RESET);

    Ok(())
}

fn list_smbios(dma: &Dma) -> Result<()> {
    println!("\n{}[*] Initializing SMBIOS module...{}", GRAY, RESET);
    let spoofer = SmbiosSpoofer::new(dma)?;

    spoofer.list()?;

    Ok(())
}

fn spoof_smbios(dma: &Dma) -> Result<()> {
    println!("\n{}[*] Initializing SMBIOS module...{}", GRAY, RESET);
    let spoofer = SmbiosSpoofer::new(dma)?;

    println!("{}[*] Spoofing SMBIOS tables...{}", GRAY, RESET);
    let count = spoofer.spoof()?;

    println!("\n{}[+] Spoofed {} SMBIOS tables!{}", WHITE, count, RESET);

    Ok(())
}

fn list_disk(dma: &Dma) -> Result<()> {
    println!("\n{}[*] Initializing Disk module...{}", GRAY, RESET);
    let spoofer = DiskSpoofer::new(dma)?;

    spoofer.list()?;

    Ok(())
}

fn spoof_disk(dma: &Dma) -> Result<()> {
    println!("\n{}[*] Initializing Disk module...{}", GRAY, RESET);
    let spoofer = DiskSpoofer::new(dma)?;

    spoofer.spoof()?;

    Ok(())
}

fn list_nic(dma: &Dma) -> Result<()> {
    println!("\n{}[*] Initializing NIC module...{}", GRAY, RESET);
    let mut spoofer = NicSpoofer::new(dma)?;

    spoofer.list()?;

    Ok(())
}

fn spoof_nic(dma: &Dma) -> Result<()> {
    println!("\n{}[*] Initializing NIC module...{}", GRAY, RESET);
    let mut spoofer = NicSpoofer::new(dma)?;

    let spoofed_macs = spoofer.spoof_and_get_macs()?;

    if !spoofed_macs.is_empty() {
        println!(
            "\n{}[*] Setting registry NetworkAddress values...{}",
            GRAY, RESET
        );

        let reg_spoofer = RegistrySpoofer::new(dma.vmm());

        if let Err(e) = reg_spoofer.set_nic_mac(&spoofed_macs[0]) {
            println!("{}[!] Registry spoof failed: {}{}", RED, e, RESET);
        } else {
            println!("{}[+] Registry NetworkAddress values set{}", WHITE, RESET);
        }
    }

    Ok(())
}

fn generate_new_uuid() -> [u8; 16] {
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    let seed_path = Path::new("hwid_seed.json");
    let config = SeedConfig::load(seed_path).unwrap_or_else(|| {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;
        SeedConfig::new(seed)
    });

    let mut generator = SerialGenerator::from_config(config);
    let uuid_str = generator.generate_uuid();

    if let Err(e) = generator.to_config().save(seed_path) {
        println!("{}[!] Failed to save seed config: {}{}", RED, e, RESET);
    }

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

    let mut uuid = [0u8; 16];
    if uuid_bytes.len() == 16 {
        uuid.copy_from_slice(&uuid_bytes);
    }
    uuid
}

fn format_uuid(uuid: &[u8; 16]) -> String {
    GpuUuid::from_bytes(*uuid).format()
}

fn list_intel_wifi(dma: &Dma) -> Result<()> {
    println!("\n{}[*] Initializing Intel WiFi module...{}", GRAY, RESET);
    let mut spoofer = IntelWifiSpoofer::new(dma)?;

    spoofer.list()?;

    Ok(())
}

fn spoof_intel_wifi(dma: &Dma) -> Result<()> {
    println!("\n{}[*] Initializing Intel WiFi module...{}", GRAY, RESET);
    let mut spoofer = IntelWifiSpoofer::new(dma)?;

    let mut new_mac: [u8; 6] = generate_random_bytes(6).try_into().unwrap();
    new_mac[0] &= 0xFE;

    spoofer.spoof(&new_mac)?;

    println!(
        "\n{}[*] Note: Intel WiFi requires reboot to apply changes{}",
        GRAY, RESET
    );

    Ok(())
}

fn list_monitors(dma: &Dma) -> Result<()> {
    println!("\n{}[*] Initializing Monitor module...{}", GRAY, RESET);
    let spoofer = MonitorSpoofer::new(dma.vmm(), 0);

    spoofer.list()?;

    Ok(())
}

fn spoof_monitors(dma: &Dma) -> Result<()> {
    println!("\n{}[*] Initializing Monitor module...{}", GRAY, RESET);

    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let mut spoofer = MonitorSpoofer::new(dma.vmm(), seed);

    spoofer.spoof()?;

    Ok(())
}

fn list_dxgkrnl_edid(dma: &Dma) -> Result<()> {
    println!(
        "\n{}[*] Initializing dxgkrnl EDID cache module...{}",
        GRAY, RESET
    );

    let spoofer = DxgkrnlEdidSpoofer::new(dma, 0)?;
    spoofer.list()?;

    Ok(())
}

fn spoof_dxgkrnl_edid(dma: &Dma) -> Result<()> {
    println!(
        "\n{}[*] Initializing dxgkrnl EDID cache module...{}",
        GRAY, RESET
    );

    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let mut spoofer = DxgkrnlEdidSpoofer::new(dma, seed)?;
    spoofer.spoof()?;

    Ok(())
}

fn list_tpm(dma: &Dma) -> Result<()> {
    println!("\n{}[*] TPM Registry Info{}", WHITE, RESET);

    let spoofer = TpmSpoofer::new(dma);
    spoofer.list()?;

    Ok(())
}

fn spoof_tpm(dma: &Dma) -> Result<()> {
    println!("\n{}[*] TPM EK Spoofer{}", WHITE, RESET);
    println!(
        "{}    1. Full - Registry + Hook (requires RWX codecave){}",
        GRAY, RESET
    );
    println!(
        "{}    2. Registry only - Spoof cached EK values{}",
        GRAY, RESET
    );
    println!("{}    3. Clear - Zero out registry values{}", GRAY, RESET);
    println!("{}    4. Hook only - Install dispatch hook{}", GRAY, RESET);
    println!("{}    5. Cancel{}", GRAY, RESET);

    print!("{}Choice:{} ", GRAY, RESET);
    io::stdout().flush().unwrap();
    let choice = read_input();

    let mut spoofer = TpmSpoofer::new(dma);

    match choice.trim() {
        "1" => match spoofer.spoof() {
            Ok(_) => {
                if spoofer.is_hooked() {
                    println!(
                        "\n{}[+] TPM fully spoofed (registry + hook){}",
                        WHITE, RESET
                    );
                    wait_for_hook_removal(&mut spoofer)?;
                } else {
                    println!(
                        "\n{}[+] TPM registry spoofed (no RWX codecave for hook){}",
                        WHITE, RESET
                    );
                }
            }
            Err(e) => println!("{}[!] TPM spoof failed: {}{}", RED, e, RESET),
        },
        "2" => match spoofer.spoof_registry_only() {
            Ok(_) => println!("{}[+] TPM registry spoofed{}", WHITE, RESET),
            Err(e) => println!("{}[!] Registry spoof failed: {}{}", RED, e, RESET),
        },
        "3" => match spoofer.clear() {
            Ok(_) => println!("{}[+] TPM registry cleared{}", WHITE, RESET),
            Err(e) => println!("{}[!] Clear failed: {}{}", RED, e, RESET),
        },
        "4" => match spoofer.install_hook() {
            Ok(_) => {
                println!("{}[+] TPM hook installed{}", WHITE, RESET);
                wait_for_hook_removal(&mut spoofer)?;
            }
            Err(e) => println!("{}[!] Hook failed: {}{}", RED, e, RESET),
        },
        _ => {
            println!("{}[*] Cancelled{}", GRAY, RESET);
        }
    }

    Ok(())
}

fn wait_for_hook_removal(spoofer: &mut TpmSpoofer) -> Result<()> {
    println!("\n{}Press Enter to remove hook and exit...{}", GRAY, RESET);
    let _ = read_input();

    if let Err(e) = spoofer.remove_hook() {
        println!("{}[!] Failed to remove hook: {}{}", RED, e, RESET);
    } else {
        println!("{}[+] Hook removed{}", WHITE, RESET);
    }
    Ok(())
}

fn scan_physical_memory_for_uuid(dma: &Dma) -> Result<()> {
    println!("\n{}[*] NVIDIA Driver Memory UUID Scanner{}", WHITE, RESET);

    println!("\n{}[*] Initializing NVIDIA module...{}", GRAY, RESET);
    let spoofer = NvidiaSpoofer::new(dma)?;

    let devices = spoofer.enumerate()?;
    if devices.is_empty() {
        println!("{}[!] No GPUs found{}", RED, RESET);
        return Ok(());
    }

    let gpu0_addr = devices[0].address;
    let driver_base = spoofer.driver_base();
    let driver_size = spoofer.driver_size();

    println!("{}[+] GPU 0 @ 0x{:X}{}", WHITE, gpu0_addr, RESET);
    println!(
        "{}[+] NVIDIA driver @ 0x{:X} (size: 0x{:X}){}",
        WHITE, driver_base, driver_size, RESET
    );

    println!(
        "\n{}Enter UUID from nvidia-smi (e.g. GPU-0138a87f-815e-d978-b363-a6ec28a7d9f6):{}",
        GRAY, RESET
    );
    print!("{}UUID:{} ", GRAY, RESET);
    io::stdout().flush().unwrap();
    let uuid_str = read_input();

    let uuid_bytes = match parse_uuid_string(uuid_str.trim()) {
        Some(bytes) => bytes,
        None => {
            println!("{}[!] Invalid UUID format{}", RED, RESET);
            return Ok(());
        }
    };

    println!(
        "{}[*] Searching for bytes: {:02x?}{}",
        GRAY, &uuid_bytes, RESET
    );

    let mut found_locations: Vec<u64> = Vec::new();
    let chunk_size: usize = 0x100000;

    println!(
        "{}[*] Scanning NVIDIA driver memory region...{}",
        GRAY, RESET
    );
    println!(
        "{}    Range: 0x{:X} - 0x{:X}{}",
        GRAY,
        driver_base,
        driver_base + driver_size,
        RESET
    );

    let mut offset = 0u64;
    while offset < driver_size {
        let addr = driver_base + offset;
        let read_size = chunk_size.min((driver_size - offset) as usize);

        if let Ok(data) = dma.read(4, addr, read_size) {
            for i in 0..data.len().saturating_sub(16) {
                if &data[i..i + 16] == &uuid_bytes {
                    let found_addr = addr + i as u64;
                    found_locations.push(found_addr);
                    println!(
                        "{}[+] Found at 0x{:X} (driver+0x{:X}){}",
                        WHITE,
                        found_addr,
                        found_addr - driver_base,
                        RESET
                    );
                }
            }
        }

        offset += chunk_size as u64;
        if offset % 0x10000000 == 0 {
            println!(
                "{}    Scanned {} MB / {} MB...{}",
                GRAY,
                offset / 0x100000,
                driver_size / 0x100000,
                RESET
            );
        }
    }

    if found_locations.is_empty() {
        println!(
            "\n{}[!] UUID not found in NVIDIA driver memory{}",
            RED, RESET
        );
        println!(
            "{}    The UUID may not be cached yet - run nvidia-smi first{}",
            GRAY, RESET
        );
        return Ok(());
    }

    println!(
        "\n{}[+] Found {} occurrence(s){}",
        WHITE,
        found_locations.len(),
        RESET
    );

    print!(
        "\n{}Replace all occurrences with random UUID? (y/n):{} ",
        GRAY, RESET
    );
    io::stdout().flush().unwrap();
    let choice = read_input();

    if choice.trim().to_lowercase() == "y" {
        let new_uuid = generate_new_uuid();
        println!("{}[*] New UUID: {}{}", GRAY, format_uuid(&new_uuid), RESET);

        for &virt_addr in &found_locations {
            if let Err(e) = dma.write(4, virt_addr, &new_uuid) {
                println!(
                    "{}[!] Failed to write at 0x{:X}: {}{}",
                    RED, virt_addr, e, RESET
                );
            } else {
                println!("{}[+] Replaced at 0x{:X}{}", WHITE, virt_addr, RESET);
            }
        }

        println!("\n{}[+] Done! Run nvidia-smi to verify{}", WHITE, RESET);
    }

    Ok(())
}

fn dse_status(dma: &Dma) -> Result<()> {
    println!(
        "\n{}[*] DSE (Driver Signature Enforcement) Status{}",
        WHITE, RESET
    );

    let mut patcher = DsePatcher::new(dma)?;

    match patcher.find_g_ci_options() {
        Ok(addr) => {
            println!("{}[+] g_CiOptions address: 0x{:X}{}", WHITE, addr, RESET);

            match patcher.get_status_string() {
                Ok(status) => {
                    println!("{}[+] DSE Status: {}{}", WHITE, status, RESET);
                }
                Err(e) => {
                    println!("{}[!] Failed to read DSE status: {}{}", RED, e, RESET);
                }
            }
        }
        Err(e) => {
            println!("{}[!] Failed to locate g_CiOptions: {}{}", RED, e, RESET);
        }
    }

    Ok(())
}

fn dse_disable(dma: &Dma) -> Result<()> {
    println!(
        "\n{}[*] Disable DSE (Driver Signature Enforcement){}",
        WHITE, RESET
    );
    println!(
        "{}    WARNING: This disables kernel code signing enforcement!{}",
        RED, RESET
    );
    println!("{}    This allows loading unsigned drivers.{}", RED, RESET);
    println!(
        "{}    PatchGuard may still trigger BSOD on kernel modifications.{}",
        RED, RESET
    );

    print!("\n{}Continue? (type 'yes' to confirm):{} ", GRAY, RESET);
    io::stdout().flush().unwrap();
    let choice = read_input();

    if choice.trim().to_lowercase() != "yes" {
        println!("{}[*] Aborted{}", GRAY, RESET);
        return Ok(());
    }

    let mut patcher = DsePatcher::new(dma)?;

    match patcher.disable_dse() {
        Ok(_) => {
            println!("\n{}[+] DSE DISABLED{}", WHITE, RESET);
            println!(
                "{}    You can now load unsigned kernel drivers{}",
                GRAY, RESET
            );
            println!(
                "{}    Use option 19 to re-enable DSE when done{}",
                GRAY, RESET
            );
        }
        Err(e) => {
            println!("{}[!] Failed to disable DSE: {}{}", RED, e, RESET);
        }
    }

    Ok(())
}

fn dse_enable(dma: &Dma) -> Result<()> {
    println!(
        "\n{}[*] Enable DSE (Driver Signature Enforcement){}",
        WHITE, RESET
    );

    let mut patcher = DsePatcher::new(dma)?;

    let _ = patcher.find_g_ci_options();

    match patcher.enable_dse() {
        Ok(_) => {
            println!("\n{}[+] DSE ENABLED{}", WHITE, RESET);
        }
        Err(e) => {
            println!("{}[!] Failed to enable DSE: {}{}", RED, e, RESET);
        }
    }

    Ok(())
}

fn patchguard_bypass(dma: &Dma) -> Result<()> {
    println!("\n{}[*] Bypass PatchGuard{}", WHITE, RESET);
    println!(
        "{}    WARNING: This modifies critical kernel structures!{}",
        RED, RESET
    );
    println!(
        "{}    - Patches KiSwInterruptDispatch with RET{}",
        RED, RESET
    );
    println!("{}    - Clears PatchGuard timer DPCs{}", RED, RESET);
    println!("{}    - Clears MaxDataSize pointer{}", RED, RESET);
    println!("{}    - Hooks MmAccessFault (Barricade){}", RED, RESET);
    println!("{}    - Flips NX bits on RWX pages{}", RED, RESET);
    println!("{}    System may become unstable!{}", RED, RESET);

    print!("\n{}Continue? (type 'yes' to confirm):{} ", GRAY, RESET);
    io::stdout().flush().unwrap();
    let choice = read_input();

    if choice.trim().to_lowercase() != "yes" {
        println!("{}[*] Aborted{}", GRAY, RESET);
        return Ok(());
    }

    match PatchGuardBypass::new(dma) {
        Ok(bypass) => match bypass.bypass_with_barricade() {
            Ok(_) => {
                println!("\n{}[+] PATCHGUARD BYPASSED WITH BARRICADE{}", WHITE, RESET);
                println!(
                    "{}    MmAccessFault hooked - PG execution will be caught{}",
                    GRAY, RESET
                );
                println!(
                    "{}    You can now safely modify kernel structures{}",
                    GRAY, RESET
                );
                println!(
                    "{}    TPM spoofing should now work without BSOD{}",
                    GRAY, RESET
                );
            }
            Err(e) => {
                println!("{}[!] Failed to bypass PatchGuard: {}{}", RED, e, RESET);
            }
        },
        Err(e) => {
            println!(
                "{}[!] Failed to initialize PatchGuard bypass: {}{}",
                RED, e, RESET
            );
        }
    }

    Ok(())
}

fn list_volume_guids(dma: &Dma) -> Result<()> {
    println!("\n{}[*] Initializing Volume module...{}", GRAY, RESET);
    let spoofer = VolumeSpoofer::new(dma)?;

    spoofer.list()?;

    Ok(())
}

fn spoof_volume_guids(dma: &Dma) -> Result<()> {
    println!("\n{}[*] Initializing Volume module...{}", GRAY, RESET);
    let spoofer = VolumeSpoofer::new(dma)?;

    spoofer.spoof()?;

    Ok(())
}

fn list_registry_traces(dma: &Dma) -> Result<()> {
    println!(
        "\n{}[*] Initializing Registry Trace module...{}",
        GRAY, RESET
    );
    let spoofer = RegistryTraceSpoofer::new(dma.vmm())?;

    spoofer.list()?;

    Ok(())
}

fn spoof_registry_traces(dma: &Dma) -> Result<()> {
    println!(
        "\n{}[*] Initializing Registry Trace module...{}",
        GRAY, RESET
    );
    let mut spoofer = RegistryTraceSpoofer::new(dma.vmm())?;

    spoofer.spoof()?;

    Ok(())
}

fn list_arp_cache(dma: &Dma) -> Result<()> {
    println!("\n{}[*] Initializing ARP Cache module...{}", GRAY, RESET);
    let mut spoofer = ArpSpoofer::new(dma)?;

    spoofer.enumerate()?;
    spoofer.print_entries();

    Ok(())
}

fn spoof_arp_cache(dma: &Dma) -> Result<()> {
    println!("\n{}[*] Initializing ARP Cache module...{}", GRAY, RESET);
    let mut spoofer = ArpSpoofer::new(dma)?;

    println!("{}[*] Enumerating ARP entries...{}", GRAY, RESET);
    spoofer.enumerate()?;
    spoofer.print_entries();

    if spoofer.list().is_empty() {
        println!("{}[!] No ARP entries found to spoof{}", RED, RESET);
        return Ok(());
    }

    println!("\n{}Options:{}", WHITE, RESET);
    println!("{}  1. Spoof specific MAC{}", GRAY, RESET);
    println!("{}  2. Spoof all entries{}", GRAY, RESET);
    print!("{}Choice:{} ", GRAY, RESET);
    io::stdout().flush().unwrap();
    let choice = read_input();

    match choice.trim() {
        "1" => {
            print!(
                "{}Enter MAC to replace (XX:XX:XX:XX:XX:XX):{} ",
                GRAY, RESET
            );
            io::stdout().flush().unwrap();
            let old_mac_str = read_input();

            print!("{}Enter new MAC (XX:XX:XX:XX:XX:XX):{} ", GRAY, RESET);
            io::stdout().flush().unwrap();
            let new_mac_str = read_input();

            let old_mac = parse_mac(&old_mac_str)?;
            let new_mac = parse_mac(&new_mac_str)?;

            let count = spoofer.spoof_mac(&old_mac, &new_mac)?;
            println!("{}[+] Spoofed {} entries{}", WHITE, count, RESET);
        }
        "2" => {
            let count = spoofer.spoof_all()?;
            println!("{}[+] Spoofed {} entries{}", WHITE, count, RESET);
        }
        _ => {
            println!("{}[!] Invalid choice{}", RED, RESET);
        }
    }

    Ok(())
}

fn parse_mac(s: &str) -> Result<[u8; 6]> {
    let parts: Vec<&str> = s.trim().split(':').collect();
    if parts.len() != 6 {
        anyhow::bail!("Invalid MAC format");
    }

    let mut mac = [0u8; 6];
    for (i, part) in parts.iter().enumerate() {
        mac[i] = u8::from_str_radix(part, 16).map_err(|_| anyhow::anyhow!("Invalid hex byte"))?;
    }

    Ok(mac)
}

fn list_efi(dma: &Dma) -> Result<()> {
    println!("\n{}[*] Initializing EFI module...{}", GRAY, RESET);

    match EfiSpoofer::new(dma) {
        Ok(spoofer) => {
            spoofer.list()?;
        }
        Err(e) => {
            println!("{}[!] EFI not available: {}{}", RED, e, RESET);
            println!(
                "{}    This system may not be UEFI or EFI runtime services are disabled{}",
                GRAY, RESET
            );
        }
    }

    Ok(())
}

fn spoof_efi(dma: &Dma) -> Result<()> {
    println!("\n{}[*] EFI/NVRAM Variable Spoofer{}", WHITE, RESET);
    println!(
        "{}    WARNING: This hooks EFI GetVariable in kernel{}",
        RED, RESET
    );
    println!(
        "{}    Spoofs PlatformData variable tracked by Vanguard{}",
        GRAY, RESET
    );

    print!("\n{}Continue? (y/n):{} ", GRAY, RESET);
    io::stdout().flush().unwrap();
    let choice = read_input();

    if choice.trim().to_lowercase() != "y" {
        println!("{}[*] Aborted{}", GRAY, RESET);
        return Ok(());
    }

    let mut spoofer = EfiSpoofer::new(dma)?;

    match spoofer.spoof() {
        Ok(_) => {
            println!("\n{}[+] EFI hook active{}", WHITE, RESET);
            println!("{}    PlatformData will return spoofed data{}", GRAY, RESET);

            loop {
                println!("\n{}Options:{}", GRAY, RESET);
                println!("{}  1. List EFI runtime services{}", GRAY, RESET);
                println!("{}  2. Remove hook and exit{}", GRAY, RESET);
                print!("{}Choice:{} ", GRAY, RESET);
                io::stdout().flush().unwrap();
                let choice = read_input();

                match choice.trim() {
                    "1" => {
                        spoofer.list()?;
                    }
                    "2" | "" => {
                        if let Err(e) = spoofer.remove_hook() {
                            println!("{}[!] Failed to remove hook: {}{}", RED, e, RESET);
                        } else {
                            println!("{}[+] Hook removed{}", WHITE, RESET);
                        }
                        break;
                    }
                    _ => println!("{}Invalid choice{}", RED, RESET),
                }
            }
        }
        Err(e) => {
            println!("{}[!] EFI spoof failed: {}{}", RED, e, RESET);
        }
    }

    Ok(())
}

fn list_boot(dma: &Dma) -> Result<()> {
    println!("\n{}[*] Initializing Boot module...{}", GRAY, RESET);
    let spoofer = BootSpoofer::new(dma)?;
    spoofer.list()?;
    Ok(())
}

fn spoof_boot(dma: &Dma) -> Result<()> {
    println!("\n{}[*] Boot Time/ID Spoofer{}", WHITE, RESET);
    println!(
        "{}    Spoofs SharedUserData->BootId and KeBootTime{}",
        GRAY, RESET
    );

    print!("\n{}Continue? (y/n):{} ", GRAY, RESET);
    io::stdout().flush().unwrap();
    let choice = read_input();

    if choice.trim().to_lowercase() != "y" {
        println!("{}[*] Aborted{}", GRAY, RESET);
        return Ok(());
    }

    let mut spoofer = BootSpoofer::new(dma)?;
    spoofer.spoof()?;

    Ok(())
}

fn list_usb(dma: &Dma) -> Result<()> {
    println!("\n{}[*] Initializing USB module...{}", GRAY, RESET);

    match UsbSpoofer::new(dma) {
        Ok(mut spoofer) => {
            spoofer.list()?;
        }
        Err(e) => {
            println!("{}[!] USB module error: {}{}", RED, e, RESET);
        }
    }

    Ok(())
}

fn spoof_usb(dma: &Dma) -> Result<()> {
    println!("\n{}[*] USB Storage Serial Spoofer{}", WHITE, RESET);
    println!(
        "{}    Spoofs USBSTOR device extension serial strings{}",
        GRAY, RESET
    );

    print!("\n{}Continue? (y/n):{} ", GRAY, RESET);
    io::stdout().flush().unwrap();
    let choice = read_input();

    if choice.trim().to_lowercase() != "y" {
        println!("{}[*] Aborted{}", GRAY, RESET);
        return Ok(());
    }

    let mut spoofer = UsbSpoofer::new(dma)?;
    spoofer.spoof()?;

    Ok(())
}

fn manage_hwid_seed() -> Result<()> {
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    let seed_path = Path::new("hwid_seed.json");

    let config = SeedConfig::load(seed_path).unwrap_or_else(|| {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;
        SeedConfig::new(seed)
    });

    let mut generator = SerialGenerator::from_config(config);

    println!("\n{}[*] HWID Seed Manager{}", WHITE, RESET);
    println!("{}    Current seed: {}{}", GRAY, generator.seed(), RESET);

    if let Some(disk) = &generator.generated().disk_serial {
        println!("{}    Disk serial: {}{}", GRAY, disk, RESET);
    }
    if !generator.generated().mac_addresses.is_empty() {
        println!(
            "{}    MAC addresses: {:?}{}",
            GRAY,
            generator.generated().mac_addresses,
            RESET
        );
    }
    if let Some(gpu) = &generator.generated().gpu_uuid {
        println!("{}    GPU UUID: {}{}", GRAY, gpu, RESET);
    }

    println!("\n{}Options:{}", WHITE, RESET);
    println!("{}  1. Generate new random seed{}", GRAY, RESET);
    println!("{}  2. Enter custom seed{}", GRAY, RESET);
    println!("{}  3. Preview generated serials{}", GRAY, RESET);
    println!("{}  4. Save and exit{}", GRAY, RESET);
    println!("{}  5. Cancel{}", GRAY, RESET);

    print!("{}Choice:{} ", GRAY, RESET);
    io::stdout().flush().unwrap();
    let choice = read_input();

    match choice.trim() {
        "1" => {
            let new_seed = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64;
            generator.reseed(new_seed);
            println!("{}[+] New seed: {}{}", WHITE, new_seed, RESET);
            generator.to_config().save(seed_path)?;
            println!("{}[+] Saved to hwid_seed.json{}", WHITE, RESET);
        }
        "2" => {
            print!("{}Enter seed (u64):{} ", GRAY, RESET);
            io::stdout().flush().unwrap();
            let seed_str = read_input();
            if let Ok(new_seed) = seed_str.trim().parse::<u64>() {
                generator.reseed(new_seed);
                println!("{}[+] Seed set to: {}{}", WHITE, new_seed, RESET);
                generator.to_config().save(seed_path)?;
                println!("{}[+] Saved to hwid_seed.json{}", WHITE, RESET);
            } else {
                println!("{}[!] Invalid seed format{}", RED, RESET);
            }
        }
        "3" => {
            println!("\n{}[*] Preview (not saved):{}", WHITE, RESET);
            println!(
                "{}    Disk (WD): {}{}",
                GRAY,
                generator.generate_disk_serial("WD"),
                RESET
            );
            println!(
                "{}    Disk (Samsung): {}{}",
                GRAY,
                generator.generate_disk_serial("Samsung"),
                RESET
            );
            println!(
                "{}    Disk (NVMe): {}{}",
                GRAY,
                generator.generate_disk_serial("NVMe"),
                RESET
            );
            println!(
                "{}    MAC (Intel): {}{}",
                GRAY,
                generator.generate_mac_with_oui("intel"),
                RESET
            );
            println!(
                "{}    MAC (Realtek): {}{}",
                GRAY,
                generator.generate_mac_with_oui("realtek"),
                RESET
            );
            println!("{}    UUID: {}{}", GRAY, generator.generate_uuid(), RESET);
            println!("{}    GUID: {}{}", GRAY, generator.generate_guid(), RESET);
            println!(
                "{}    SMBIOS System: {}{}",
                GRAY,
                generator.generate_smbios_serial("AMI", "ASUS", 1),
                RESET
            );
            println!(
                "{}    SMBIOS Baseboard: {}{}",
                GRAY,
                generator.generate_smbios_serial("AMI", "ASUS", 2),
                RESET
            );
        }
        "4" => {
            generator.to_config().save(seed_path)?;
            println!("{}[+] Saved to hwid_seed.json{}", WHITE, RESET);
        }
        _ => {
            println!("{}[*] Cancelled{}", GRAY, RESET);
        }
    }

    Ok(())
}
