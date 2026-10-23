use openpilot_can::{parser::Parser, Frame, Packet};
use openpilot_card::{
    brands::psa::{self, ParamsInput, Psa, Setup},
    core::ApplyInput,
    firmware::Firmware,
    firmware_query::StartupIo,
    isotp,
    query::QueryIo,
    runtime::Common,
};
use openpilot_cereal::car_capnp::{car_control, car_params};
use openpilot_params::Params;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    io::{self, Read},
    path::Path,
};

#[derive(Deserialize)]
struct Case {
    op: String,
    candidate: String,
    alpha_long: bool,
    firmware: Vec<Firmware>,
    fingerprints: Vec<(u8, Vec<(u32, usize)>)>,
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
}
#[derive(Default)]
struct Io {
    calls: Vec<Vec<String>>,
}
impl QueryIo for Io {
    fn receive(&mut self, _: bool) -> Result<Vec<Vec<Frame>>, isotp::Error> {
        self.calls.push(vec!["recv".into()]);
        Ok(Vec::new())
    }
    fn send(&mut self, _: &[Frame]) -> Result<(), isotp::Error> {
        self.calls.push(vec!["send".into()]);
        Ok(())
    }
    fn sleep(&mut self, _: f64) -> Result<(), isotp::Error> {
        Ok(())
    }
    fn now(&mut self) -> f64 {
        0.
    }
}
impl StartupIo for Io {
    fn set_obd_multiplexing(&mut self, _: bool) -> Result<(), isotp::Error> {
        Ok(())
    }
}
fn parser_snapshot(parser: &Parser) -> Value {
    let values: BTreeMap<_, _> = parser
        .states
        .iter()
        .map(|(a, s)| (*a, s.values.clone()))
        .collect();
    let all: BTreeMap<_, _> = parser
        .states
        .iter()
        .map(|(a, s)| (*a, s.all_values.clone()))
        .collect();
    let counters: BTreeMap<_, _> = parser
        .states
        .iter()
        .map(|(a, s)| (*a, s.counter.to_string()))
        .collect();
    let failures: BTreeMap<_, _> = parser
        .states
        .iter()
        .map(|(a, s)| (*a, s.counter_fail))
        .collect();
    let frequencies: BTreeMap<_, _> = parser
        .states
        .iter()
        .map(|(a, s)| (*a, s.frequency))
        .collect();
    let thresholds: BTreeMap<_, _> = parser
        .states
        .iter()
        .map(|(a, s)| (*a, s.timeout_threshold))
        .collect();
    let timestamps: BTreeMap<_, _> = parser
        .states
        .iter()
        .map(|(a, s)| (*a, s.timestamps.iter().copied().collect::<Vec<_>>()))
        .collect();
    json!({"updated":[],"checks":[],"bus_timeout":parser.bus_timeout(),"values":values,
        "all_values":all,"counters":counters,"failures":failures,"frequencies":frequencies,
        "thresholds":thresholds,"timestamps":timestamps,"raw":parser.raw,"seen":parser.seen_addresses,
        "last_nonempty":parser.last_nonempty,"last_update":parser.last_update,"invalid_count":parser.invalid_count})
}
fn trace(
    case: Case,
    dbc: &Path,
    assets: &Path,
    numerics: &Path,
) -> Result<Value, Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let settings = Params::open(root.path(), "d")?;
    for (key, value) in &case.settings {
        settings.put(key, value.as_bytes())?;
    }
    let mut cp = psa::parameters(ParamsInput {
        candidate: &case.candidate,
        fingerprints: &case.fingerprints,
        firmware: &case.firmware,
        alpha_long: case.alpha_long,
        settings: &settings,
    })?;
    if case.op == "params" {
        return Ok(json!({"params":capnp::serialize::write_message_to_words(&cp),"writes":[]}));
    }
    if !case.firmware.is_empty() {
        return Err("PSA runtime fixtures require empty firmware".into());
    }
    cp.get_root::<car_params::Builder>()?.init_car_fw(0);
    let bytes = capnp::serialize::write_message_to_words(&cp);
    let observer = Params::open(root.path(), "d")?;
    let mut vehicle = Psa::new(Setup {
        params_bytes: &bytes,
        dbc_root: dbc,
        settings,
        fingerprints: &case.fingerprints,
        now_ns: case.now,
    })?;
    let _common = Common::new(cp.get_root_as_reader()?, &observer, assets, numerics)?;
    let mut io = Io::default();
    vehicle.init(&mut io)?;
    let mut rows = Vec::new();
    let mut failure = None;
    for step in case.steps {
        let state = if case.op == "before_update" {
            let mut message = openpilot_card::core::Message::new_default();
            message.set_root(
                vehicle
                    .state
                    .out
                    .get_root_as_reader::<openpilot_cereal::car_capnp::car_state::Reader>()?,
            )?;
            message
        } else {
            vehicle.update(&step.packets, step.now)?
        };
        vehicle.set_soft_hold(step.soft_hold);
        let control = capnp::serialize::read_message(
            std::io::Cursor::new(&step.control),
            capnp::message::ReaderOptions::new(),
        )?;
        let output = match vehicle.apply(ApplyInput {
            control: control.get_root::<car_control::Reader>()?,
            now_ns: step.now,
            model: None,
            radar: None,
        }) {
            Ok(output) => output,
            Err(error) if case.op != "runtime" => {
                failure = Some(match error {
                    psa::Error::Stock(attribute) => json!({"kind":"Stock","attribute":attribute}),
                    psa::Error::Numeric | psa::Error::Can(openpilot_can::Error::Numeric) => {
                        json!({"kind":"Numeric"})
                    }
                    other => return Err(other.into()),
                });
                break;
            }
            Err(error) => return Err(error.into()),
        };
        let counters: BTreeMap<_, _> = vehicle
            .controller
            .packer
            .counters
            .iter()
            .map(|(a, v)| (*a, v.to_string()))
            .collect();
        let parsers = [
            &vehicle.state.main,
            &vehicle.state.adas,
            &vehicle.state.camera,
        ]
        .map(parser_snapshot);
        let logs: Vec<_> = vehicle
            .take_logs()
            .into_iter()
            .map(|l| vec![format!("{:?}", l.level).to_ascii_uppercase(), l.message])
            .collect();
        rows.push(json!({"state":capnp::serialize::write_message_to_words(&state),"actuators":capnp::serialize::write_message_to_words(&output.actuators),
            "can":output.can,"snapshot":vehicle.state.snapshot(),"controller":vehicle.controller,"counters":counters,
            "parsers":parsers,"logs":logs,"soft_hold":vehicle.state.soft_hold}));
    }
    vehicle.deinit(&mut io)?;
    let personality = String::from_utf8(
        observer
            .get("LongitudinalPersonalityMax")?
            .ok_or("missing constructor write")?,
    )?;
    Ok(
        json!({"params":bytes,"writes":[["LongitudinalPersonalityMax",personality]],"steps":rows,"error":failure,"lifecycle":io.calls,"prints":[]}),
    )
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let cases: Vec<Case> = serde_json::from_str(&input)?;
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 5 {
        return Err("expected output, DBC root, common assets and numerics".into());
    }
    let result = cases
        .into_iter()
        .map(|case| {
            trace(
                case,
                Path::new(&args[2]),
                Path::new(&args[3]),
                Path::new(&args[4]),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    std::fs::write(&args[1], serde_json::to_vec(&result)?)?;
    Ok(())
}
