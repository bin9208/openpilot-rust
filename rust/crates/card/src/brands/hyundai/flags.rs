//! Exact masks from `opendbc/car/hyundai/values.py` (original MIT provenance).
pub const HDA2: u32 = 1;
pub const ALT_BUTTONS: u32 = 1 << 1;
pub const ALT_GEARS: u32 = 1 << 2;
pub const CAMERA_SCC: u32 = 1 << 3;
pub const ALT_LIMITS: u32 = 1 << 4;
pub const ENABLE_BLINKERS: u32 = 1 << 5;
pub const ALT_GEARS_2: u32 = 1 << 6;
pub const SEND_LFA: u32 = 1 << 7;
pub const USE_FCA: u32 = 1 << 8;
pub const ALT_STEERING: u32 = 1 << 9;
pub const HYBRID: u32 = 1 << 10;
pub const EV: u32 = 1 << 11;
pub const MANDO_RADAR: u32 = 1 << 12;
pub const CANFD: u32 = 1 << 13;
pub const RADAR_SCC: u32 = 1 << 14;
pub const CHECKSUM_CRC8: u32 = 1 << 16;
pub const CHECKSUM_6B: u32 = 1 << 17;
pub const LEGACY: u32 = 1 << 18;
pub const UNSUPPORTED_LONGITUDINAL: u32 = 1 << 19;
pub const CANFD_NO_RADAR_DISABLE: u32 = 1 << 20;
pub const CLUSTER_GEARS: u32 = 1 << 21;
pub const TCU_GEARS: u32 = 1 << 22;
pub const MIN_STEER_32_MPH: u32 = 1 << 23;
pub const ANGLE_CONTROL: u32 = 1 << 24;
pub const FCEV: u32 = 1 << 25;
pub const ALT_LIMITS_2: u32 = 1 << 26;
pub const CC_ONLY_CAR: u32 = 1 << 31;

pub mod ext {
    pub const NAVI_CLUSTER: u32 = 1 << 2;
    pub const HAS_LFAHDA: u32 = 1 << 4;
    pub const GEARS_NONE: u32 = 1 << 6;
    pub const RADAR_GROUP1: u32 = 1 << 7;
    pub const GEARS_69: u32 = 1 << 10;
    pub const RADAR_GROUP3: u32 = 1 << 11;
    pub const CORNER_235: u32 = 1 << 12;
    pub const CORNER_180: u32 = 1 << 13;
    pub const CORNER_430: u32 = 1 << 14;
    pub const RADAR_GROUP4: u32 = 1 << 15;
    pub const EV_MODE_230: u32 = 1 << 16;
}

pub mod safety {
    pub const EV_GAS: u16 = 1;
    pub const HYBRID_GAS: u16 = 2;
    pub const LONG: u16 = 4;
    pub const CAMERA_SCC: u16 = 8;
    pub const LKA_STEERING: u16 = 16;
    pub const ALT_BUTTONS: u16 = 32;
    pub const ALT_LIMITS: u16 = 64;
    pub const ALT_STEERING: u16 = 128;
    pub const FCEV_GAS: u16 = 256;
    pub const ALT_LIMITS_2: u16 = 512;
}
