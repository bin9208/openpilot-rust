use openpilot_cluster::runtime_args::{ParseError, RuntimeArgs};
use serde::Deserialize;
use serde_json::{json, Value};
use std::io::Read;
use std::path::PathBuf;
#[derive(Deserialize)]
struct Request {
    root: PathBuf,
    cases: Vec<Vec<String>>,
}
fn output(args: &RuntimeArgs) -> Result<Value, Box<dyn std::error::Error>> {
    let mut rates = Vec::new();
    for tici in [false, true] {
        for egpu in [false, true] {
            rates.push(match args.output_rate(tici, egpu) {
                Ok(rate) => {
                    json!({"value":rate, "bits":format!("{:016x}",rate.target_fps.to_bits())})
                }
                Err(error) => json!({"error":error.to_string()}),
            });
        }
    }
    let bits = [
        ("fps", Some(args.fps)),
        ("duration", args.duration),
        (
            "usb_h264_diagnose_interval",
            Some(args.usb_h264_diagnose_interval),
        ),
        ("route_replay_speed", Some(args.route_replay_speed)),
        ("route_start_time", Some(args.route_start_time)),
        ("profile_interval", Some(args.profile_interval)),
    ]
    .map(|(name, value)| (name, value.map(|value| format!("{:016x}", value.to_bits()))));
    Ok(
        json!({"status":"ok", "args":serde_json::to_value(args)?, "float_bits": bits.into_iter().collect::<std::collections::BTreeMap<_,_>>(), "rates":rates}),
    )
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input)?;
    let request: Request = serde_json::from_str(&input)?;
    let result: Vec<_> = request
        .cases
        .iter()
        .map(|argv| match RuntimeArgs::parse_from(&request.root, argv) {
            Ok(args) => output(&args),
            Err(ParseError::Help(_)) => Ok(json!({"status":"help"})),
            Err(ParseError::Invalid(error)) => Ok(json!({"status":"error","error":error})),
        })
        .collect::<Result<_, Box<dyn std::error::Error>>>()?;
    println!("{}", serde_json::to_string(&result)?);
    Ok(())
}
