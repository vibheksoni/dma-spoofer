#[derive(Debug, Clone, Copy)]
pub struct SmbiosOffsets {
    pub table_physical_address: u64,
    pub table_length: u64,
}

impl SmbiosOffsets {
    pub const fn new(table_physical_address: u64, table_length: u64) -> Self {
        Self {
            table_physical_address,
            table_length,
        }
    }
}

pub const WIN10_2004_BUILD: u32 = 19041;
pub const WIN10_22H2_BUILD: u32 = 19045;
pub const WIN11_24H2_BUILD: u32 = 26100;

pub const WIN10_2004_OFFSETS: SmbiosOffsets = SmbiosOffsets::new(0xD2D100, 0xD2D044);
pub const WIN11_24H2_OFFSETS: SmbiosOffsets = SmbiosOffsets::new(0xFD70F0, 0xFD703C);

pub fn offsets_for_build(build_number: u32) -> Option<SmbiosOffsets> {
    match build_number {
        WIN10_2004_BUILD..=WIN10_22H2_BUILD => Some(WIN10_2004_OFFSETS),
        WIN11_24H2_BUILD..=u32::MAX => Some(WIN11_24H2_OFFSETS),
        _ => None,
    }
}
