use openpilot_card::runtime::Common;
use openpilot_cereal::car_capnp::car_params;
use openpilot_params::Params;
use serde::Deserialize;
use serde_json::json;
use std::{
    io::{self, Read},
    path::Path,
};

#[derive(Deserialize)]
struct Case {
    params: Vec<u8>,
    nnff: bool,
    lite: bool,
    inputs: Vec<Vec<f64>>,
    assets: Option<String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let cases: Vec<Case> = serde_json::from_str(&input)?;
    let output = std::env::args().nth(1).ok_or("missing output path")?;
    let assets = std::env::args().nth(2).ok_or("missing assets path")?;
    let numerics = std::env::args()
        .nth(3)
        .ok_or("missing numerical artifact")?;
    let root = Path::new(&output)
        .parent()
        .ok_or("missing output directory")?
        .join("params");
    let mut results = Vec::new();
    for (index, case) in cases.into_iter().enumerate() {
        let settings = Params::open(&root, &format!("case{index}"))?;
        settings.put_bool("NNFF", case.nnff)?;
        settings.put_bool("NNFFLite", case.lite)?;
        let message = capnp::serialize::read_message(
            std::io::Cursor::new(case.params),
            capnp::message::ReaderOptions::new(),
        )?;
        let common = Common::new(
            message.get_root::<car_params::Reader>()?,
            &settings,
            Path::new(case.assets.as_deref().unwrap_or(&assets)),
            Path::new(&numerics),
        );
        match common {
            Ok(common) => {
                let values: Vec<_> = case
                    .inputs
                    .iter()
                    .map(|input| common.evaluate(input))
                    .collect::<Result<_, _>>()?;
                results.push(json!({"use_nnff": common.use_nnff, "use_nnff_lite": common.use_nnff_lite,
                    "file": common.model_path.as_ref().and_then(|path|path.file_name().and_then(|name|name.to_str())),
                    "friction": common.friction_override(), "values": values, "maximum": settings.get("LongitudinalPersonalityMax")?, "error": false}));
            }
            Err(error) => {
                eprintln!("case {index}: {error}");
                results.push(
                    json!({"maximum": settings.get("LongitudinalPersonalityMax")?, "error": true}),
                );
            }
        }
    }
    std::fs::write(output, serde_json::to_vec(&results)?)?;
    Ok(())
}
