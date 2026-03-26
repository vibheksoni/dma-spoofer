#[derive(Debug, Clone, Copy)]
pub struct ArpLayout {
    pub compartment_set_global_rva: Option<u64>,
    pub compartment_set_count: u64,
    pub compartment_set_table: u64,
    pub compartment_entry_offset: u64,
    pub compartment_id: u64,
    pub compartment_flags: u64,
    pub compartment_neighbor_table: u64,
    pub hash_table_size: u64,
    pub hash_table_num_entries: u64,
    pub hash_table_directory: u64,
    pub neighbor_entry_offset: u64,
    pub neighbor_interface: u64,
    pub neighbor_state: u64,
    pub neighbor_refcount: u64,
    pub neighbor_dl_address: u64,
}

impl ArpLayout {
    pub const fn new(
        compartment_set_global_rva: Option<u64>,
        compartment_set_count: u64,
        compartment_set_table: u64,
        compartment_entry_offset: u64,
        compartment_id: u64,
        compartment_flags: u64,
        compartment_neighbor_table: u64,
        hash_table_size: u64,
        hash_table_num_entries: u64,
        hash_table_directory: u64,
        neighbor_entry_offset: u64,
        neighbor_interface: u64,
        neighbor_state: u64,
        neighbor_refcount: u64,
        neighbor_dl_address: u64,
    ) -> Self {
        Self {
            compartment_set_global_rva,
            compartment_set_count,
            compartment_set_table,
            compartment_entry_offset,
            compartment_id,
            compartment_flags,
            compartment_neighbor_table,
            hash_table_size,
            hash_table_num_entries,
            hash_table_directory,
            neighbor_entry_offset,
            neighbor_interface,
            neighbor_state,
            neighbor_refcount,
            neighbor_dl_address,
        }
    }
}

pub const WIN10_2004_BUILD: u32 = 19041;
pub const WIN11_21H2_BUILD: u32 = 22000;

pub const WIN10_LAYOUT: ArpLayout = ArpLayout::new(
    Some(0x1F8A70),
    0x4D74,
    0x4D78,
    0x30,
    0x00,
    0x40,
    0x290,
    0x08,
    0x14,
    0x20,
    0x28,
    0x08,
    0x40,
    0x04,
    0x88,
);
pub const WIN11_LAYOUT: ArpLayout = ArpLayout::new(
    Some(0x2206B0),
    0x4D9C,
    0x4DA0,
    0x30,
    0x00,
    0x40,
    0x290,
    0x08,
    0x14,
    0x20,
    0x28,
    0x08,
    0x1C,
    0x60,
    0xA8,
);

pub const DL_ADDRESS_SIZE: usize = 6;

pub const NEIGHBOR_STATE_UNREACHABLE: u32 = 0;
pub const NEIGHBOR_STATE_INCOMPLETE: u32 = 1;
pub const NEIGHBOR_STATE_PROBE: u32 = 2;
pub const NEIGHBOR_STATE_DELAY: u32 = 3;
pub const NEIGHBOR_STATE_STALE: u32 = 4;
pub const NEIGHBOR_STATE_REACHABLE: u32 = 5;
pub const NEIGHBOR_STATE_PERMANENT: u32 = 6;

pub fn layout_for_build(build_number: u32) -> ArpLayout {
    match build_number {
        WIN10_2004_BUILD..WIN11_21H2_BUILD => WIN10_LAYOUT,
        WIN11_21H2_BUILD..=u32::MAX => WIN11_LAYOUT,
        _ => WIN11_LAYOUT,
    }
}
