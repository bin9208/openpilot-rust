use openpilot_can::{Frame, Packet};
use openpilot_card::{
    brands::tesla::{self, ParamsInput, Setup, Tesla},
    core::{ApplyInput, Message},
    firmware::Firmware,
    firmware_query::StartupIo,
    isotp,
    query::QueryIo,
    runtime::Common,
};
use openpilot_cereal::car_capnp::{car_control, car_state};
use openpilot_params::Params;
use serde::Deserialize;
use serde_json::json;
use std::{
    collections::BTreeMap,
    io::{self, Read},
    path::Path,
};

#[derive(Deserialize)]
struct Case {
    op: String,
    name: String,
    candidate: String,
    alpha_long: bool,
    fingerprints: Vec<(u8, Vec<(u32, usize)>)>,
    firmware: Vec<Firmware>,
    settings: BTreeMap<String, String>,
    now: u64,
    steps: Vec<Step>,
}
#[derive(Deserialize)]
struct Step {
    now: u64,
    packets: Vec<Packet>,
    control: Vec<u8>,
    soft_hold: i16,
    commit: Commit,
}
#[derive(Deserialize)]
struct Commit {
    #[serde(rename = "vCruise")]
    cruise: f32,
    #[serde(rename = "activateCruise")]
    activate: i16,
}
#[derive(Default)]
struct Io {
    calls: Vec<Vec<String>>,
}
impl QueryIo for Io {
    fn receive(&mut self, _wait: bool) -> Result<Vec<Vec<Frame>>, isotp::Error> {
        self.calls.push(vec!["recv".into()]);
        Ok(Vec::new())
    }
    fn send(&mut self, _frames: &[Frame]) -> Result<(), isotp::Error> {
        self.calls.push(vec!["send".into()]);
        Ok(())
    }
    fn sleep(&mut self, _seconds: f64) -> Result<(), isotp::Error> {
        self.calls.push(vec!["sleep".into()]);
        Ok(())
    }
    fn now(&mut self) -> f64 {
        0.
    }
}
impl StartupIo for Io {
    fn set_obd_multiplexing(&mut self, _enabled: bool) -> Result<(), isotp::Error> {
        self.calls.push(vec!["obd".into()]);
        Ok(())
    }
}
fn trace(
    case: Case,
    dbc: &Path,
    assets: &Path,
    numerics: &Path,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let root = tempfile::Builder::new().prefix("card-tesla-").tempdir()?;
    let settings = Params::open(root.path(), "d")?;
    for (key, value) in &case.settings {
        settings.put(key, value.as_bytes())?;
    }
    let mut cp = match tesla::parameters(ParamsInput {
        candidate: &case.candidate,
        fingerprints: &case.fingerprints,
        firmware: &case.firmware,
        alpha_long: case.alpha_long,
        settings: &settings,
    }) {
        Ok(cp) => cp,
        Err(tesla::Error::Baseline(openpilot_card::vehicle_params::Error::MissingTorque(_))) => {
            return Ok(json!({"error":"missing_torque"}))
        }
        Err(error) => return Err(error.into()),
    };
    if case.op == "params" {
        let bytes = capnp::serialize::write_message_to_words(&cp);
        return Ok(json!({"params":bytes,"writes":[]}));
    }
    if case.op != "runtime" {
        return Err(case.name.into());
    }
    let mut firmware = cp
        .get_root::<openpilot_cereal::car_capnp::car_params::Builder>()?
        .init_car_fw(u32::try_from(case.firmware.len())?);
    for (index, source) in case.firmware.iter().enumerate() {
        if source.ecu != openpilot_card::firmware::Ecu::Eps {
            return Err("Tesla fixture expects EPS firmware".into());
        }
        let mut fw = firmware.reborrow().get(u32::try_from(index)?);
        fw.set_ecu(openpilot_cereal::car_capnp::car_params::Ecu::Eps);
        fw.set_fw_version(&source.fw_version);
    }
    let bytes = capnp::serialize::write_message_to_words(&cp);
    let observer = Params::open(root.path(), "d")?;
    let mut vehicle = Tesla::new(Setup {
        params_bytes: &bytes,
        dbc_root: dbc,
        settings,
        fingerprints: &case.fingerprints,
        now_ns: case.now,
    })?;
    let _common = Common::new(cp.get_root_as_reader()?, &observer, assets, numerics)?;
    let mut io = Io::default();
    vehicle.init(&mut io)?;
    let mut steps = Vec::new();
    let mut prints = vehicle.take_diagnostics();
    for input in case.steps {
        let mut state: Message = vehicle.update(&input.packets, input.now)?;
        vehicle.set_soft_hold(input.soft_hold);
        let mut cs = state.get_root::<car_state::Builder>()?;
        cs.set_v_cruise(input.commit.cruise);
        cs.set_activate_cruise(input.commit.activate);
        vehicle.commit_state(state.get_root_as_reader()?)?;
        let control = capnp::serialize::read_message(
            std::io::Cursor::new(input.control),
            capnp::message::ReaderOptions::new(),
        )?;
        let output = vehicle.apply(ApplyInput {
            control: control.get_root::<car_control::Reader>()?,
            now_ns: input.now,
            model: None,
            radar: None,
        })?;
        prints.extend(vehicle.take_diagnostics());
        let logs = vehicle
            .take_logs()
            .into_iter()
            .map(|l| vec![format!("{:?}", l.level).to_ascii_uppercase(), l.message])
            .collect::<Vec<_>>();
        steps.push(json!({"state":capnp::serialize::write_message_to_words(&state),"actuators":capnp::serialize::write_message_to_words(&output.actuators),"can":output.can,"extra":vehicle.state.extras,"controller":vehicle.controller.snapshot(),"logs":logs,"soft_hold":vehicle.state.soft_hold}));
    }
    vehicle.deinit(&mut io)?;
    let personality = String::from_utf8(
        observer
            .get("LongitudinalPersonalityMax")?
            .ok_or("missing constructor Params write")?,
    )?;
    Ok(
        json!({"params":bytes,"writes":[["LongitudinalPersonalityMax",personality]],"steps":steps,"lifecycle":io.calls,"prints":prints}),
    )
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let cases: Vec<Case> = serde_json::from_str(&input)?;
    let path = std::env::args().nth(1).ok_or("missing output")?;
    let dbc = std::env::args().nth(2).ok_or("missing DBC root")?;
    let assets = std::env::args().nth(3).ok_or("missing common assets")?;
    let numerics = std::env::args().nth(4).ok_or("missing numerics")?;
    let result = cases
        .into_iter()
        .map(|c| trace(c, Path::new(&dbc), Path::new(&assets), Path::new(&numerics)))
        .collect::<Result<Vec<_>, _>>()?;
    std::fs::write(path, serde_json::to_vec(&result)?)?;
    Ok(())
}
