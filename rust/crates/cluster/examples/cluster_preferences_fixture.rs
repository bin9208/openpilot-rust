use openpilot_cluster::{runtime_args::RuntimeArgs, runtime_preferences::Reader};
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    io::{self, Read},
};
#[derive(Deserialize)]
struct Case {
    values: Option<BTreeMap<String, Vec<u8>>>,
    args: Vec<String>,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let cases: Vec<Case> = serde_json::from_str(&input)?;
    let mut rows = Vec::new();
    for case in cases {
        let directory = tempfile::tempdir()?;
        let params = case
            .values
            .map(|values| -> Result<_, openpilot_params::Error> {
                let params = openpilot_params::Params::open(directory.path(), "fixture")?;
                for (key, value) in values {
                    params.put(&key, &value)?;
                }
                Ok(params)
            })
            .transpose()?;
        let reader = Reader::new(params);
        let mut args = RuntimeArgs::parse_from(std::path::Path::new("."), &case.args)?;
        let encoder_source = reader.apply_encoder(&mut args);
        rows.push(serde_json::json!({"preferences":reader.snapshot(),"encoder_source":encoder_source,"codec":args.usb_codec,"backend":args.usb_h264_backend,"ffmpeg_encoder":args.usb_h264_ffmpeg_encoder}));
    }
    println!("{}", serde_json::to_string(&rows)?);
    Ok(())
}
