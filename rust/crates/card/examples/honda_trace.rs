use openpilot_card::{
    brands::honda::{self, Honda, ParamsInput, Setup},
    core::ApplyInput,
    runtime::Common,
};
use openpilot_cereal::car_capnp::{car_control, car_state};
use openpilot_params::Params;
use serde_json::json;
use std::{
    io::{self, Read},
    path::Path,
};
#[path = "honda_trace/input.rs"]
mod input;
#[path = "honda_trace/io.rs"]
mod trace_io;
use input::Case;

fn trace(case: Case, paths: [&Path; 3]) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let [dbc, assets, numerics] = paths;
    let root = tempfile::Builder::new().prefix("card-honda-").tempdir()?;
    let settings = Params::open(root.path(), "d")?;
    for (key, value) in &case.settings {
        settings.put(key, value.as_bytes())?;
    }
    let mut cp = honda::parameters(ParamsInput {
        candidate: &case.candidate,
        fingerprints: &case.fingerprints,
        firmware: &case.firmware,
        alpha_long: case.alpha_long,
        settings: &settings,
    })?;
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
            json!({"params":capnp::serialize::write_message_to_words(&cp),"writes":writes,"parameter_prints":parameter_prints}),
        );
    }
    if !matches!(
        case.op.as_str(),
        "runtime" | "before_update" | "numeric_error"
    ) {
        return Err(case.name.into());
    }
    {
        let mut params = cp.get_root::<openpilot_cereal::car_capnp::car_params::Builder>()?;
        let flags = params.reborrow_as_reader().get_flags();
        params.set_flags(flags | case.flags_or);
        if case.offset {
            let mut safety = params.init_safety_configs(2);
            safety
                .reborrow()
                .get(0)
                .set_safety_model(openpilot_cereal::car_capnp::car_params::SafetyModel::NoOutput);
        }
    }
    let mut firmware = cp
        .get_root::<openpilot_cereal::car_capnp::car_params::Builder>()?
        .init_car_fw(u32::try_from(case.firmware.len())?);
    for (index, source) in case.firmware.iter().enumerate() {
        let mut fw = firmware.reborrow().get(u32::try_from(index)?);
        if source.ecu != openpilot_card::firmware::Ecu::Eps {
            return Err("fixture ECU".into());
        }
        fw.set_ecu(openpilot_cereal::car_capnp::car_params::Ecu::Eps);
        fw.set_fw_version(&source.fw_version);
    }
    let bytes = capnp::serialize::write_message_to_words(&cp);
    let observer = Params::open(root.path(), "d")?;
    let mut vehicle = Honda::new(Setup {
        params_bytes: &bytes,
        dbc_root: dbc,
        settings,
        fingerprints: &case.fingerprints,
        now_ns: case.now,
    })?;
    let common = Common::new(cp.get_root_as_reader()?, &observer, assets, numerics)?;
    let initial_state = capnp::serialize::write_message_to_words(&vehicle.state.out);
    let initial_extra = json!(vehicle.state.extras);
    let initial_state_snapshot = json!(vehicle.state.snapshot());
    let initial_controller = json!(vehicle.controller.snapshot());
    let initial_packer_counters = vehicle.controller.packer_counters();
    let mut io = trace_io::Io::default();
    vehicle.init(&mut io)?;
    let mut steps = Vec::new();
    let mut error = None;
    for input in case.steps {
        for (key, value) in input.settings {
            observer.put(&key, value.as_bytes())?;
        }
        let state = if case.op == "before_update" {
            None
        } else {
            let mut state = vehicle.update(&input.packets, input.now)?;
            vehicle.set_soft_hold(input.soft_hold);
            let mut cs = state.get_root::<car_state::Builder>()?;
            cs.set_v_cruise(input.commit.cruise);
            cs.set_activate_cruise(input.commit.activate);
            vehicle.commit_state(state.get_root_as_reader()?)?;
            Some(state)
        };
        let control = capnp::serialize::read_message(
            std::io::Cursor::new(input.control),
            capnp::message::ReaderOptions::new(),
        )?;
        let applied = vehicle.apply(ApplyInput {
            control: control.get_root::<car_control::Reader>()?,
            now_ns: input.now,
            model: None,
            radar: None,
        });
        let output = match applied {
            Ok(output) if case.op == "runtime" => output,
            Ok(_) => {
                return Err(
                    format!("Honda error fixture unexpectedly succeeded: {}", case.name).into(),
                );
            }
            Err(failure) if case.op != "runtime" => {
                error = Some(match failure {
                    honda::Error::Stock(_) => json!({"kind":"Stock","message":failure.to_string()}),
                    honda::Error::Numeric => json!({"kind":"Numeric"}),
                    honda::Error::Can(openpilot_can::Error::Numeric) => {
                        json!({"kind":"CanNumeric"})
                    }
                    honda::Error::SettingInteger { key, .. } => {
                        json!({"kind":"SettingInteger","key":key})
                    }
                    other => return Err(other.into()),
                });
                break;
            }
            Err(failure) => return Err(failure.into()),
        };
        let state = state.ok_or("missing updated state")?;
        let logs = vehicle
            .take_logs()
            .into_iter()
            .map(|l| vec![format!("{:?}", l.level).to_ascii_uppercase(), l.message])
            .collect::<Vec<_>>();
        steps.push(json!({"state":capnp::serialize::write_message_to_words(&state),"actuators":capnp::serialize::write_message_to_words(&output.actuators),"can":output.can,"extra":vehicle.state.extras,"state_snapshot":vehicle.state.snapshot(),"controller":vehicle.controller.snapshot(),"packer_counters":vehicle.controller.packer_counters(),"logs":logs,"soft_hold":vehicle.state.soft_hold,"is_metric":vehicle.state.is_metric}));
    }
    let final_logs = vehicle
        .take_logs()
        .into_iter()
        .map(|l| vec![format!("{:?}", l.level).to_ascii_uppercase(), l.message])
        .collect::<Vec<_>>();
    vehicle.deinit(&mut io)?;
    writes.push(vec![
        "LongitudinalPersonalityMax".to_owned(),
        String::from_utf8(
            observer
                .get("LongitudinalPersonalityMax")?
                .ok_or("missing constructor write")?,
        )?,
    ]);
    Ok(
        json!({"params":bytes,"writes":writes,"parameter_prints":parameter_prints,"steps":steps,"lifecycle":io.calls,"lifecycle_can":io.sent,"lifecycle_logs":io.logs,"prints":[],
        "common":{"use_nnff":common.use_nnff,"use_nnff_lite":common.use_nnff_lite,"model_present":common.model_path.is_some()},
        "initial_state":initial_state,"initial_extra":initial_extra,"initial_state_snapshot":initial_state_snapshot,"initial_controller":initial_controller,"initial_packer_counters":initial_packer_counters,
        "pre_update_error":error,"final_extra":vehicle.state.extras,"final_state_snapshot":vehicle.state.snapshot(),"final_controller":vehicle.controller.snapshot(),"final_packer_counters":vehicle.controller.packer_counters(),"final_logs":final_logs}),
    )
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let cases: Vec<Case> = serde_json::from_str(&input)?;
    let args: Vec<_> = std::env::args().collect();
    let path = args.get(1).ok_or("missing output")?;
    let dbc = args.get(2).ok_or("missing DBC")?;
    let assets = args.get(3).ok_or("missing common assets")?;
    let numerics = args.get(4).ok_or("missing numerics")?;
    let result = cases
        .into_iter()
        .map(|c| trace(c, [Path::new(dbc), Path::new(assets), Path::new(numerics)]))
        .collect::<Result<Vec<_>, _>>()?;
    std::fs::write(path, serde_json::to_vec(&result)?)?;
    Ok(())
}
