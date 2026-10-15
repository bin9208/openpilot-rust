//! Pinned ARS510 support-frame decoders; keep radar and vehicle targets distinct.
#[derive(serde::Serialize)]
pub struct Target192 {
    pub range_code13: u16,
    pub field1_code13: u16,
}
impl Target192 {
    pub fn d_rel(&self) -> f64 {
        f64::from(i32::from(self.range_code13) - 160) / 16.
    }
    pub fn y_rel(&self) -> f64 {
        f64::from(i32::from(self.field1_code13) - 2048) * 0.015
    }
}
pub fn parse_0x192(data: &[u8]) -> Option<Target192> {
    if data.len() < 4 || data[..4] == [0, 255, 0, 255] {
        return None;
    }
    Some(Target192 {
        range_code13: u16::from_be_bytes([data[0], data[1]]) & 0x1fff,
        field1_code13: u16::from_be_bytes([data[2], data[3]]) & 0x1fff,
    })
}
#[derive(serde::Serialize)]
pub struct CycleHeader {
    pub active_summaries: u8,
    pub timestamp_us: u32,
    pub counter: u8,
}
pub fn parse_0x190(data: &[u8]) -> Option<CycleHeader> {
    if data.len() < 7 || data[0] == 255 {
        return None;
    }
    Some(CycleHeader {
        active_summaries: data[0] >> 2,
        timestamp_us: u32::from_be_bytes(data[2..6].try_into().expect("checked frame length")),
        counter: data[6] >> 4,
    })
}
#[derive(serde::Serialize)]
pub struct Target191 {
    pub score: u8,
    pub age: u8,
    pub track_code: u8,
    pub width_m: f64,
    pub height_m: f64,
    pub length_m: f64,
    pub quality: u8,
}
pub fn parse_0x191(data: &[u8]) -> Option<Target191> {
    if data.len() != 8 || data == [254, 254, 254, 252, 252, 255, 254, 255] || data[0] >> 1 == 127 {
        return None;
    }
    Some(Target191 {
        score: data[0] >> 1,
        age: data[1] >> 1,
        track_code: data[3] >> 2,
        width_m: f64::from(data[4] >> 2) * 0.1,
        height_m: f64::from(data[6] >> 1) * 0.1,
        length_m: f64::from(data[7]) * 0.1,
        quality: data[5] >> 3,
    })
}
pub fn be_field(data: &[u8], start: u32, length: u32) -> u64 {
    let mut bytes = [0; 8];
    let count = data.len().min(8);
    bytes[..count].copy_from_slice(&data[..count]);
    (u64::from_be_bytes(bytes) >> start)
        & if length >= 64 {
            u64::MAX
        } else {
            (1 << length) - 1
        }
}
#[derive(serde::Serialize)]
pub struct Object680 {
    pub selector: u8,
    pub d_rel: f64,
    pub y_rel: f64,
    pub v_ground: f64,
    pub flags: u8,
    pub lat_speed_code: i64,
}
impl Object680 {
    pub fn stationary(&self) -> bool {
        self.flags & 1 != 0
    }
    pub fn oncoming(&self) -> bool {
        self.flags & 2 != 0
    }
    pub fn seen_moving(&self) -> bool {
        self.flags & 8 != 0
    }
}
pub fn parse_0x680(data: &[u8]) -> Option<Object680> {
    if data.len() != 8 {
        return None;
    }
    let range = be_field(data, 43, 13);
    let lateral = be_field(data, 32, 11) as i64;
    if range <= 1 && lateral == 0 {
        return None;
    }
    Some(Object680 {
        selector: data[0],
        d_rel: range as f64 / 32.,
        y_rel: (if lateral >= 1024 {
            lateral - 2048
        } else {
            lateral
        }) as f64
            * 0.015,
        v_ground: (be_field(data, 22, 10) as i64 - 512) as f64 * 0.15,
        flags: be_field(data, 16, 6) as u8,
        lat_speed_code: be_field(data, 8, 8) as i64 - 128,
    })
}
pub fn parse_acc_target_vrel(data: &[u8]) -> Option<f64> {
    if data.len() < 8 {
        None
    } else {
        Some((be_field(data, 29, 11) as i64 - 1024) as f64 * 0.125)
    }
}
pub fn parse_acc_target_arel(data: &[u8]) -> Option<f64> {
    if data.len() < 8 {
        None
    } else {
        Some(f64::from(i16::from(data[2]) - 100) * 0.125)
    }
}
pub fn parse_acc_target_position(data: &[u8]) -> Option<(f64, f64)> {
    if data.len() < 8 {
        None
    } else {
        Some((
            be_field(data, 39, 13) as f64 * 0.025,
            (be_field(data, 27, 12) as i64 - 2000) as f64 * 0.01,
        ))
    }
}
pub fn parse_acc_target_range_code(data: &[u8]) -> Option<u64> {
    if data.len() < 8 {
        None
    } else {
        Some(be_field(data, 39, 13))
    }
}
#[derive(serde::Serialize)]
pub struct AccTarget {
    pub target_id: u64,
    pub d_rel: f64,
    pub y_rel: f64,
    pub v_rel: f64,
    pub a_rel: f64,
    pub v_lat: f64,
    pub a_lat: f64,
    pub in_path: bool,
    pub in_path_level: u64,
    pub tracker_codes: (u64, u64),
}
pub fn parse_acc_target(data235: &[u8], data237: &[u8]) -> Option<AccTarget> {
    if data235.len() != 8
        || data237.len() != 8
        || data235[1] & 15 != 5
        || be_field(data237, 10, 1) == 0
    {
        return None;
    }
    let (d_rel, y_rel) = parse_acc_target_position(data237).expect("checked frame length");
    Some(AccTarget {
        target_id: be_field(data235, 0, 5),
        d_rel,
        y_rel,
        v_rel: parse_acc_target_vrel(data235).expect("checked frame length"),
        a_rel: parse_acc_target_arel(data235).expect("checked frame length"),
        v_lat: (be_field(data235, 8, 11) as i64 - 1024) as f64 * 0.0125,
        a_lat: (be_field(data235, 19, 8) as i64 - 100) as f64 * 0.125,
        in_path: be_field(data237, 8, 1) != 0,
        in_path_level: be_field(data237, 4, 3),
        tracker_codes: (be_field(data237, 19, 8), be_field(data237, 11, 8)),
    })
}
#[derive(serde::Serialize)]
pub struct AccTargetInfo {
    pub target_class: u8,
    pub present: bool,
    pub in_object_list: bool,
    pub timestamp_us: u64,
}
pub fn parse_0x239(data: &[u8]) -> Option<AccTargetInfo> {
    if data.len() != 8 || data[1] & 15 == 15 {
        return None;
    }
    Some(AccTargetInfo {
        target_class: data[1] & 15,
        present: data[3] & 8 != 0,
        in_object_list: data[3] & 128 != 0,
        timestamp_us: be_field(data, 3, 32),
    })
}
pub fn crc8_0x1d_bits(bits: impl IntoIterator<Item = bool>) -> u8 {
    let mut crc = 0_u8;
    for bit in bits {
        crc = crc.wrapping_shl(1) ^ (((crc >> 7) ^ u8::from(bit)) * 0x1d);
    }
    crc
}
pub fn crc_0x23b(value: u8, counter_and_low: u8) -> u8 {
    let bits = (0..4)
        .map(|i| (counter_and_low >> (7 - i)) & 1 != 0)
        .chain([false; 4])
        .chain((0..4).map(|i| (counter_and_low >> (3 - i)) & 1 != 0))
        .chain((0..8).map(|i| (value >> (7 - i)) & 1 != 0));
    crc8_0x1d_bits(bits) ^ 0x59
}
pub fn parse_0x23b(data: &[u8]) -> Option<(u16, u8, bool)> {
    if data.len() != 3 {
        return None;
    }
    Some((
        (u16::from(data[1] & 15) << 8) | u16::from(data[2]),
        data[1] >> 4,
        crc_0x23b(data[2], data[1]) == data[0],
    ))
}
#[derive(serde::Serialize)]
pub struct Target366 {
    pub speed_code: u16,
    pub distance_code: u8,
    pub header_raw: u16,
    pub tail_raw: u32,
    pub lateral_code: i8,
}
impl Target366 {
    pub fn y_rel(&self) -> Option<f64> {
        if [15, -16].contains(&self.lateral_code) {
            None
        } else {
            Some(-0.34 * f64::from(self.lateral_code))
        }
    }
    pub fn v_rel(&self) -> Option<f64> {
        if self.speed_code < 256 {
            Some(f64::from(i32::from(self.speed_code) - 155) * 5. / 36.)
        } else {
            None
        }
    }
    pub fn d_rel(&self) -> f64 {
        f64::from(self.distance_code) * 0.8
    }
}
pub fn parse_0x366(data: &[u8]) -> Option<Target366> {
    if data.len() != 7 || data[2..4] == [127, 255] {
        return None;
    }
    let lateral = (data[5] >> 3) as i8;
    Some(Target366 {
        speed_code: u16::from(data[2]) * 2 + u16::from(data[3] >> 7),
        distance_code: data[3] & 127,
        header_raw: u16::from_be_bytes([data[0], data[1]]),
        tail_raw: u32::from_be_bytes([0, data[4], data[5], data[6]]),
        lateral_code: if lateral >= 16 { lateral - 32 } else { lateral },
    })
}

pub fn parse_toyota_yaw_rate(data: &[u8]) -> Option<f64> {
    if data.len() < 2 {
        return None;
    }
    let raw = (u16::from(data[0] & 3) << 8) | u16::from(data[1]);
    Some((f64::from(raw) * 0.244 - 125.) * std::f64::consts::PI / 180.)
}
pub fn parse_toyota_speed_mps(data: &[u8]) -> Option<f64> {
    if data.len() < 7 {
        return None;
    }
    Some(f64::from(u16::from_be_bytes([data[5], data[6]])) * 0.01 / 3.6)
}
