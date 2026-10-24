//! Source estimator state and scheduling, independent of the runtime crate.
use super::{flag, real, structure};
use crate::{
    lagd_frames,
    lagd_packet_reference::{object, PacketState},
    lagd_reference,
    seal::Result,
};
use capnp::{dynamic_value, message::ReaderOptions};
use openpilot_cereal::log_capnp::event;
use openpilot_log_format::{JsonValue, JsonView};
use serde_json::{json, Value};
use std::collections::VecDeque;

fn normalized(value: f64) -> Value {
    if value.is_nan() {
        json!("nan")
    } else if value == f64::INFINITY {
        json!("inf")
    } else if value == f64::NEG_INFINITY {
        json!("-inf")
    } else {
        json!(value)
    }
}
fn node(value: Value) -> Result<JsonValue> {
    Ok(JsonValue::parse(&value.to_string())?)
}
fn result_real(value: &Value) -> Result<f64> {
    if let Some(value) = value.as_f64() {
        return Ok(value);
    }
    match value.as_str() {
        Some("nan") => Ok(f64::NAN),
        Some("inf") => Ok(f64::INFINITY),
        Some("-inf") => Ok(f64::NEG_INFINITY),
        _ => Err("expected numerical source result".into()),
    }
}
pub(super) fn json_real(value: &JsonValue) -> Result<f64> {
    if let JsonView::Text(_) = value.view() {
        result_real(&serde_json::from_str(&value.to_json()?)?)
    } else {
        lagd_frames::number(value)
    }
}
#[derive(Clone, Copy, Default)]
struct Point {
    time: f64,
    desired: f64,
    actual: f64,
    okay: bool,
}
pub(super) struct Estimator {
    initial: f64,
    t: f64,
    active: bool,
    pressed: bool,
    saturated: bool,
    curvature: f64,
    speed: f64,
    yaw: f64,
    yaw_std: f64,
    pose_valid: bool,
    rpy: Vec<f64>,
    calibrated: bool,
    recovery: [f64; 4],
    last_estimate: f64,
    points: VecDeque<Point>,
    values: Vec<f64>,
    block_idx: usize,
    idx: i64,
    valid_blocks: i64,
}
impl Estimator {
    pub(super) fn new(initial: f64) -> Self {
        let mut value = Self {
            initial,
            t: 0.,
            active: false,
            pressed: false,
            saturated: false,
            curvature: 0.,
            speed: 0.,
            yaw: 0.,
            yaw_std: 0.,
            pose_valid: false,
            rpy: vec![0.; 3],
            calibrated: false,
            recovery: [0.; 4],
            last_estimate: 0.,
            points: VecDeque::new(),
            values: Vec::new(),
            block_idx: 0,
            idx: 0,
            valid_blocks: 0,
        };
        value.reset(initial, 0);
        value
    }
    pub(super) fn reset(&mut self, initial: f64, blocks: i64) {
        self.points = std::iter::repeat_n(Point::default(), 1200).collect();
        self.values = vec![initial; 50];
        self.block_idx = blocks.rem_euclid(50) as usize;
        self.idx = 0;
        self.valid_blocks = blocks;
    }
    pub(super) fn handle(&mut self, topic: &str, wire: &[u8], time: f64) -> Result<()> {
        let message = capnp::serialize::read_message(&mut &wire[..], ReaderOptions::new())?;
        let root = structure(message.get_root::<event::Reader<'_>>()?.into())?;
        let value = structure(root.get_named(topic)?)?;
        match topic {
            "carControl" => self.active = flag(value.get_named("latActive")?)?,
            "carState" => {
                self.pressed = flag(value.get_named("steeringPressed")?)?;
                self.speed = real(value.get_named("vEgo")?)?;
            }
            "controlsState" => {
                let control = structure(value.get_named("lateralControlState")?)?;
                let kind = control.which()?.ok_or("unknown lateral control union")?;
                self.saturated = flag(structure(control.get(kind)?)?.get_named("saturated")?)?;
                self.curvature = real(value.get_named("desiredCurvature")?)?;
            }
            "liveCalibration" => {
                let dynamic_value::Reader::List(values) = value.get_named("rpyCalib")? else {
                    return Err("expected rpyCalib list".into());
                };
                let rpy = (0..values.len())
                    .map(|i| real(values.get(i)?))
                    .collect::<Result<Vec<_>>>()?;
                if rpy.len() != 3 {
                    return Err("calibration Euler angles must have length three".into());
                }
                self.rpy = rpy;
                let dynamic_value::Reader::Enum(status) = value.get_named("calStatus")? else {
                    return Err("expected calibration enum".into());
                };
                self.calibrated = status.get_value() == 1;
            }
            "livePose" => {
                let measurement = |name| -> Result<Value> {
                    let value = structure(value.get_named(name)?)?;
                    let xyz = ["x", "y", "z"]
                        .iter()
                        .map(|key| Ok(normalized(real(value.get_named(key)?)?)))
                        .collect::<Result<Vec<_>>>()?;
                    let std = ["xStd", "yStd", "zStd"]
                        .iter()
                        .map(|key| Ok(normalized(real(value.get_named(key)?)?)))
                        .collect::<Result<Vec<_>>>()?;
                    Ok(json!({"xyz":xyz,"std":std}))
                };
                let input = node(
                    json!({"kind":"pose", "rpy":self.rpy.iter().copied().map(normalized).collect::<Vec<_>>(), "valid":self.calibrated,
                    "pose": {"orientation":measurement("orientationNED")?, "velocity":measurement("velocityDevice")?,
                    "acceleration":measurement("accelerationDevice")?, "angular_velocity":measurement("angularVelocityDevice")?}}),
                )?;
                let transformed = lagd_reference::numeric(&input)?;
                let transformed_pose = lagd_frames::field(&transformed, "pose")?;
                let JsonView::Array(measurements) = transformed_pose.view() else {
                    return Err("expected transformed pose measurements".into());
                };
                let angular = measurements.get(3).ok_or("missing angular measurement")?;
                let component = |name| -> Result<f64> {
                    let values = lagd_frames::field(angular, name)?;
                    let JsonView::Array(values) = values.view() else {
                        return Err("expected transformed measurement vector".into());
                    };
                    json_real(values.get(2).ok_or("missing yaw component")?)
                };
                self.yaw = component("xyz")?;
                self.yaw_std = component("std")?;
                self.pose_valid = flag(
                    structure(value.get_named("angularVelocityDevice")?)?.get_named("valid")?,
                )? && flag(value.get_named("posenetOK")?)?
                    && flag(value.get_named("inputsOK")?)?;
            }
            _ => (),
        }
        self.t = time;
        Ok(())
    }
    pub(super) fn update_points(&mut self) {
        let desired = self.curvature * self.speed * self.speed;
        let actual = self.yaw * self.speed;
        let sensors = self.pose_valid && self.yaw.abs() < 1. && self.yaw_std < 1.;
        let valid_accel = actual.abs() <= 2. && (desired - actual).abs() <= 0.6;
        if !self.active {
            self.recovery[0] = self.t;
        }
        if self.pressed {
            self.recovery[1] = self.t;
        }
        if self.saturated {
            self.recovery[2] = self.t;
        }
        if !sensors || !valid_accel {
            self.recovery[3] = self.t;
        }
        let recovered = self.recovery.iter().all(|last| self.t - last >= 2.);
        let okay = self.active
            && !self.pressed
            && !self.saturated
            && self.speed > 15.
            && self.yaw.abs() >= 0.
            && recovered
            && self.calibrated
            && sensors
            && valid_accel;
        self.points.pop_front();
        self.points.push_back(Point {
            time: self.t,
            desired,
            actual,
            okay,
        });
    }
    pub(super) fn update_estimate(&mut self) -> Result<()> {
        let mut valid = self.points.iter().filter(|point| point.okay).count() >= 500;
        let minimum = self.points.iter().fold(f64::INFINITY, |v, p| {
            if v.is_nan() || p.actual.is_nan() {
                f64::NAN
            } else {
                v.min(p.actual)
            }
        });
        let maximum = self.points.iter().fold(f64::NEG_INFINITY, |v, p| {
            if v.is_nan() || p.actual.is_nan() {
                f64::NAN
            } else {
                v.max(p.actual)
            }
        });
        valid &= maximum - minimum >= 0.5;
        if self.last_estimate != 0. && self.points[0].time <= self.last_estimate {
            let reverse_index = self
                .points
                .iter()
                .rev()
                .position(|point| point.time <= self.last_estimate)
                .ok_or("StopIteration: no previous estimate point")?;
            valid &= reverse_index != 0
                && self
                    .points
                    .iter()
                    .skip(self.points.len() - reverse_index)
                    .any(|point| point.okay);
        }
        let mask = self
            .points
            .iter()
            .map(|point| f64::from(u8::from(point.okay)))
            .collect::<Vec<_>>();
        let smooth = |desired: bool| -> Result<Vec<f64>> {
            let values = self
                .points
                .iter()
                .zip(&mask)
                .map(|(point, mask)| (if desired { point.desired } else { point.actual }) * mask)
                .collect::<Vec<_>>();
            let mut values = lagd_reference::numeric::smooth(&values, &mask, 5, 1.)?;
            for (value, mask) in values.iter_mut().zip(&mask) {
                if *mask == 0. {
                    *value = 0.;
                }
            }
            Ok(values)
        };
        // Original still computes smoothing/correlation on invalid windows.
        let result =
            lagd_reference::numeric::delay(&smooth(true)?, &smooth(false)?, &mask, 0.05, 0.15, 1.)?;
        let delay = result_real(&result["delay"])?;
        let correlation = result_real(&result["correlation"])?;
        let confidence = result_real(&result["confidence"])?;
        if correlation < 0.95 || confidence < 0.7 || !valid {
            return Ok(());
        }
        self.values[self.block_idx] =
            (self.idx as f64 * self.values[self.block_idx] + delay) / (self.idx + 1) as f64;
        self.idx = (self.idx + 1) % 100;
        if self.idx == 0 {
            self.block_idx = (self.block_idx + 1) % 50;
            self.valid_blocks = (self.valid_blocks + 1).min(50);
        }
        self.last_estimate = self.t;
        Ok(())
    }
    pub(super) fn packet(&self, valid: bool) -> Result<Option<Vec<u8>>> {
        PacketState {
            initial: self.initial,
            values: &self.values,
            block_idx: self.block_idx,
            idx: self.idx,
            valid_blocks: self.valid_blocks,
        }
        .encode(valid, true)
    }
    pub(super) fn snapshot(&self) -> Result<JsonValue> {
        // The original loop snapshot has raw Python floats; only packet/pose
        // projections normalize nonfinite numbers to strings.
        fn raw(value: f64) -> Result<JsonValue> {
            let text = if value.is_nan() {
                "NaN".to_owned()
            } else if value == f64::INFINITY {
                "Infinity".to_owned()
            } else if value == f64::NEG_INFINITY {
                "-Infinity".to_owned()
            } else {
                format!("{value:?}")
            };
            Ok(JsonValue::parse(&text)?)
        }
        let last = self
            .points
            .back()
            .ok_or("source points unexpectedly empty")?;
        object(&[
            (
                "okay",
                node(json!(self.points.iter().filter(|point| point.okay).count()))?,
            ),
            (
                "last",
                object(&[
                    ("time", raw(last.time)?),
                    ("desired", raw(last.desired)?),
                    ("actual", raw(last.actual)?),
                    ("okay", node(json!(last.okay))?),
                ])?,
            ),
            ("last_estimate", raw(self.last_estimate)?),
            ("block_idx", node(json!(self.block_idx))?),
            ("idx", node(json!(self.idx))?),
            ("valid_blocks", node(json!(self.valid_blocks))?),
            (
                "recovery",
                lagd_frames::array(
                    &self
                        .recovery
                        .iter()
                        .copied()
                        .map(raw)
                        .collect::<Result<Vec<_>>>()?,
                )?,
            ),
        ])
    }
}
