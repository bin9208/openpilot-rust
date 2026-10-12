use openpilot_cluster::route_replay::Frame;
use serde::Deserialize;
use std::io::{self, BufRead};

#[derive(Deserialize)]
struct Query {
    left: Frame,
    right: Frame,
    amount: f64,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    for line in io::stdin().lock().lines() {
        let query: Query = serde_json::from_str(&line?)?;
        let blended = query.left.blend(&query.right, query.amount);
        println!(
            "{}",
            serde_json::to_string(&serde_json::json!({
                "left":query.left.to_state(), "right":query.right.to_state(),
                "blend":blended.to_state(), "t":blended.t,
                "curvature":blended.metadata.road_curvature,
            }))?
        );
    }
    Ok(())
}
