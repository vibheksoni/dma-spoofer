#[derive(Debug, Clone, Copy)]
pub struct VolumeLayout {
    pub extension_mounted_devices_list: u64,
    pub mounted_device_symbolic_links: u64,
    pub mounted_device_device_name: u64,
    pub mounted_device_unique_id: Option<u64>,
    pub symbolic_link_device: u64,
    pub symbolic_link_name: u64,
}

impl VolumeLayout {
    pub const fn new(
        extension_mounted_devices_list: u64,
        mounted_device_symbolic_links: u64,
        mounted_device_device_name: u64,
        mounted_device_unique_id: Option<u64>,
        symbolic_link_device: u64,
        symbolic_link_name: u64,
    ) -> Self {
        Self {
            extension_mounted_devices_list,
            mounted_device_symbolic_links,
            mounted_device_device_name,
            mounted_device_unique_id,
            symbolic_link_device,
            symbolic_link_name,
        }
    }
}

pub const DEVICE_OBJECT_DEVICE_EXTENSION: u64 = 0x40;

pub const WIN10_2004_BUILD: u32 = 19041;
pub const WIN11_21H2_BUILD: u32 = 22000;

pub const WIN10_LAYOUT: VolumeLayout =
    VolumeLayout::new(0x10, 0x10, 0x50, Some(0x60), 0x10, 0x18);
pub const WIN11_LAYOUT: VolumeLayout = VolumeLayout::new(0x10, 0x10, 0x40, Some(0x60), 0x10, 0x18);

pub fn layout_for_build(build_number: u32) -> VolumeLayout {
    match build_number {
        WIN10_2004_BUILD..WIN11_21H2_BUILD => WIN10_LAYOUT,
        WIN11_21H2_BUILD..=u32::MAX => WIN11_LAYOUT,
        _ => WIN11_LAYOUT,
    }
}

pub const LIST_ENTRY_FLINK: u64 = 0x00;

pub const UNICODE_STRING_LENGTH: u64 = 0x00;
pub const UNICODE_STRING_BUFFER: u64 = 0x08;

pub const VOLUME_GUID_PREFIX: &str = "\\??\\Volume{";
pub const VOLUME_GUID_CHAR_COUNT: usize = 36;
pub const VOLUME_GUID_START_OFFSET: usize = 11;
