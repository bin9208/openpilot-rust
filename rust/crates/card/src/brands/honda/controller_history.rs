use super::Error;
use openpilot_cereal::car_capnp::car_params;
use serde::{Serialize, Serializer};

fn number<S: Serializer>(value: &f64, serializer: S) -> Result<S::Ok, S::Error> {
    if value.is_nan() {
        serializer.serialize_str("nan")
    } else if *value == f64::INFINITY {
        serializer.serialize_str("inf")
    } else if *value == f64::NEG_INFINITY {
        serializer.serialize_str("-inf")
    } else {
        serializer.serialize_f64(*value)
    }
}
#[derive(Default, Serialize)]
pub struct History {
    pub frame: u64,
    pub braking: bool,
    pub brake_steady: f64,
    pub brake_last: f64,
    pub apply_brake_last: i32,
    pub last_pump_ts: f64,
    pub stopping_counter: u64,
    #[serde(serialize_with = "number")]
    pub accel: f64,
    pub speed: f64,
    #[serde(serialize_with = "number")]
    pub gas: f64,
    pub brake: f64,
    #[serde(serialize_with = "number")]
    pub last_torque: f64,
}
#[derive(Serialize)]
pub struct Limits {
    pub steer_max: f64,
    pub steer_delta_up: i32,
    pub steer_delta_down: i32,
    pub steer_lookup_bp: Vec<f64>,
    pub steer_lookup_v: Vec<f64>,
}
impl Limits {
    pub fn new(cp: car_params::Reader<'_>) -> Result<Self, Error> {
        let lateral = cp.get_lateral_params()?;
        let bp = lateral.get_torque_b_p()?;
        let values = lateral.get_torque_v()?;
        if bp.is_empty() || bp.get(0) != 0 {
            return Err(Error::Numeric);
        }
        let mut points = bp.iter().skip(1).map(|v| -f64::from(v)).collect::<Vec<_>>();
        points.reverse();
        points.extend(bp.iter().map(f64::from));
        let mut outputs = values
            .iter()
            .skip(1)
            .map(|v| -f64::from(v))
            .collect::<Vec<_>>();
        outputs.reverse();
        outputs.extend(values.iter().map(f64::from));
        Ok(Self {
            steer_max: f64::from(bp.get(bp.len() - 1)),
            steer_delta_up: 3,
            steer_delta_down: 3,
            steer_lookup_bp: points,
            steer_lookup_v: outputs,
        })
    }
}
#[derive(Serialize)]
pub struct Snapshot<'a> {
    pub history: &'a History,
    pub limits: &'a Limits,
}
