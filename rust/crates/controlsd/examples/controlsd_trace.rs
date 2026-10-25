use openpilot_control_policy::numerics::Numerics;
use openpilot_controlsd::{
    config::Config, controller::Controls, interface::ControlInterface, parameters::Parameters,
    Error,
};
use openpilot_messaging::state::{Options, Poll, State};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    io::{BufWriter, Write},
    path::Path,
};

#[derive(Deserialize)]
struct Frame {
    time: f64,
    messages: Vec<Vec<u8>>,
    #[serde(default)]
    params: BTreeMap<String, Vec<u8>>,
}
#[derive(Deserialize)]
struct Case {
    name: String,
    simulation: bool,
    params: BTreeMap<String, Vec<u8>>,
    frames: Vec<Frame>,
}
#[derive(Deserialize)]
struct Request {
    cases: Vec<Case>,
}
struct Store {
    values: BTreeMap<String, Vec<u8>>,
    operations: Vec<Value>,
}
impl Store {
    fn read(&mut self, operation: &str, key: &str) -> &[u8] {
        self.operations.push(json!([operation, key]));
        self.values.get(key).map_or(&[], Vec::as_slice)
    }
}
impl Parameters for Store {
    fn integer(&mut self, key: &'static str) -> Result<i32, Error> {
        Ok(openpilot_beepd::integer(self.read("get_int", key))?)
    }
    fn float(&mut self, key: &'static str) -> Result<f64, Error> {
        Ok(openpilot_calibrationd::parameters::parse_float(
            self.read("get_float", key),
        )?)
    }
    fn boolean(&mut self, key: &'static str) -> Result<bool, Error> {
        Ok(self.read("get_bool", key) == b"1")
    }
    fn string(&mut self, key: &'static str) -> Result<Option<String>, Error> {
        let bytes = self.read("get", key);
        if bytes.is_empty() {
            Ok(None)
        } else {
            Ok(Some(
                std::str::from_utf8(bytes)
                    .map_err(|_| Error::Contract("non-UTF8 Params"))?
                    .into(),
            ))
        }
    }
    fn put_integer(&mut self, key: &'static str, value: i32) -> Result<(), Error> {
        self.operations.push(json!(["put_int", key, value]));
        self.values
            .insert(key.into(), value.to_string().into_bytes());
        Ok(())
    }
    fn put_boolean(&mut self, key: &'static str, value: bool) -> Result<(), Error> {
        self.operations.push(json!(["put_bool", key, value]));
        self.values.insert(
            key.into(),
            if value { b"1".to_vec() } else { b"0".to_vec() },
        );
        Ok(())
    }
}
fn snapshot(controls: &Controls) -> Value {
    let lo = &controls.longitudinal;
    let la = &controls.lateral;
    let mut state = json!({"curvature":controls.curvature,"desired":controls.desired,"safety_limited":controls.safety_limited,
        "long_state": match lo.state { openpilot_controlsd::longitudinal::State::Off => "0", openpilot_controlsd::longitudinal::State::Pid => "1", openpilot_controlsd::longitudinal::State::Stopping => "2", openpilot_controlsd::longitudinal::State::Starting => "3" },
        "long_pid":[lo.pid.p,lo.pid.i,lo.pid.f,lo.pid.control],"last_accel":lo.last,"coasting":lo.correction,"saturation":la.saturation,
        "suspended":controls.suspend.active,"suspend_times":[controls.suspend.enter,controls.suspend.hold]});
    if let Some(pid) = &la.pid {
        state["lat_pid"] = json!([pid.p, pid.i, pid.d, pid.f, pid.control]);
    }
    if let Some(pid) = &controls.meb {
        state["meb_pid"] = json!([
            pid.p,
            pid.i,
            pid.f,
            pid.control,
            pid.multiplicative.as_ref().map(|m| m.factor)
        ]);
    }
    state
}
#[cfg(feature = "diagnostic-553")]
fn scalar_553(value: f64) -> Value {
    json!({"dtype":"f64", "bits":format!("{:016x}",value.to_bits()), "runtime_type":"rust.f64"})
}

#[cfg(feature = "diagnostic-553")]
fn record_scalar_553(
    case: &str,
    controls: &Controls,
    error: f64,
    speed: f64,
) -> Result<(), Box<dyn std::error::Error>> {
    let pid = controls
        .lateral
        .pid
        .as_ref()
        .ok_or("#553 missing lateral PID")?;
    if pid.gains.p.x.is_empty()
        || pid.gains.p.x.len() > 256
        || pid.gains.p.y.is_empty()
        || pid.gains.p.y.len() > 256
    {
        return Err("#553 stored gain bound".into());
    }
    // 계산이 끝난 필드만 읽는다. Gain::at/PID/NN을 다시 호출하지 않는다.
    let receipt = json!({"schema":1,"observer":"native","case":case,"frame":0,
        "fields":{"error":scalar_553(error),"p":scalar_553(pid.p),"speed":scalar_553(speed)},
        "gain_x":pid.gains.p.x.iter().copied().map(scalar_553).collect::<Vec<_>>(),
        "gain_y":pid.gains.p.y.iter().copied().map(scalar_553).collect::<Vec<_>>(),
        "used_kp_scalar_observed":false});
    let raw = serde_json::to_vec(&receipt)?;
    if raw.len() > 64 * 1024 {
        return Err("#553 scalar receipt exceeds 64KiB".into());
    }
    writeln!(
        std::io::stderr().lock(),
        "CONTROLSD_553_SCALAR {}",
        std::str::from_utf8(&raw)?
    )?;
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().collect::<Vec<_>>();
    let output_path = args.get(1).ok_or("output path")?;
    let numerics = std::env::var("CONTROLS_NUMERICS")?;
    let assets =
        std::env::var("CONTROLS_ASSETS").unwrap_or("opendbc_repo/opendbc/car/torque_data".into());
    let request: Request = serde_json::from_reader(std::io::stdin())?;
    #[cfg(feature = "diagnostic-553")]
    if request.cases.len() != 1 || request.cases[0].frames.len() != 1 {
        return Err("#553 getter requires the bound one-case/one-frame input".into());
    }
    #[cfg(feature = "diagnostic-553")]
    let mut diagnostic_controls = None;
    let mut results = Vec::new();
    for case in request.cases {
        let mut store = Store {
            values: case.params,
            operations: Vec::new(),
        };
        let config = Config::decode(store.read("get", "CarParams"))?;
        let printed = format!(
            "########get_nn_model_path : {} {}\n",
            config.fingerprint, config.firmware
        );
        let interface = ControlInterface::new(
            &config,
            &mut store,
            Path::new(&assets),
            Numerics::load(Path::new(&numerics))?,
        )?;
        let mut controls = Controls::new(config, interface, &mut store)?;
        let mut state = State::new(
            &openpilot_controlsd::SERVICES,
            Options {
                poll: Poll::One("selfdriveState".into()),
                simulation: case.simulation,
                ..Options::default()
            },
        )?;
        let mut rows = Vec::new();
        let mut logs = vec![
            json!(["info", "controlsd is waiting for CarParams"]),
            json!(["info", "controlsd got CarParams"]),
        ];
        for frame in case.frames {
            store.values.extend(frame.params);
            state.update(frame.time, &frame.messages)?;
            let input = openpilot_controlsd::input_decode::decode(&state, frame.time)?;
            let command = controls.control(&input, &mut store)?;
            #[cfg(feature = "diagnostic-553")]
            record_scalar_553(&case.name, &controls, command.log.error, input.car.speed)?;
            logs.extend(command.errors.iter().map(|error| json!(["error", error])));
            let packets = openpilot_controlsd::publication::publish(
                &mut controls,
                &input,
                &command,
                &mut store,
                || (frame.time * 1e9) as u64,
            )?;
            rows.push(json!({"packets":packets,"params":std::mem::take(&mut store.operations),"logs":std::mem::take(&mut logs),"state":snapshot(&controls)}));
        }
        results.push(json!({"name":case.name,"printed":printed,"rows":rows}));
        #[cfg(feature = "diagnostic-553")]
        {
            diagnostic_controls = Some(controls);
        }
    }
    let mut output = BufWriter::new(std::fs::File::create(output_path)?);
    serde_json::to_writer(&mut output, &results)?;
    output.flush()?;
    #[cfg(feature = "diagnostic-553")]
    diagnostic_controls
        .as_ref()
        .ok_or("#553 missing live Controls instance")?
        .interface
        .numerics
        .diagnostic_553_getters(Path::new(&numerics))?;
    Ok(())
}
