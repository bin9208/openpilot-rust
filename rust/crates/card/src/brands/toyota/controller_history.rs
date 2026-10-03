use serde::{Serialize, Serializer};

#[derive(Serialize)]
pub struct History {
    pub frame: u64,
    pub last_torque: i32,
    #[serde(serialize_with = "number")]
    pub last_angle: f64,
    pub alert_active: bool,
    pub last_standstill: bool,
    pub standstill_req: bool,
    pub permit_braking: bool,
    pub steer_rate_counter: u32,
    #[serde(serialize_with = "number")]
    pub distance_button: f64,
    #[serde(serialize_with = "number")]
    pub accel: f64,
    #[serde(serialize_with = "number")]
    pub prev_accel: f64,
    pub secoc_lka_message_counter: u64,
    pub secoc_lta_message_counter: u64,
    #[serde(serialize_with = "number")]
    pub secoc_prev_reset_counter: f64,
}
impl Default for History {
    fn default() -> Self {
        Self {
            frame: 0,
            last_torque: 0,
            last_angle: 0.,
            alert_active: false,
            last_standstill: false,
            standstill_req: false,
            permit_braking: true,
            steer_rate_counter: 0,
            distance_button: 0.,
            accel: 0.,
            prev_accel: 0.,
            secoc_lka_message_counter: 0,
            secoc_lta_message_counter: 0,
            secoc_prev_reset_counter: 0.,
        }
    }
}
#[derive(Serialize)]
pub struct Snapshot<'a> {
    pub(super) history: &'a History,
    #[serde(serialize_with = "number")]
    pub(super) aego: f64,
    #[serde(serialize_with = "number")]
    pub(super) pitch: f64,
    pub(super) steer_max: i32,
    pub(super) steer_delta_up: i32,
    pub(super) steer_delta_down: i32,
    #[serde(serialize_with = "numbers")]
    pub(super) pid: [f64; 6],
}
#[derive(Serialize)]
#[serde(untagged)]
enum Number {
    Finite(f64),
    Special(&'static str),
}
impl From<f64> for Number {
    fn from(value: f64) -> Self {
        if value.is_nan() {
            Self::Special("nan")
        } else if value == f64::INFINITY {
            Self::Special("inf")
        } else if value == f64::NEG_INFINITY {
            Self::Special("-inf")
        } else {
            Self::Finite(value)
        }
    }
}
fn number<S: Serializer>(value: &f64, serializer: S) -> Result<S::Ok, S::Error> {
    Number::from(*value).serialize(serializer)
}
fn numbers<S: Serializer>(values: &[f64; 6], serializer: S) -> Result<S::Ok, S::Error> {
    serializer.collect_seq(values.iter().copied().map(Number::from))
}
