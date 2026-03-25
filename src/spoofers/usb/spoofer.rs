use anyhow::{anyhow, Result};

use crate::core::Dma;

const USBSTOR_FDO_SIGNATURE: u32 = 0x214F4446;
const USBSTOR_PDO_SIGNATURE: u32 = 0x214F4450;

const EXT_SIGNATURE: u64 = 0x00;
const EXT_SERIAL_LENGTH: u64 = 0x40;
const EXT_SERIAL_STRING: u64 = 0x6C;

const DRIVER_OBJECT_DEVICE_OBJECT: u64 = 0x08;
const DEVICE_OBJECT_NEXT_DEVICE: u64 = 0x10;
const DEVICE_OBJECT_DEVICE_EXTENSION: u64 = 0x40;

#[derive(Debug, Clone)]
pub struct UsbDevice {
    pub device_object: u64,
    pub extension: u64,
    pub serial_length: u32,
    pub serial: String,
    pub serial_addr: u64,
}

pub struct UsbSpoofer<'a> {
    dma: &'a Dma<'a>,
    usbstor_driver: u64,
    devices: Vec<UsbDevice>,
}

impl<'a> UsbSpoofer<'a> {
    pub fn new(dma: &'a Dma<'a>) -> Result<Self> {
        let usbstor_driver = Self::find_usbstor_driver(dma)?;
        println!("[+] USBSTOR.SYS driver object: 0x{:X}", usbstor_driver);

        Ok(Self {
            dma,
            usbstor_driver,
            devices: Vec::new(),
        })
    }

    fn find_usbstor_driver(dma: &Dma) -> Result<u64> {
        let vmm = dma.vmm();
        let drivers = vmm
            .map_kdriver()
            .map_err(|e| anyhow!("Failed to enumerate drivers: {}", e))?;

        let search_terms = ["usbstor", "USBSTOR", "UsbStor"];

        for driver in drivers.iter() {
            let name_lower = driver.name.to_lowercase();
            if search_terms
                .iter()
                .any(|t| name_lower.contains(&t.to_lowercase()))
            {
                println!("[*] Found driver: {} @ 0x{:X}", driver.name, driver.va);
                if driver.va_device_object == 0 {
                    return Err(anyhow!(
                        "USBSTOR found but no devices attached (va_device_object=0)"
                    ));
                }
                return Ok(driver.va);
            }
        }

        println!("[!] Available drivers:");
        for driver in drivers.iter() {
            if driver.name.to_lowercase().contains("usb") {
                println!("    {} @ 0x{:X}", driver.name, driver.va);
            }
        }

        Err(anyhow!(
            "USBSTOR.SYS driver not found - no USB storage devices?"
        ))
    }

    pub fn enumerate(&mut self) -> Result<&Vec<UsbDevice>> {
        self.devices.clear();

        let first_device = self
            .dma
            .read_u64(4, self.usbstor_driver + DRIVER_OBJECT_DEVICE_OBJECT)?;

        if first_device == 0 {
            println!("[*] No USB storage devices attached");
            return Ok(&self.devices);
        }

        let mut current = first_device;
        let mut count = 0;

        while current != 0 && count < 100 {
            if let Ok(device) = self.read_device(current) {
                self.devices.push(device);
            }

            current = self.dma.read_u64(4, current + DEVICE_OBJECT_NEXT_DEVICE)?;
            count += 1;
        }

        Ok(&self.devices)
    }

    fn read_device(&self, device_object: u64) -> Result<UsbDevice> {
        let extension = self
            .dma
            .read_u64(4, device_object + DEVICE_OBJECT_DEVICE_EXTENSION)?;

        if extension == 0 {
            return Err(anyhow!("No device extension"));
        }

        let signature = self.dma.read_u32(4, extension + EXT_SIGNATURE)?;
        if signature == USBSTOR_FDO_SIGNATURE {
            return Err(anyhow!("FDO extension, skipping"));
        }
        if signature != USBSTOR_PDO_SIGNATURE {
            return Err(anyhow!("Invalid signature: 0x{:X}", signature));
        }

        let serial_length = self.dma.read_u32(4, extension + EXT_SERIAL_LENGTH)?;
        let serial_addr = extension + EXT_SERIAL_STRING;

        let serial = if serial_length > 0 && serial_length < 256 {
            let bytes = self.dma.read(4, serial_addr, serial_length as usize)?;
            String::from_utf8_lossy(&bytes)
                .trim_end_matches('\0')
                .to_string()
        } else {
            String::new()
        };

        Ok(UsbDevice {
            device_object,
            extension,
            serial_length,
            serial,
            serial_addr,
        })
    }

    pub fn list(&mut self) -> Result<()> {
        println!("[*] Enumerating USB storage devices...");
        self.enumerate()?;

        if self.devices.is_empty() {
            println!("[!] No USB storage devices found");
            return Ok(());
        }

        println!("\n[+] Found {} USB storage device(s):", self.devices.len());
        println!("{:-<70}", "");
        println!(
            "{:<20} {:<16} {:<30}",
            "Device Object", "Extension", "Serial"
        );
        println!("{:-<70}", "");

        for device in &self.devices {
            println!(
                "0x{:X}  0x{:X}  {}",
                device.device_object,
                device.extension,
                if device.serial.is_empty() {
                    "(none)"
                } else {
                    &device.serial
                }
            );
        }
        println!("{:-<70}", "");

        Ok(())
    }

    pub fn spoof(&mut self) -> Result<u32> {
        self.enumerate()?;

        if self.devices.is_empty() {
            println!("[!] No USB storage devices to spoof");
            return Ok(0);
        }

        let mut spoofed = 0;

        for device in &self.devices {
            if device.serial_length == 0 || device.serial.is_empty() {
                continue;
            }

            let new_serial = self.generate_serial(device.serial_length as usize);

            println!(
                "[*] Spoofing: {} -> {}",
                device.serial,
                String::from_utf8_lossy(&new_serial).trim_end_matches('\0')
            );

            self.dma.write(4, device.serial_addr, &new_serial)?;
            spoofed += 1;
        }

        println!("\n[+] Spoofed {} USB device serial(s)", spoofed);
        Ok(spoofed)
    }

    fn generate_serial(&self, length: usize) -> Vec<u8> {
        let charset = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";
        let mut serial = Vec::with_capacity(length);

        for _ in 0..length {
            let idx = rand::random::<usize>() % charset.len();
            serial.push(charset[idx]);
        }

        serial
    }
}
