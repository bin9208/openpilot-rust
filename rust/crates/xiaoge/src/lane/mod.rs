mod head;
pub use head::Head;

use crate::Error;
use serde::Serialize;

pub const INPUT_SIZE: usize = 416;
pub const PROTO_SIZE: usize = 104;
pub const MASK_CHANNELS: usize = 32;

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Candidate {
    pub class_id: u8,
    #[serde(serialize_with = "score_as_double")]
    pub score: f32,
    pub bottom: u32,
    pub center: f64,
}

fn score_as_double<S: serde::Serializer>(value: &f32, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_f64(f64::from(*value))
}

impl Candidate {
    fn distance(self) -> f64 {
        (self.center - 52.0).powi(2) + 2.0 * (f64::from(self.bottom) - 103.0).powi(2)
    }

    pub const fn line_type(self) -> i8 {
        match self.class_id {
            0 | 2 | 5 => 1,
            1 => 0,
            _ => -1,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultData {
    pub left_line: i8,
    pub right_line: i8,
    pub left_conf: f64,
    pub right_conf: f64,
    pub valid: bool,
    pub error: String,
    pub candidates_count: u64,
}

impl ResultData {
    pub fn failed(error: String) -> Self {
        Self {
            left_line: -1,
            right_line: -1,
            left_conf: 0.0,
            right_conf: 0.0,
            valid: false,
            error,
            candidates_count: 0,
        }
    }
}

pub fn select(candidates: &[Candidate]) -> Result<ResultData, Error> {
    let mut sides: [Option<Candidate>; 2] = [None, None];
    for &candidate in candidates {
        let side = usize::from(candidate.center >= 52.0);
        if sides[side].is_none_or(|best| candidate.distance() < best.distance()) {
            sides[side] = Some(candidate);
        }
    }
    let confidence = |candidate: Option<Candidate>| {
        let value = candidate.map_or(0.0, |candidate| f64::from(candidate.score));
        format!("{value:.3}")
            .parse::<f64>()
            .map_err(|_| Error::Invalid("invalid lane confidence"))
    };
    Ok(ResultData {
        left_line: sides[0].map_or(-1, Candidate::line_type),
        right_line: sides[1].map_or(-1, Candidate::line_type),
        left_conf: confidence(sides[0])?,
        right_conf: confidence(sides[1])?,
        valid: true,
        error: String::new(),
        candidates_count: u64::try_from(candidates.len())
            .map_err(|_| Error::Invalid("lane candidate count overflow"))?,
    })
}
