use openpilot_card::{
    brands::psa::{self, ParamsInput, Psa, Setup, State},
    core::Message,
    firmware::Firmware,
    vehicle_params,
};
use openpilot_cereal::car_capnp::car_params;
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
    candidate: String,
    alpha_long: bool,
    fingerprints: Vec<(u8, Vec<(u32, usize)>)>,
    firmware: Vec<Firmware>,
    settings: BTreeMap<String, String>,
}
fn trace(case: Case, dbc: &Path) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let settings = Params::open(root.path(), "d")?;
    for (key, value) in &case.settings {
        settings.put(key, value.as_bytes())?;
    }
    let mut cp = Message::new_default();
    cp.init_root::<car_params::Builder>()
        .set_car_fingerprint(&case.candidate);
    let failure = match case.op.as_str() {
        "params" => match psa::parameters(ParamsInput {
            candidate: &case.candidate,
            fingerprints: &case.fingerprints,
            firmware: &case.firmware,
            alpha_long: case.alpha_long,
            settings: &settings,
        }) {
            Err(psa::Error::Baseline(vehicle_params::Error::MissingTorque(candidate))) => {
                json!({"kind":"MissingTorque","candidate":candidate})
            }
            Err(error) => return Err(error.into()),
            Ok(_) => return Err("pinned PSA params unexpectedly succeeded".into()),
        },
        "constructor" => {
            let bytes = capnp::serialize::write_message_to_words(&cp);
            match Psa::new(Setup {
                params_bytes: &bytes,
                dbc_root: dbc,
                settings: Params::open(root.path(), "d")?,
                fingerprints: &[],
                now_ns: 0,
            }) {
                Err(psa::Error::DbcLoad {
                    path,
                    source: openpilot_can::Error::Io(error),
                }) if error.kind() == io::ErrorKind::NotFound => {
                    json!({"kind":"MissingDbc","path":path})
                }
                Err(error) => return Err(error.into()),
                Ok(_) => return Err("pinned PSA constructor unexpectedly succeeded".into()),
            }
        }
        "state" => match State::new()?.update() {
            Err(psa::Error::SourceMethod(method)) => {
                json!({"kind":"MissingMethod","method":method})
            }
            Err(error) => return Err(error.into()),
            Ok(_) => return Err("pinned PSA state unexpectedly succeeded".into()),
        },
        _ => return Err(case.op.into()),
    };
    assert!(settings.get("NNFFModelName")?.is_none());
    assert!(settings.get("LongitudinalPersonalityMax")?.is_none());
    Ok(json!({"failure":failure,"writes":[],"prints":[]}))
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let cases: Vec<Case> = serde_json::from_str(&input)?;
    let output = std::env::args().nth(1).ok_or("missing output")?;
    let dbc = std::env::args().nth(2).ok_or("missing DBC root")?;
    let results = cases
        .into_iter()
        .map(|case| trace(case, Path::new(&dbc)))
        .collect::<Result<Vec<_>, _>>()?;
    std::fs::write(output, serde_json::to_vec(&results)?)?;
    Ok(())
}
