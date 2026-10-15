use serde::{Deserialize, Serialize};

pub const LANE_TIMEOUT_NS: u64 = 4_000_000_000;
pub const BLINDSPOT_TIMEOUT_NS: u64 = 1_500_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Left,
    Right,
}

impl Side {
    pub const fn index(self) -> usize {
        match self {
            Self::Left => 0,
            Self::Right => 1,
        }
    }
    pub const fn name(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct GateInput {
    pub alive_valid: bool,
    pub left_width: f64,
    pub right_width: f64,
    pub configured: [bool; 2],
    pub next_side: Side,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Gate {
    pub active: bool,
    #[serde(serialize_with = "gate_side")]
    pub side: Option<Side>,
    pub reason: &'static str,
    pub lane_width: f64,
    pub eligible_sides: Vec<Side>,
    #[serde(serialize_with = "gate_widths")]
    pub lane_widths: [f64; 2],
}

fn gate_widths<S: serde::Serializer>(widths: &[f64; 2], serializer: S) -> Result<S::Ok, S::Error> {
    use serde::ser::SerializeMap;
    let mut map = serializer.serialize_map(Some(2))?;
    map.serialize_entry("left", &widths[0])?;
    map.serialize_entry("right", &widths[1])?;
    map.end()
}

fn gate_side<S: serde::Serializer>(side: &Option<Side>, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(side.map_or("", Side::name))
}

pub fn gate(input: GateInput) -> Gate {
    let widths = if input.alive_valid {
        [input.left_width, input.right_width]
    } else {
        [0.0; 2]
    };
    let eligible_sides: Vec<_> = [Side::Left, Side::Right]
        .into_iter()
        .filter(|side| {
            input.alive_valid && input.configured[side.index()] && widths[side.index()] >= 2.0
        })
        .collect();
    let side = if eligible_sides.contains(&input.next_side) {
        Some(input.next_side)
    } else {
        eligible_sides.first().copied()
    };
    Gate {
        active: !eligible_sides.is_empty(),
        side,
        reason: if !input.alive_valid {
            "carState or modelV2 is unavailable"
        } else if eligible_sides.is_empty() {
            "configured lane widths below 2.0 m"
        } else {
            ""
        },
        lane_width: side.map_or(0.0, |side| widths[side.index()]),
        eligible_sides,
        lane_widths: widths,
    }
}

pub fn fresh(received: u64, now: u64, timeout: u64) -> bool {
    received > 0 && now.checked_sub(received).is_some_and(|age| age <= timeout)
}

#[derive(Debug, Default, Clone, Copy, Serialize)]
pub struct Detection {
    pub score: f64,
    pub active: bool,
    pub confidence: f64,
}

#[derive(Debug, Default, Serialize)]
pub struct Blindspot {
    sides: [Detection; 2],
}

impl Blindspot {
    pub fn side(&self, side: Side) -> &Detection {
        &self.sides[side.index()]
    }

    pub fn update(&mut self, side: Side, confidence: f64, threshold: f64, smoothing: f64, dt: f64) {
        let result = &mut self.sides[side.index()];
        let alpha = (dt / smoothing.max(0.001)).min(1.0);
        result.score = (1.0 - alpha) * result.score + alpha * confidence;
        result.confidence = confidence;
        if !result.active && result.score >= threshold {
            result.active = true;
        } else if result.active && result.score < (threshold - 0.15).max(0.0) {
            result.active = false;
        }
    }
}
