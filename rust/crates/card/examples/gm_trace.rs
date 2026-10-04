use openpilot_can::{Frame, Packet};
use openpilot_card::{
    brands::gm::{self, Gm, ParamsInput, Setup},
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
    settings: BTreeMap<String, String>,
    is_metric: bool,
}
#[derive(Deserialize)]
struct Commit {
    #[serde(rename = "vCruise")]
    cruise: f32,
    #[serde(rename = "activateCruise")]
    activate: i16,
    #[serde(rename = "softHoldActive")]
    soft_hold: i16,
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
    let root = tempfile::Builder::new().prefix("card-gm-").tempdir()?;
    let settings = Params::open(root.path(), "d")?;
    for (key, value) in &case.settings {
        settings.put(key, value.as_bytes())?;
    }
    let parameter_logs: Vec<Vec<String>> = Vec::new();
    let params_result = gm::parameters(ParamsInput {
        candidate: &case.candidate,
        fingerprints: &case.fingerprints,
        firmware: &case.firmware,
        alpha_long: case.alpha_long,
        settings: &settings,
    });
    let mut cp = match params_result {
        Ok(cp) => cp,
        Err(gm::Error::Baseline(openpilot_card::vehicle_params::Error::MissingTorque(
            candidate,
        ))) => {
            return Ok(json!({"error":{"kind":"MissingTorque","candidate":candidate},"writes":[]}));
        }
        Err(error) => return Err(error.into()),
    };
    let parameter_prints = openpilot_card::vehicle_params::parameter_diagnostics(
        openpilot_card::vehicle_params::DiagnosticInput {
            params: cp.get_root_as_reader()?,
            firmware: &case.firmware,
            settings: &settings,
            assets,
        },
    )?;
    let mut writes = Vec::new();
    if let Some(value) = settings.get("NNFFModelName")? {
        writes.push(vec!["NNFFModelName".to_owned(), String::from_utf8(value)?]);
    }
    if case.op == "params" {
        return Ok(
            json!({"params":capnp::serialize::write_message_to_words(&cp),"writes":writes,"parameter_prints":parameter_prints,"parameter_logs":parameter_logs}),
        );
    }
    if case.op != "runtime"
        && case.op != "before_update"
        && case.op != "numeric_error"
        && case.op != "state_error"
    {
        return Err(case.name.into());
    }
    let mut firmware = cp
        .get_root::<openpilot_cereal::car_capnp::car_params::Builder>()?
        .init_car_fw(u32::try_from(case.firmware.len())?);
    for (index, source) in case.firmware.iter().enumerate() {
        if source.ecu != openpilot_card::firmware::Ecu::Eps {
            return Err("Gm fixture expects EPS firmware".into());
        }
        let mut fw = firmware.reborrow().get(u32::try_from(index)?);
        fw.set_ecu(openpilot_cereal::car_capnp::car_params::Ecu::Eps);
        fw.set_fw_version(&source.fw_version);
    }
    let bytes = capnp::serialize::write_message_to_words(&cp);
    let observer = Params::open(root.path(), "d")?;
    let mut vehicle = Gm::new(Setup {
        params_bytes: &bytes,
        dbc_root: dbc,
        settings,
        fingerprints: &case.fingerprints,
        now_ns: case.now,
    })?;
    let common = Common::new(cp.get_root_as_reader()?, &observer, assets, numerics)?;
    let personality = String::from_utf8(
        observer
            .get("LongitudinalPersonalityMax")?
            .ok_or("missing constructor Params write")?,
    )?;
    writes.push(vec!["LongitudinalPersonalityMax".to_owned(), personality]);

    let initial_state = capnp::serialize::write_message_to_words(&vehicle.state.out);
    let initial_extra = json!(vehicle.state.extras);
    let initial_state_snapshot = json!(vehicle.state.snapshot());
    let initial_controller =
        json!({"state":vehicle.controller.snapshot,"limits":vehicle.controller.limits});
    let initial_packer_counters = vehicle.controller.packer_counters();
    let mut io = Io::default();
    vehicle.init(&mut io)?;
    let mut steps = Vec::new();
    let mut prints = vehicle.take_diagnostics();
    let mut pre_update_error = None;
    for input in case.steps {
        if case.op == "before_update" {
            let control = capnp::serialize::read_message(
                std::io::Cursor::new(input.control),
                capnp::message::ReaderOptions::new(),
            )?;
            let error = match vehicle.apply(ApplyInput {
                control: control.get_root::<car_control::Reader>()?,
                now_ns: input.now,
                model: None,
                radar: None,
            }) {
                Err(error @ gm::Error::Stock(_)) => error,
                Err(error) => return Err(error.into()),
                Ok(_) => return Err("initial Gm apply unexpectedly succeeded".into()),
            };
            pre_update_error = Some(json!({"kind":"Stock","message":error.to_string()}));
            break;
        }
        for (key, value) in input.settings {
            observer.put(&key, value.as_bytes())?;
        }
        let mut state: Message = match vehicle.update(&input.packets, input.now) {
            Ok(state) => state,
            Err(gm::Error::Can(openpilot_can::Error::Message(message)))
                if case.op == "state_error" =>
            {
                pre_update_error = Some(json!({"kind":"MissingMessage","message":message}));
                break;
            }
            Err(error) => return Err(error.into()),
        };
        vehicle.set_soft_hold(input.soft_hold);
        vehicle.state.is_metric = input.is_metric;
        let mut cs = state.get_root::<car_state::Builder>()?;
        cs.set_v_cruise(input.commit.cruise);
        cs.set_activate_cruise(input.commit.activate);
        cs.set_soft_hold_active(input.commit.soft_hold);
        vehicle.commit_state(state.get_root_as_reader()?)?;
        let control = capnp::serialize::read_message(
            std::io::Cursor::new(input.control),
            capnp::message::ReaderOptions::new(),
        )?;
        let write_start = writes.len();
        let output = match vehicle.apply(ApplyInput {
            control: control.get_root::<car_control::Reader>()?,
            now_ns: input.now,
            model: None,
            radar: None,
        }) {
            Ok(output) => output,
            Err(gm::Error::Numeric) if case.op == "numeric_error" => {
                pre_update_error = Some(json!({"kind":"Numeric"}));
                break;
            }
            Err(gm::Error::Can(openpilot_can::Error::Numeric)) if case.op == "numeric_error" => {
                pre_update_error = Some(json!({"kind":"CanNumeric"}));
                break;
            }
            Err(error) => return Err(error.into()),
        };
        for (key, value) in vehicle.take_param_writes() {
            observer.put(&key, &value)?;
            writes.push(vec![key, String::from_utf8(value)?]);
        }
        prints.extend(vehicle.take_diagnostics());
        let logs = vehicle
            .take_logs()
            .into_iter()
            .map(|l| vec![format!("{:?}", l.level).to_ascii_uppercase(), l.message])
            .collect::<Vec<_>>();
        steps.push(json!({"state":capnp::serialize::write_message_to_words(&state),"actuators":capnp::serialize::write_message_to_words(&output.actuators),"can":output.can,"extra":vehicle.state.extras,"state_snapshot":vehicle.state.snapshot(),"controller":{"state":vehicle.controller.snapshot,"limits":vehicle.controller.limits},"packer_counters":vehicle.controller.packer_counters(),"logs":logs,"writes":&writes[write_start..],"soft_hold":vehicle.state.soft_hold,"is_metric":vehicle.state.is_metric}));
    }
    vehicle.deinit(&mut io)?;
    Ok(
        json!({"params":bytes,"writes":writes,"parameter_prints":parameter_prints,"parameter_logs":parameter_logs,"steps":steps,"lifecycle":io.calls,"prints":prints,
        "common":{"use_nnff":common.use_nnff,"use_nnff_lite":common.use_nnff_lite,"model_present":common.model_path.is_some()},
        "initial_state":initial_state,"initial_extra":initial_extra,"initial_controller":initial_controller,"initial_packer_counters":initial_packer_counters,
        "pre_update_error":pre_update_error,"initial_state_snapshot":initial_state_snapshot,"final_state_snapshot":vehicle.state.snapshot(),"final_extra":vehicle.state.extras,"final_controller":{"state":vehicle.controller.snapshot,"limits":vehicle.controller.limits},
        "final_packer_counters":vehicle.controller.packer_counters()}),
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
