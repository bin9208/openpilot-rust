use openpilot_cluster::{models::ClusterUiState, navi_simulator::merge_overlay_state};
use serde::Deserialize;
use std::io::{self, Read};

#[derive(Deserialize)]
struct Query {
    base: ClusterUiState,
    overlay: ClusterUiState,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut text = String::new();
    io::stdin().read_to_string(&mut text)?;
    let queries: Vec<Query> = serde_json::from_str(&text)?;
    let output: Vec<_> = queries
        .iter()
        .map(|query| merge_overlay_state(&query.base, &query.overlay))
        .collect();
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}
