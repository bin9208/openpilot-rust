use openpilot_cluster::route_replay::paths;
use serde::{Deserialize, Serialize};
use std::{
    io::{self, BufRead},
    path::PathBuf,
};

#[derive(Deserialize)]
struct Query {
    path: PathBuf,
    #[serde(default = "default_kind")]
    kind: String,
    start: Option<i64>,
    maximum: Option<i64>,
    direction: Option<i64>,
    #[serde(default)]
    names: Vec<String>,
}
fn default_kind() -> String {
    "rlog".into()
}
#[derive(Serialize)]
struct ResultRow {
    discovered: Result<Vec<PathBuf>, String>,
    adjacent: Option<Result<Option<PathBuf>, String>>,
    indexes: Vec<Option<i64>>,
    routes: Vec<Option<String>>,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    for line in io::stdin().lock().lines() {
        let query: Query = serde_json::from_str(&line?)?;
        let output = ResultRow {
            discovered: paths::discover_route_logs(
                &query.path,
                &query.kind,
                query.start,
                query.maximum,
            )
            .map_err(|error| error.to_string()),
            adjacent: query.direction.map(|direction| {
                paths::adjacent_route_log_path(&query.path, direction, &query.kind)
                    .map_err(|error| error.to_string())
            }),
            indexes: query
                .names
                .iter()
                .map(|name| paths::segment_index_from_name(name))
                .collect(),
            routes: query
                .names
                .iter()
                .map(|name| paths::segment_route_id_from_name(name).map(str::to_owned))
                .collect(),
        };
        println!("{}", serde_json::to_string(&output)?);
    }
    Ok(())
}
