//! Decode a 36-byte ARS510 object slot in the source's bit/float order.
use super::fields::*;
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct NativeObject {
    pub slot: i64,
    pub age: i64,
    pub d_rel: f64,
    pub y_rel: f64,
    pub v_long_ground: f64,
    pub v_lat_ground: f64,
    pub accel_like_code: i64,
    pub move_state: i64,
    pub oncoming_flag: bool,
    pub vel_unc_code: i64,
    pub geometry_valid: bool,
    pub lateral_valid: bool,
    pub movement_code: Option<i64>,
    pub raw8_low5: Option<i64>,
    pub raw13_bit: Option<i64>,
    pub raw_weights148: Option<[i64; 3]>,
    pub raw_weight_state128: Option<i64>,
    pub vel_code: i64,
    pub v_lat_code: i64,
    pub existence_pct: Option<i64>,
    pub predicted: bool,
    pub camera_assoc: Option<i64>,
    pub class_confidence_pct: Option<i64>,
    pub object_class: Option<i64>,
    pub length_m: Option<f64>,
    pub width_m: Option<f64>,
    pub height_code: Option<i64>,
    pub range_unc_code: Option<i64>,
    pub lateral_unc_code: Option<i64>,
    pub vlat_unc_code: Option<i64>,
    pub accel_unc_code: Option<i64>,
    pub lateral_accel_unc_code: Option<i64>,
    pub orientation_unc_code: Option<i64>,
    pub secondary_score_pct: Option<i64>,
}
pub fn slot_bits(slot: &[u8], start: usize, length: usize) -> u64 {
    let mut value = 0;
    for index in 0..length.min(64) {
        let bit = start + index;
        value |= u64::from((slot.get(bit / 8).copied().unwrap_or(0) >> (bit % 8)) & 1) << index;
    }
    value
}
pub fn field_code(slot: &[u8], field: NativeField) -> i64 {
    slot_bits(slot, field.bit_start, field.bit_len) as i64
}
pub fn field_value(slot: &[u8], field: NativeField) -> f64 {
    (field_code(slot, field) as f64 - field.zero_code) * field.scale
}
pub fn encode_slot(codes: &[(&str, i64)]) -> Result<[u8; 36], String> {
    let legacy = codes.iter().find(|(name, _)| *name == "move_state");
    let full = codes.iter().find(|(name, _)| *name == "movement_code");
    if let (Some(legacy), Some(full)) = (legacy, full) {
        if legacy.1 & 3 != full.1 & 3 {
            return Err("move_state must equal the low two bits of movement_code".into());
        }
    }
    let mut data = [0; 36];
    for (name, code) in codes {
        let field = NAMED_FIELDS
            .iter()
            .find(|field| field.name == *name)
            .ok_or_else(|| format!("'{name}'"))?;
        for index in 0..field.bit_len {
            if ((*code as u64) >> index) & 1 != 0 {
                let bit = field.bit_start + index;
                data[bit / 8] |= 1 << (bit % 8);
            }
        }
    }
    Ok(data)
}
pub fn decode_native_slot(slot_index: i64, slot: &[u8]) -> Result<NativeObject, String> {
    if slot.len() != 36 {
        return Err(format!("0x80 slot must be 36 bytes, got {}", slot.len()));
    }
    let age = field_code(slot, AGE);
    let lateral = field_code(slot, LAT_DIST) as f64 - LAT_DIST.zero_code;
    let init = age == 1 && field_code(slot, LENGTH) == 0 && field_code(slot, WIDTH) == 0;
    Ok(NativeObject {
        slot: slot_index,
        age,
        d_rel: field_value(slot, LONG_DIST),
        y_rel: lateral * LAT_DIST.scale,
        v_long_ground: field_value(slot, LONG_VEL_GROUND),
        v_lat_ground: field_value(slot, LAT_VEL),
        accel_like_code: field_code(slot, ACCEL_LIKE) - 511,
        move_state: field_code(slot, MOVE_STATE),
        movement_code: Some(field_code(slot, MOVEMENT_CODE)),
        raw8_low5: Some(field_code(slot, RAW8_LOW5)),
        raw13_bit: Some(field_code(slot, RAW13_BIT)),
        raw_weights148: Some([
            field_code(slot, RAW_WEIGHT_148),
            field_code(slot, RAW_WEIGHT_152),
            field_code(slot, RAW_WEIGHT_156),
        ]),
        raw_weight_state128: Some(field_code(slot, RAW_WEIGHT_STATE128)),
        oncoming_flag: field_code(slot, ONCOMING_FLAG) != 0,
        vel_unc_code: field_code(slot, VEL_UNC_240),
        geometry_valid: age >= 1 && !init,
        lateral_valid: lateral.abs() < 2000.,
        vel_code: field_code(slot, LONG_VEL_GROUND),
        v_lat_code: field_code(slot, LAT_VEL),
        existence_pct: Some(field_code(slot, EXISTENCE)),
        predicted: field_code(slot, PREDICTED) != 0,
        camera_assoc: Some(field_code(slot, CAMERA_ASSOC)),
        class_confidence_pct: Some(field_code(slot, CLASS_CONFIDENCE) * 5),
        object_class: Some(field_code(slot, OBJECT_CLASS)),
        length_m: Some(field_value(slot, LENGTH)),
        width_m: Some((field_code(slot, WIDTH) + 1) as f64 * WIDTH.scale),
        height_code: Some(field_code(slot, HEIGHT_LIKE_272)),
        range_unc_code: Some(field_code(slot, UNC_RANGE_224)),
        lateral_unc_code: Some(field_code(slot, UNC_LATERAL_232)),
        vlat_unc_code: Some(field_code(slot, UNC_VLAT_248)),
        accel_unc_code: Some(field_code(slot, UNC_AX_256)),
        lateral_accel_unc_code: Some(field_code(slot, UNC_AY_264)),
        orientation_unc_code: Some(field_code(slot, UNC_ORIENT_200)),
        secondary_score_pct: Some(field_code(slot, SECONDARY_SCORE_184)),
    })
}
pub fn relative_velocity(object: &NativeObject, v_ego: f64) -> f64 {
    object.v_long_ground - v_ego
}
