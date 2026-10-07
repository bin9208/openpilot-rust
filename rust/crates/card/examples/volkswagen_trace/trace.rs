use crate::{input::Case, parser, steps};
use openpilot_card::{
    brands::volkswagen::{self, ParamsInput, Setup, Volkswagen},
    runtime::Common,
};
use openpilot_cereal::car_capnp::{car_params, car_state};
use openpilot_params::Params;
use serde_json::{json, Value};
use std::path::Path;
pub fn run(case: Case, paths: [&Path; 3]) -> Result<Value, Box<dyn std::error::Error>> {
    let [dbc, assets, numerics] = paths;
    let root = tempfile::Builder::new()
        .prefix("card-volkswagen-")
        .tempdir()?;
    let settings = Params::open(root.path(), "d")?;
    for (key, value) in &case.settings {
        settings.put(key, value.as_bytes())?;
    }
    let mut cp = volkswagen::parameters(ParamsInput {
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
    let mut params = cp.get_root::<car_params::Builder>()?;
    let flags = params.reborrow_as_reader().get_flags();
    params.set_flags(flags | case.flags_or);
    let bytes = capnp::serialize::write_message_to_words(&cp);
    let observer = Params::open(root.path(), "d")?;
    let mut vehicle = Volkswagen::new(Setup {
        params_bytes: &bytes,
        dbc_root: dbc,
        settings,
        fingerprints: &case.fingerprints,
        now_ns: case.now,
    })?;
    let common = Common::new(cp.get_root_as_reader()?, &observer, assets, numerics)?;
    if let Some(data) = &case.seed_state {
        let reader = capnp::serialize::read_message(
            std::io::Cursor::new(data),
            capnp::message::ReaderOptions::new(),
        )?;
        vehicle.commit_state(reader.get_root::<car_state::Reader>()?)?;
    }
    if let Some(value) = &case.seed_extra {
        vehicle.state.extras = serde_json::from_value(value.clone())?;
    }
    if let Some(value) = &case.seed_history {
        vehicle.controller.history = serde_json::from_value(value.clone())?;
    }
    let initial = json!({"initial_state":capnp::serialize::write_message_to_words(&vehicle.state.out),"initial_extra":vehicle.state.extras,"initial_state_snapshot":vehicle.state.snapshot(),"initial_controller":vehicle.controller.snapshot(),"initial_packer_counters":vehicle.controller.packer_counters(),"initial_parsers":[parser::snapshot(&vehicle.state.pt),parser::snapshot(&vehicle.state.camera)]});
    let mut io = crate::trace_io::Io::default();
    vehicle.init(&mut io)?;
    let (rows, error) = steps::run(&mut vehicle, &case)?;
    vehicle.deinit(&mut io)?;
    writes.push(vec![
        "LongitudinalPersonalityMax".to_owned(),
        String::from_utf8(
            observer
                .get("LongitudinalPersonalityMax")?
                .ok_or("constructor write")?,
        )?,
    ]);
    let mut value = json!({"params":bytes,"writes":writes,"parameter_prints":parameter_prints,"steps":rows,"prints":[],"common":{"use_nnff":common.use_nnff,"use_nnff_lite":common.use_nnff_lite,"model_present":common.model_path.is_some()},"error":error,"final_extra":vehicle.state.extras,"final_state_snapshot":vehicle.state.snapshot(),"final_controller":vehicle.controller.snapshot(),"final_packer_counters":vehicle.controller.packer_counters(),"final_parsers":[parser::snapshot(&vehicle.state.pt),parser::snapshot(&vehicle.state.camera)],"final_state":capnp::serialize::write_message_to_words(&vehicle.state.out),"final_logs":steps::logs(&mut vehicle)});
    let fields = value.as_object_mut().ok_or("trace object")?;
    fields.insert("lifecycle".into(), json!(io.calls));
    fields.extend(initial.as_object().ok_or("initial trace")?.clone());
    Ok(value)
}
