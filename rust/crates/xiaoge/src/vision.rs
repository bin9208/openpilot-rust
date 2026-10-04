use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

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

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    None,
    Left,
    Right,
}

#[derive(Debug, Deserialize)]
pub struct GateInput {
    pub alive_valid: bool,
    pub speed: f64,
    pub direction: Direction,
    pub left_width: f64,
    pub right_width: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Gate {
    pub active: bool,
    #[serde(serialize_with = "gate_side")]
    pub side: Option<Side>,
    pub reason: &'static str,
    pub lane_width: f64,
}

fn gate_side<S: serde::Serializer>(side: &Option<Side>, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(side.map_or("", Side::name))
}

pub fn gate(input: GateInput) -> Gate {
    let stopped = |reason| Gate {
        active: false,
        side: None,
        reason,
        lane_width: 0.0,
    };
    if !input.alive_valid {
        return stopped("carState or modelV2 is unavailable");
    }
    if input.speed.partial_cmp(&(30.0 / 3.6)) == Some(Ordering::Less)
        || input.speed.partial_cmp(&(120.0 / 3.6)) == Some(Ordering::Greater)
    {
        return stopped("speed outside 30-120 km/h");
    }
    let (side, lane_width) = match input.direction {
        Direction::None => return stopped("no lane-change direction"),
        Direction::Left => (Side::Left, input.left_width),
        Direction::Right => (Side::Right, input.right_width),
    };
    let active = lane_width >= 3.0;
    Gate {
        active,
        side: Some(side),
        lane_width,
        reason: if active {
            ""
        } else {
            "target lane width below 3.0 m"
        },
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
        let delta = (dt / smoothing.max(0.001)).min(1.0);
        result.score = if confidence >= threshold {
            (result.score + delta).min(1.0)
        } else {
            (result.score - delta).max(0.0)
        };
        result.confidence = confidence;
        if result.score >= 0.65 {
            result.active = true;
        } else if result.score <= 0.25 {
            result.active = false;
        }
    }
}
