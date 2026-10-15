mod encoding;
mod payload;
use openpilot_cereal::car_capnp::car_state;
pub use payload::parse;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Utf8(#[from] std::str::Utf8Error),
    #[error(transparent)]
    Utf16(#[from] std::string::FromUtf16Error),
    #[error("invalid JSON character encoding")]
    Encoding,
    #[error("JSON integer exceeds the source 4300-digit limit")]
    IntegerDigits,
    #[error("invalid xiaogeVision field {0}")]
    Field(&'static str),
}

#[derive(Debug)]
pub struct VisionResult {
    pub left_lane: i16,
    pub right_lane: i16,
    pub lane_valid: bool,
    pub lane_received: Option<u64>,
    pub left_blindspot: bool,
    pub right_blindspot: bool,
    pub blindspot_valid: bool,
    pub blindspot_received: Option<u64>,
    pub blindspot_received_by_side: [Option<u64>; 2],
}

fn fresh(received: Option<u64>, now: u64, timeout: u64) -> bool {
    received.is_some_and(|received| {
        received != 0 && now.checked_sub(received).is_some_and(|age| age <= timeout)
    })
}

pub fn merge_lane(current: i16, detected: i16) -> i16 {
    if detected < 0 {
        return current;
    }
    let color = if current >= 10 { current / 10 * 10 } else { 0 };
    color + detected
}

pub fn apply(mut state: car_state::Builder<'_>, result: Option<&VisionResult>, now: u64) -> bool {
    let Some(result) = result else {
        return false;
    };
    let mut applied = false;
    if result.lane_valid && fresh(result.lane_received, now, 4_000_000_000) {
        if result.left_lane >= 0 {
            let previous = state.reborrow_as_reader().get_left_lane_line();
            state.set_left_lane_line(merge_lane(previous, result.left_lane));
            applied = true;
        }
        if result.right_lane >= 0 {
            let previous = state.reborrow_as_reader().get_right_lane_line();
            state.set_right_lane_line(merge_lane(previous, result.right_lane));
            applied = true;
        }
    }
    let [left_vision, right_vision] = blindspot_vision(Some(result), now);
    if left_vision || right_vision {
        let previous = state.reborrow_as_reader();
        let left = previous.get_left_blindspot() || left_vision;
        let right = previous.get_right_blindspot() || right_vision;
        state.set_left_blindspot(left);
        state.set_right_blindspot(right);
        applied = true;
    }
    applied
}

/// Final source aggregate gate followed by independent per-side freshness.
pub fn blindspot_vision(result: Option<&VisionResult>, now: u64) -> [bool; 2] {
    let Some(result) = result else {
        return [false; 2];
    };
    if !result.blindspot_valid || !fresh(result.blindspot_received, now, 1_500_000_000) {
        return [false; 2];
    }
    let side = |index: usize| {
        let received = result.blindspot_received_by_side[index];
        fresh(
            if received == Some(0) {
                result.blindspot_received
            } else {
                received
            },
            now,
            1_500_000_000,
        )
    };
    [
        result.left_blindspot && side(0),
        result.right_blindspot && side(1),
    ]
}
