use openpilot_cereal::log_capnp::LongitudinalPersonality;
use openpilot_selfdrived::callbacks::{AlertParams, Context, Error, Snapshot};
use openpilot_selfdrived::events::Callback;
use openpilot_ui_framework::multilang::Multilang;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;

#[derive(Deserialize)]
struct Request {
    snapshot: Snapshot,
    callback: Callback,
    language: String,
    params: BTreeMap<String, Value>,
    metric: bool,
    soft_disable_time: u32,
    personality: u16,
    branch: String,
    replay: bool,
    mici: bool,
    nonfinite: Option<NonFinite>,
}

#[derive(Deserialize)]
struct NonFinite {
    field: Field,
    value: Float,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Float {
    Nan,
    Inf,
    NegInf,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Field {
    MinEnableSpeed,
    MinSteerSpeed,
    EgoSpeed,
    FreeSpacePercent,
    AngleOffset,
    SteerRatio,
    StiffnessFactor,
    FrameDropPercent,
    Accel,
    Torque,
    MemoryTemp,
    CpuFirst,
    CpuLast,
    GpuFirst,
    CalibrationPitch,
    CalibrationYaw,
    ModelVelocity,
}

impl NonFinite {
    fn apply(&self, snapshot: &mut Snapshot) {
        let value = match self.value {
            Float::Nan => f64::NAN,
            Float::Inf => f64::INFINITY,
            Float::NegInf => f64::NEG_INFINITY,
        };
        match self.field {
            Field::MinEnableSpeed => snapshot.min_enable_speed = value,
            Field::MinSteerSpeed => snapshot.min_steer_speed = value,
            Field::EgoSpeed => snapshot.ego_speed = value,
            Field::FreeSpacePercent => snapshot.free_space_percent = value,
            Field::AngleOffset => snapshot.angle_offset = value,
            Field::SteerRatio => snapshot.steer_ratio = value,
            Field::StiffnessFactor => snapshot.stiffness_factor = value,
            Field::FrameDropPercent => snapshot.frame_drop_percent = value,
            Field::Accel => snapshot.accel = value,
            Field::Torque => snapshot.torque = value,
            Field::MemoryTemp => snapshot.memory_temp = value,
            Field::CpuFirst => snapshot.cpu_temps = vec![value, 50.0],
            Field::CpuLast => snapshot.cpu_temps = vec![50.0, value],
            Field::GpuFirst => snapshot.gpu_temps = vec![value, 50.0],
            Field::CalibrationPitch => snapshot.calibration_rpy = vec![0.0, value, 0.0],
            Field::CalibrationYaw => snapshot.calibration_rpy = vec![0.0, 0.0, value],
            Field::ModelVelocity => snapshot.model_velocity = vec![value],
        }
    }
}

struct Params {
    values: BTreeMap<String, Value>,
    reads: Vec<Value>,
}
impl AlertParams for Params {
    fn text(&mut self, key: &str) -> Result<Option<String>, Error> {
        self.reads.push(json!(["get", key]));
        match self.values.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(value)) => Ok(Some(value.clone())),
            _ => Err(Error::Params(format!("invalid text {key}"))),
        }
    }
    fn integer(&mut self, key: &str) -> Result<i32, Error> {
        self.reads.push(json!(["get_int", key]));
        self.values
            .get(key)
            .and_then(Value::as_i64)
            .and_then(|value| i32::try_from(value).ok())
            .ok_or_else(|| Error::Params(format!("invalid integer {key}")))
    }
    fn boolean(&mut self, key: &str) -> Result<bool, Error> {
        self.reads.push(json!(["get_bool", key]));
        self.values
            .get(key)
            .and_then(Value::as_bool)
            .ok_or_else(|| Error::Params(format!("invalid boolean {key}")))
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("translation root required")?,
    );
    let mut languages = BTreeMap::new();
    let mut output = io::BufWriter::new(io::stdout().lock());
    for line in io::stdin().lock().lines() {
        let mut request: Request = serde_json::from_str(&line?)?;
        if let Some(nonfinite) = &request.nonfinite {
            nonfinite.apply(&mut request.snapshot);
        }
        if !languages.contains_key(&request.language) {
            languages.insert(
                request.language.clone(),
                Multilang::new(&root, Some(&request.language))?,
            );
        }
        let language = languages
            .get(&request.language)
            .ok_or("language not loaded")?;
        let mut params = Params {
            values: request.params,
            reads: Vec::new(),
        };
        let mut context = Context {
            snapshot: &request.snapshot,
            language,
            params: &mut params,
            metric: request.metric,
            soft_disable_time: request.soft_disable_time,
            personality: LongitudinalPersonality::try_from(request.personality)?,
            branch: &request.branch,
            replay: request.replay,
            mici: request.mici,
        };
        let result = match context.resolve(&request.callback) {
            Ok(alert) => json!({"alert":alert,"reads":params.reads}),
            Err(error) => json!({"error":error.to_string(),"reads":params.reads}),
        };
        serde_json::to_writer(&mut output, &result)?;
        writeln!(output)?;
    }
    Ok(())
}
