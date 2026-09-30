use openpilot_dashcam_upload::{catalog, report, Error};
use serde_json::{json, Value};
use std::{
    io::{self, Read},
    path::Path,
};
fn observed<T: serde::Serialize>(result: Result<T, Error>) -> Value {
    match result {
        Ok(value) => json!({"value":value}),
        Err(Error::Http { status, text }) => json!({"status":status,"error":text}),
        Err(error) => json!({"error":error.to_string()}),
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let input: Value = serde_json::from_str(&input)?;
    let mode = input["mode"].as_str().ok_or("mode missing")?;
    let output = match mode {
        "paths" => {
            let names: Vec<String> = serde_json::from_value(input["names"].clone())?;
            json!(names.iter().map(|name|json!({"safe":observed(catalog::safe_segment(name)),"index":catalog::segment_index(name),"route":catalog::route_name(name)})).collect::<Vec<_>>())
        }
        "files" => {
            let root = Path::new(input["root"].as_str().ok_or("root missing")?);
            let names: Vec<String> = serde_json::from_value(input["names"].clone())?;
            json!(names.iter().map(|name|json!({"directory":observed(catalog::segment_dir(root,name)),"complete":catalog::segment_complete(root,name),"files":observed(catalog::file_summary(&root.join(name)))})).collect::<Vec<_>>())
        }
        "reports" => json!(input["payloads"].as_array().ok_or("payloads missing")?.iter().map(|payload|json!({"share":report::share_text(payload),"discord":report::discord_content(payload)})).collect::<Vec<_>>()),
        "urls" => json!(input["urls"].as_array().ok_or("urls missing")?.iter().map(|url|report::public_url(url.as_str().unwrap_or(""))).collect::<Vec<_>>()),
        _ => return Err("invalid mode".into()),
    };
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}
