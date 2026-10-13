use base64::{engine::general_purpose::STANDARD, Engine};
use openpilot_cluster::{
    renderer::nv12::Nv12Layout,
    runtime::output::{
        build_nv12_color_test_pattern, build_rgba_color_test_pattern, parse_native_bitrate,
    },
};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Request {
    Rgba { width: i32, height: i32 },
    Nv12 { layout: Nv12Layout },
    Bitrate { text: String },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let requests: Vec<Request> = serde_json::from_reader(std::io::stdin().lock())?;
    let responses: Vec<_> = requests
        .into_iter()
        .map(|request| match request {
            Request::Rgba { width, height } => build_rgba_color_test_pattern(width, height)
                .map(|bytes| serde_json::json!({"bytes": STANDARD.encode(bytes)})),
            Request::Nv12 { layout } => build_nv12_color_test_pattern(layout)
                .map(|bytes| serde_json::json!({"bytes": STANDARD.encode(bytes)})),
            Request::Bitrate { text } => {
                parse_native_bitrate(&text).map(|value| serde_json::json!({"value": value}))
            }
        })
        .map(|result| result.unwrap_or_else(|_| serde_json::json!({"error": true})))
        .collect();
    println!("{}", serde_json::to_string(&responses)?);
    Ok(())
}
