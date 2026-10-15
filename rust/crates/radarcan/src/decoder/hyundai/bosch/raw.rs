//! Two independent raw records per Bosch track CAN frame; 0x601 is excluded.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct RawDetection {
    pub timestamp_ns: i64,
    pub slot: usize,
    pub d_rel: f64,
    pub y_rel: f64,
    pub v_rel: f64,
    pub raw_word: u32,
}
pub fn decode_frame(
    timestamp_ns: i64,
    address: u32,
    payload: &[u8],
) -> Result<(Vec<RawDetection>, bool), &'static str> {
    if !(0x602..0x612).contains(&address) || payload.len() != 8 {
        return Err("expected an eight-byte Bosch raw track frame");
    }
    let pair = u64::from_le_bytes(payload.try_into().expect("checked payload length"));
    let first = usize::try_from((address - 0x602) * 2).expect("bounded raw slot");
    let mut objects = Vec::with_capacity(2);
    let mut unsupported = false;
    for half in 0..2 {
        let word = ((pair >> (half * 32)) & 0xffff_ffff) as u32;
        if word == 0x4010_0000 {
            continue;
        }
        if word & (1 << 31) != 0 {
            unsupported = true;
            continue;
        }
        if timestamp_ns < 0 {
            return Err("timestamp_ns must be an integer in [0, None]");
        }
        objects.push(RawDetection {
            timestamp_ns,
            slot: first + half,
            d_rel: f64::from(word & 0x3ff) * 0.25,
            y_rel: f64::from((word >> 10) & 0x7ff) * 0.03125 - 32.,
            v_rel: f64::from((word >> 21) & 0x3ff) * 0.25 - 128.,
            raw_word: word,
        });
    }
    Ok((objects, unsupported))
}

impl RawDetection {
    pub fn new(
        timestamp_ns: i64,
        slot: usize,
        d_rel: f64,
        y_rel: f64,
        v_rel: f64,
        raw_word: u32,
    ) -> Result<Self, &'static str> {
        if timestamp_ns < 0 {
            return Err("timestamp_ns must be an integer in [0, None]");
        }
        if slot > 31 {
            return Err("slot must be an integer in [0, 31]");
        }
        for (value, error) in [
            (d_rel, "d_rel must be finite"),
            (y_rel, "y_rel must be finite"),
            (v_rel, "v_rel must be finite"),
        ] {
            if !value.is_finite() {
                return Err(error);
            }
        }
        if !(0.0..=255.75).contains(&d_rel) {
            return Err("d_rel outside the supported Bosch decoded range");
        }
        if !(-32.0..=31.96875).contains(&y_rel) {
            return Err("y_rel outside the supported Bosch decoded range");
        }
        if !(-128.0..=127.75).contains(&v_rel) {
            return Err("v_rel outside the supported Bosch decoded range");
        }
        if raw_word == 0x4010_0000 || raw_word & (1 << 31) != 0 {
            return Err("empty or unsupported bit31 record is not a RawDetection");
        }
        Ok(Self {
            timestamp_ns,
            slot,
            d_rel,
            y_rel,
            v_rel,
            raw_word,
        })
    }
    pub fn address(&self) -> usize {
        0x602 + self.slot / 2
    }
    pub fn half(&self) -> usize {
        self.slot % 2
    }
}
