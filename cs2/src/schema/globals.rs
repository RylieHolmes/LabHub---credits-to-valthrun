use cs2_schema_cutl::PtrCStr;
use raw_struct::raw_struct;

#[raw_struct(size = 0x28)]
pub struct EngineBuildInfo {
    #[field(offset = 0x00)]
    pub revision: PtrCStr,

    #[field(offset = 0x08)]
    pub build_date: PtrCStr,

    #[field(offset = 0x10)]
    pub build_time: PtrCStr,

    /* pub unknown_zero: u64 */
    #[field(offset = 0x20)]
    pub product_name: PtrCStr,
}

#[raw_struct(size = 0x1C0)]
pub struct Globals {
    #[field(offset = 0x00)]
    pub time_1: f32,

    #[field(offset = 0x04)]
    pub frame_count_1: u32,

    #[field(offset = 0x10)]
    pub max_player_count: u32,

    #[field(offset = 0x30)]
    pub time_2: f32,

    #[field(offset = 0x38)]
    pub time_3: f32,

    #[field(offset = 0x48)]
    pub frame_count_2: u32,

    #[field(offset = 0x4C)]
    pub two_tick_time: f32,

    // Forum post suggests: 
    // ...
    // float m_flIntervalPerTick2; // at 0x54?
    // pad3[0x158]; // Ends at 0x1B0
    // uint64 m_uCurrentMap; // 0x1B0
    // uint64 m_uCurrentMapName; // 0x1B8

    #[field(offset = 0x1B0)]
    pub current_map_ptr: PtrCStr,

    #[field(offset = 0x1B8)]
    pub current_map_name_ptr: PtrCStr,
}
