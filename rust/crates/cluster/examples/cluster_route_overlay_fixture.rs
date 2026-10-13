use openpilot_cluster::route_replay::{overlay::data_lines, Frame};
use serde::Deserialize;
use std::io::{self, BufRead};

#[derive(Deserialize)]
struct Query {
    frame: Frame,
    shown_time: f64,
    duration: f64,
    segment_label: String,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    for line in io::stdin().lock().lines() {
        let query: Query = serde_json::from_str(&line?)?;
        let lines = data_lines(
            &query.frame,
            &query.frame.to_state(),
            query.shown_time,
            query.duration,
            &query.segment_label,
        );
        println!("{}", serde_json::to_string(&lines)?);
    }
    Ok(())
}
