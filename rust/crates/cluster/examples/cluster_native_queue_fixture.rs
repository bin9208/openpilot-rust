use openpilot_cluster::h264::native_sender::{select_access_units, AccessUnit, Selection};
use serde::Deserialize;
use std::{
    io::{self, Read},
    path::PathBuf,
};

#[derive(Deserialize)]
struct Case {
    steps: Vec<AccessUnit>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let cases: Vec<Case> = serde_json::from_str(&input)?;
    let output = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .ok_or("OUTPUT required")?;
    let mut rows = Vec::new();
    for case in cases {
        let mut pending = Vec::new();
        let mut steps = Vec::new();
        for incoming in case.steps {
            let selection = if incoming.chunks.is_empty() {
                Selection {
                    kept: pending,
                    dropped_units: 0,
                    dropped_chunks: 0,
                }
            } else {
                select_access_units(pending, incoming)
            };
            steps.push(serde_json::to_value(&selection)?);
            pending = selection.kept;
        }
        rows.push(steps);
    }
    std::fs::write(output, serde_json::to_vec(&rows)?)?;
    Ok(())
}
