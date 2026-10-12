use openpilot_cluster::{
    route_replay::{read_log_bytes, Parser},
    runtime_args::CornerSource,
};
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Deserialize)]
struct Query {
    paths: Vec<PathBuf>,
    mode: String,
    #[serde(default)]
    corner: Option<String>,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let query: Query = serde_json::from_reader(std::io::stdin().lock())?;
    if query.mode == "bytes" {
        let results: Vec<_> = query
            .paths
            .iter()
            .map(|path| read_log_bytes(path).map_err(|error| error.to_string()))
            .collect();
        println!("{}", serde_json::to_string(&results)?);
    } else {
        let mut parser = Parser::new(match query.corner.as_deref() {
            Some("stable") => CornerSource::Stable,
            Some("raw") => CornerSource::Raw,
            _ => CornerSource::Live,
        });
        let frames: Result<Vec<_>, _> = query
            .paths
            .iter()
            .map(|path| parser.parse_file(path))
            .collect();
        println!("{}", serde_json::to_string(&frames?)?);
    }
    Ok(())
}
