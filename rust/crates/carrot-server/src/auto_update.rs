//! Pure conditions from services/auto_update.py; callers supply observed time and state.
use crate::{param_changes::text, state::trim, Error, Value};
use num_bigint::BigInt;

pub const READY_DELAY: f64 = 10.;
pub const DISENGAGED_DELAY: f64 = 1.;
pub const COOLDOWN: f64 = 300.;
const SAMPLE_GAP: f64 = 2.5;
const ERROR_DETAIL_LIMIT: usize = 2000;

pub struct ManagerReady {
    delay: f64,
    last_sample: Option<f64>,
    ready_since: Option<f64>,
}

impl ManagerReady {
    pub const fn new(delay: f64) -> Self {
        Self {
            delay,
            last_sample: None,
            ready_since: None,
        }
    }

    pub fn update(&mut self, now: f64, valid: bool) -> bool {
        if self.last_sample.is_some_and(|last| now - last > SAMPLE_GAP) {
            self.ready_since = None;
        }
        self.last_sample = Some(now);
        if !valid {
            self.ready_since = None;
            return false;
        }
        let since = self.ready_since.get_or_insert(now);
        now - *since >= self.delay
    }
}

#[derive(Clone, Copy)]
enum RebootMode {
    Off,
    Park,
    Disengaged,
}

pub struct RebootSample<'a> {
    pub now: f64,
    pub selfdrive_valid: bool,
    pub engaged: bool,
    pub car_state_valid: bool,
    pub gear_shifter: &'a Value,
    pub device_state_valid: bool,
    pub device_started: bool,
}

pub struct AutoRebootCondition {
    mode: RebootMode,
    delay: f64,
    ready_since: Option<f64>,
}

impl AutoRebootCondition {
    pub fn new(mode: &str, delay: f64) -> Self {
        let mode = match mode {
            "park" => RebootMode::Park,
            "disengaged" => RebootMode::Disengaged,
            _ => RebootMode::Off,
        };
        Self {
            mode,
            delay: delay.max(0.),
            ready_since: None,
        }
    }

    pub fn update(&mut self, sample: &RebootSample<'_>) -> bool {
        match self.mode {
            RebootMode::Off => false,
            RebootMode::Park => {
                sample.selfdrive_valid
                    && !sample.engaged
                    && sample.car_state_valid
                    && is_park(sample.gear_shifter)
            }
            RebootMode::Disengaged => {
                let disengaged = sample.selfdrive_valid && !sample.engaged;
                let offroad = sample.device_state_valid && !sample.device_started;
                if !(disengaged || offroad) {
                    self.ready_since = None;
                    return false;
                }
                let since = self.ready_since.get_or_insert(sample.now);
                sample.now - *since >= self.delay
            }
        }
    }
}

fn is_park(gear: &Value) -> bool {
    let Ok(Value::Text(points)) = gear.py_string() else {
        return false;
    };
    let Some(component) = trim(&points)
        .rsplit(|point| *point == u32::from('.'))
        .next()
    else {
        return false;
    };
    let Some(text) = component
        .iter()
        .copied()
        .map(char::from_u32)
        .collect::<Option<String>>()
    else {
        return false;
    };
    text.to_lowercase() == "park"
}

pub fn verified_update_target(status: &Value) -> Result<(BigInt, Value), Error> {
    let empty = || (BigInt::from(0), Value::text(""));
    if !status.get("available").truth() || !status.get("state").text_eq("ok") {
        return Ok(empty());
    }
    let value = status.get("behind");
    let behind = if value.truth() {
        value.int()?
    } else {
        BigInt::from(0)
    }
    .max(BigInt::from(0));
    let head = text::stripped(status.get("target_head"), true)?;
    Ok(if behind > BigInt::from(0) && head.truth() {
        (behind, head)
    } else {
        empty()
    })
}

pub fn short_error(output: &Value, fallback: &[u32]) -> Result<Value, Error> {
    let Value::Text(points) = text::string(output, true)? else {
        return Err(Error::Source("expected Python string".into()));
    };
    let mut detail = Vec::with_capacity(points.len());
    for word in points.split(|point| {
        char::from_u32(*point)
            .is_some_and(|c| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c))
    }) {
        if word.is_empty() {
            continue;
        }
        if !detail.is_empty() {
            detail.push(u32::from(' '));
        }
        detail.extend_from_slice(word);
    }
    let detail = if detail.is_empty() { fallback } else { &detail };
    Ok(Value::Text(
        detail[detail.len().saturating_sub(ERROR_DETAIL_LIMIT)..].to_vec(),
    ))
}
