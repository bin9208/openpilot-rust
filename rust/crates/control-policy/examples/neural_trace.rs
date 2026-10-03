use openpilot_control_policy::{flux::Flux, nano::Nano, numerics::Numerics, numpy_exp};
use serde::Deserialize;
use serde_json::json;
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Deserialize)]
struct Request {
    inputs: Vec<Vec<f64>>,
    exp: Vec<u32>,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let kernel = Numerics::load(&PathBuf::from(args.next().ok_or("numerics")?))?;
    let root = PathBuf::from(args.next().ok_or("assets")?);
    let output = args.next().ok_or("output")?;
    let request: Request = serde_json::from_reader(std::io::stdin())?;
    let mut flux = BTreeMap::new();
    for entry in std::fs::read_dir(root.join("lat_models"))? {
        let path = entry?.path();
        if path.extension().and_then(|v| v.to_str()) != Some("json") {
            continue;
        }
        let model = Flux::decode(&std::fs::read(&path)?, &kernel)?;
        let values = request
            .inputs
            .iter()
            .map(|input| model.evaluate(input, &kernel))
            .collect::<Result<Vec<_>, _>>()?;
        flux.insert(
            path.file_name()
                .ok_or("filename")?
                .to_string_lossy()
                .into_owned(),
            json!({"values":values,"friction":model.friction_override}),
        );
    }
    let models: BTreeMap<String, Nano> =
        serde_json::from_slice(&std::fs::read(root.join("neural_ff_weights.json"))?)?;
    let nano = models
        .into_iter()
        .map(|(name, model)| {
            let values = request
                .inputs
                .iter()
                .map(|input| model.predict(&input[..4], &kernel))
                .collect::<Result<Vec<_>, _>>()?;
            Ok((name, values))
        })
        .collect::<Result<BTreeMap<_, _>, openpilot_control_policy::Error>>()?;
    let exp: Vec<u32> = request
        .exp
        .into_iter()
        .map(|bits| numpy_exp::exp(f32::from_bits(bits)).to_bits())
        .collect();
    serde_json::to_writer(
        std::fs::File::create(output)?,
        &json!({"flux":flux,"nano":nano,"exp":exp}),
    )?;
    Ok(())
}
