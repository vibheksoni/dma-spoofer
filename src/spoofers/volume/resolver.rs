use anyhow::{anyhow, Result};

use crate::core::Dma;

use super::offsets::{layout_for_build, VolumeLayout};

const KERNEL_PID: u32 = 4;
const KUSER_SHARED_DATA: u64 = 0xFFFFF78000000000;
const NT_BUILD_NUMBER_OFFSET: u64 = 0x260;
const MIN_KERNEL_POINTER: u64 = 0xFFFF000000000000;

#[derive(Debug, Clone, Copy)]
pub struct ResolvedVolumeContext {
    pub build_number: u32,
    pub device_object: u64,
    pub layout: VolumeLayout,
}

pub fn resolve_volume_context(dma: &Dma<'_>) -> Result<ResolvedVolumeContext> {
    let build_number = detect_build_number(dma).unwrap_or(0);
    let device_object = resolve_mountmgr_device_object(dma)?;
    let layout = layout_for_build(build_number);

    Ok(ResolvedVolumeContext {
        build_number,
        device_object,
        layout,
    })
}

fn detect_build_number(dma: &Dma<'_>) -> Result<u32> {
    if let Ok(build_number) = dma
        .vmm()
        .get_config(memprocfs::CONFIG_OPT_WIN_VERSION_BUILD)
    {
        if build_number > 0 {
            return Ok(build_number as u32);
        }
    }

    let build_number = dma.read_u32(KERNEL_PID, KUSER_SHARED_DATA + NT_BUILD_NUMBER_OFFSET)?;
    Ok(build_number & 0xFFFF)
}

fn resolve_mountmgr_device_object(dma: &Dma<'_>) -> Result<u64> {
    let drivers = dma.get_kernel_drivers()?;

    let driver = drivers
        .iter()
        .find(|driver| is_mountmgr_driver(&driver.name))
        .ok_or_else(|| anyhow!("mountmgr.sys driver not found"))?;

    if !is_kernel_pointer(driver.device_object) {
        return Err(anyhow!(
            "Invalid mountmgr device object: 0x{:X}",
            driver.device_object
        ));
    }

    Ok(driver.device_object)
}

fn is_mountmgr_driver(name: &str) -> bool {
    name.eq_ignore_ascii_case("mountmgr") || name.eq_ignore_ascii_case("mountmgr.sys")
}

fn is_kernel_pointer(address: u64) -> bool {
    address >= MIN_KERNEL_POINTER
}
