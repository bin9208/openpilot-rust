use capnp::{
    message::{Builder, ReaderOptions},
    serialize,
};
use openpilot_card::{core::StateTail, cruise::CruiseCarrot};
use openpilot_cereal::car_capnp::{car_params, car_state};
use openpilot_messaging::state::{Options, State};
use openpilot_params::Params;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    io::{self, Cursor, Read},
};

#[derive(Deserialize)]
struct Cp {
    #[serde(rename = "pcmCruise")]
    pcm: bool,
    #[serde(rename = "openpilotLongitudinalControl")]
    long: bool,
}
#[derive(Deserialize)]
struct Frame {
    cs: Vec<u8>,
    events: Vec<Vec<u8>>,
    metric: bool,
    advance: f64,
    params: BTreeMap<String, String>,
    files: BTreeMap<String, serde_json::Value>,
}
#[derive(Deserialize)]
struct Case {
    name: String,
    cp: Cp,
    params: BTreeMap<String, String>,
    frames: Vec<Frame>,
}
#[derive(Serialize)]
struct Output {
    cs: Vec<u8>,
    state: serde_json::Value,
    writes: Vec<[String; 2]>,
    prints: Vec<String>,
}
fn trace(case: Case) -> Result<Vec<Output>, Box<dyn std::error::Error>> {
    let root = tempfile::Builder::new()
        .prefix("card-cruise-native-")
        .tempdir()?;
    let params = Params::open(root.path(), "d")?;
    for (key, value) in case.params {
        params.put(&key, value.as_bytes())?;
    }
    let external = Params::open(root.path(), "d")?;
    let mut cp = Builder::new_default();
    let mut cpb = cp.init_root::<car_params::Builder>();
    cpb.set_pcm_cruise(case.cp.pcm);
    cpb.set_openpilot_longitudinal_control(case.cp.long);
    let mut tail = CruiseCarrot::new(cp.get_root_as_reader()?, params, root.path(), 10.)?;
    let services = [
        "carControl",
        "carrotMan",
        "longitudinalPlan",
        "radarState",
        "drivingModelData",
    ];
    let mut sm = State::new(
        &services,
        Options {
            simulation: true,
            ..Options::default()
        },
    )?;
    let mut previous = Builder::new_default();
    previous.init_root::<car_state::Builder>();
    let mut last = false;
    let mut now = 10.;
    let mut output = Vec::new();
    for frame in case.frames {
        now += frame.advance;
        for (key, value) in frame.params {
            external.put(&key, value.as_bytes())?;
        }
        for (name, value) in frame.files {
            std::fs::write(root.path().join(name), serde_json::to_vec(&value)?)?;
        }
        sm.update(now, &frame.events)?;
        let input = serialize::read_message(Cursor::new(&frame.cs), ReaderOptions::new())?;
        let mut cs = Builder::new_default();
        cs.set_root(input.get_root::<car_state::Reader>()?)?;
        tail.update_at(cs.get_root_as_reader()?, &sm, frame.metric, now)?;
        let openpilot_cereal::log_capnp::event::Which::CarControl(cc) =
            sm.topic("carControl")?.event()?.which()?
        else {
            return Err(case.name.into());
        };
        let enabled = cc?.get_enabled();
        if enabled && !last {
            tail.initialize(previous.get_root_as_reader()?, false)?;
        }
        tail.project(cs.get_root()?)?;
        let writes = tail.take_param_writes();
        let mut captured = Vec::new();
        for (key, value) in writes {
            external.put(&key, &value)?;
            captured.push([key, String::from_utf8(value)?]);
        }
        output.push(Output {
            cs: serialize::write_message_to_words(&cs),
            state: serde_json::to_value(tail.snapshot())?,
            writes: captured,
            prints: tail.take_prints(),
        });
        previous = cs;
        last = enabled;
    }
    Ok(output)
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let cases: Vec<Case> = serde_json::from_str(&input)?;
    let output = cases
        .into_iter()
        .map(trace)
        .collect::<Result<Vec<_>, _>>()?;
    let path = std::env::args().nth(1).ok_or("missing output")?;
    std::fs::write(path, serde_json::to_vec(&output)?)?;
    Ok(())
}
