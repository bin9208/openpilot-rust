//! Pinned ARS510 bit-field catalog; original MIT license in this directory.
#[derive(Clone, Copy, serde::Serialize)]
pub struct NativeField {
    pub name: &'static str,
    pub bit_start: usize,
    pub bit_len: usize,
    pub zero_code: f64,
    pub scale: f64,
    pub unit: &'static str,
    pub status: &'static str,
}
pub const LONG_DIST: NativeField = NativeField {
    name: "long_dist",
    bit_start: 32,
    bit_len: 12,
    zero_code: 160.0,
    scale: 0.0625,
    unit: "m",
    status: "validated",
};
pub const LAT_DIST: NativeField = NativeField {
    name: "lat_dist_left",
    bit_start: 44,
    bit_len: 12,
    zero_code: 2048.0,
    scale: 0.015,
    unit: "m",
    status: "validated_sign_likely_scale",
};
pub const LONG_VEL_GROUND: NativeField = NativeField {
    name: "long_vel_over_ground",
    bit_start: 64,
    bit_len: 10,
    zero_code: 510.5,
    scale: 0.15,
    unit: "m/s",
    status: "validated_with_caveats",
};
pub const LAT_VEL: NativeField = NativeField {
    name: "lat_vel_over_ground",
    bit_start: 74,
    bit_len: 10,
    zero_code: 510.5,
    scale: 0.15,
    unit: "m/s",
    status: "likely",
};
pub const ACCEL_LIKE: NativeField = NativeField {
    name: "accel_like_84",
    bit_start: 84,
    bit_len: 10,
    zero_code: 511.0,
    scale: 1.0,
    unit: "code",
    status: "likely",
};
pub const AGE: NativeField = NativeField {
    name: "age_cycles",
    bit_start: 24,
    bit_len: 7,
    zero_code: 0.0,
    scale: 1.0,
    unit: "cycles",
    status: "structure",
};
pub const MOVE_STATE: NativeField = NativeField {
    name: "move_state",
    bit_start: 109,
    bit_len: 2,
    zero_code: 0.0,
    scale: 1.0,
    unit: "enum",
    status: "legacy_two_bit_view",
};
pub const MOVEMENT_CODE: NativeField = NativeField {
    name: "movement_code",
    bit_start: 109,
    bit_len: 3,
    zero_code: 0.0,
    scale: 1.0,
    unit: "code",
    status: "likely",
};
pub const RAW8_LOW5: NativeField = NativeField {
    name: "raw8_low5",
    bit_start: 8,
    bit_len: 5,
    zero_code: 0.0,
    scale: 1.0,
    unit: "code",
    status: "confirmed",
};
pub const RAW13_BIT: NativeField = NativeField {
    name: "raw13_bit",
    bit_start: 13,
    bit_len: 1,
    zero_code: 0.0,
    scale: 1.0,
    unit: "bit",
    status: "raw",
};
pub const RAW_WEIGHT_STATE128: NativeField = NativeField {
    name: "raw_weight_state128",
    bit_start: 128,
    bit_len: 3,
    zero_code: 0.0,
    scale: 1.0,
    unit: "code",
    status: "confirmed_structure",
};
pub const RAW_WEIGHT_148: NativeField = NativeField {
    name: "raw_weight148",
    bit_start: 148,
    bit_len: 4,
    zero_code: 0.0,
    scale: 1.0,
    unit: "code",
    status: "likely",
};
pub const RAW_WEIGHT_152: NativeField = NativeField {
    name: "raw_weight152",
    bit_start: 152,
    bit_len: 4,
    zero_code: 0.0,
    scale: 1.0,
    unit: "code",
    status: "likely",
};
pub const RAW_WEIGHT_156: NativeField = NativeField {
    name: "raw_weight156",
    bit_start: 156,
    bit_len: 4,
    zero_code: 0.0,
    scale: 1.0,
    unit: "code",
    status: "likely",
};
pub const ONCOMING_FLAG: NativeField = NativeField {
    name: "oncoming_flag",
    bit_start: 14,
    bit_len: 1,
    zero_code: 0.0,
    scale: 1.0,
    unit: "flag",
    status: "tested_semantics",
};
pub const EXISTENCE: NativeField = NativeField {
    name: "existence_pct",
    bit_start: 16,
    bit_len: 8,
    zero_code: 0.0,
    scale: 1.0,
    unit: "%",
    status: "likely",
};
pub const PREDICTED: NativeField = NativeField {
    name: "predicted",
    bit_start: 107,
    bit_len: 1,
    zero_code: 0.0,
    scale: 1.0,
    unit: "flag",
    status: "likely",
};
pub const CAMERA_ASSOC: NativeField = NativeField {
    name: "camera_assoc",
    bit_start: 112,
    bit_len: 3,
    zero_code: 0.0,
    scale: 1.0,
    unit: "state",
    status: "likely",
};
pub const CLASS_CONFIDENCE: NativeField = NativeField {
    name: "class_confidence",
    bit_start: 115,
    bit_len: 5,
    zero_code: 0.0,
    scale: 5.0,
    unit: "%",
    status: "likely",
};
pub const VEL_UNC_240: NativeField = NativeField {
    name: "vel_uncertainty_candidate",
    bit_start: 240,
    bit_len: 7,
    zero_code: 0.0,
    scale: 1.0,
    unit: "code",
    status: "likely",
};
pub const LENGTH: NativeField = NativeField {
    name: "length",
    bit_start: 56,
    bit_len: 7,
    zero_code: 0.0,
    scale: 0.1,
    unit: "m",
    status: "likely",
};
pub const WIDTH: NativeField = NativeField {
    name: "width_minus_one",
    bit_start: 216,
    bit_len: 6,
    zero_code: 0.0,
    scale: 0.1,
    unit: "m",
    status: "likely",
};
pub const UNC_RANGE_224: NativeField = NativeField {
    name: "range_uncertainty_code",
    bit_start: 224,
    bit_len: 7,
    zero_code: 0.0,
    scale: 1.0,
    unit: "code",
    status: "candidate",
};
pub const UNC_LATERAL_232: NativeField = NativeField {
    name: "lateral_uncertainty_code",
    bit_start: 232,
    bit_len: 7,
    zero_code: 0.0,
    scale: 1.0,
    unit: "code",
    status: "likely",
};
pub const UNC_VLAT_248: NativeField = NativeField {
    name: "lateral_speed_uncertainty_code",
    bit_start: 248,
    bit_len: 7,
    zero_code: 0.0,
    scale: 1.0,
    unit: "code",
    status: "candidate",
};
pub const UNC_AX_256: NativeField = NativeField {
    name: "accel_uncertainty_code",
    bit_start: 256,
    bit_len: 8,
    zero_code: 0.0,
    scale: 1.0,
    unit: "code",
    status: "candidate",
};
pub const UNC_AY_264: NativeField = NativeField {
    name: "lateral_accel_uncertainty_code",
    bit_start: 264,
    bit_len: 8,
    zero_code: 0.0,
    scale: 1.0,
    unit: "code",
    status: "candidate",
};
pub const UNC_ORIENT_200: NativeField = NativeField {
    name: "orientation_uncertainty_code",
    bit_start: 200,
    bit_len: 7,
    zero_code: 0.0,
    scale: 1.0,
    unit: "code",
    status: "likely",
};
pub const SECONDARY_SCORE_184: NativeField = NativeField {
    name: "secondary_score_pct",
    bit_start: 184,
    bit_len: 8,
    zero_code: 0.0,
    scale: 1.0,
    unit: "%",
    status: "obstacle_probability_candidate",
};
pub const OBJECT_CLASS: NativeField = NativeField {
    name: "object_class",
    bit_start: 163,
    bit_len: 3,
    zero_code: 0.0,
    scale: 1.0,
    unit: "code",
    status: "likely",
};
pub const HEIGHT_LIKE_272: NativeField = NativeField {
    name: "height_like_code",
    bit_start: 272,
    bit_len: 5,
    zero_code: 0.0,
    scale: 1.0,
    unit: "code",
    status: "candidate",
};
pub const NAMED_FIELDS: &[NativeField] = &[
    AGE,
    LONG_DIST,
    LAT_DIST,
    LONG_VEL_GROUND,
    LAT_VEL,
    ACCEL_LIKE,
    MOVE_STATE,
    MOVEMENT_CODE,
    RAW8_LOW5,
    RAW13_BIT,
    ONCOMING_FLAG,
    VEL_UNC_240,
    EXISTENCE,
    PREDICTED,
    RAW_WEIGHT_STATE128,
    RAW_WEIGHT_148,
    RAW_WEIGHT_152,
    RAW_WEIGHT_156,
    CAMERA_ASSOC,
    CLASS_CONFIDENCE,
    LENGTH,
    WIDTH,
    UNC_RANGE_224,
    UNC_LATERAL_232,
    UNC_VLAT_248,
    UNC_AX_256,
    UNC_AY_264,
    UNC_ORIENT_200,
    SECONDARY_SCORE_184,
    OBJECT_CLASS,
    HEIGHT_LIKE_272,
];
