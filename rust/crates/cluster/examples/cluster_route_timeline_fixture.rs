use openpilot_cluster::route_replay::{timeline::Timeline, Frame};
use serde::Deserialize;
use std::{
    io::{self, BufRead},
    path::PathBuf,
};

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Query {
    Append {
        index: usize,
        path: PathBuf,
        frames: Vec<Frame>,
    },
    Sample {
        trim: f64,
        times: Vec<f64>,
    },
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut timeline = Timeline::default();
    for line in io::stdin().lock().lines() {
        match serde_json::from_str(&line?)? {
            Query::Append {
                index,
                path,
                frames,
            } => {
                timeline.append(index, path, frames);
            }
            Query::Sample { trim, times } => {
                timeline.trim(trim);
                let samples: Vec<_> = times.into_iter().map(|time|serde_json::json!({
                    "state": timeline.playback_frame(time).map(|frame|frame.to_state()),
                    "status": timeline.status_frame(time), "path":timeline.log_path(time), "offset":timeline.log_offset(time),
                })).collect();
                println!(
                    "{}",
                    serde_json::to_string(&serde_json::json!({"chunks":timeline.chunk_count(),
                    "duration":timeline.duration(), "first_time":timeline.first_time(), "samples":samples}))?
                );
            }
        }
    }
    Ok(())
}
