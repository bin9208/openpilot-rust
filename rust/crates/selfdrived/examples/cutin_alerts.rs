use openpilot_selfdrived::cutin::{promoted, Candidate, Tracker};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::io::{self, BufRead, Write};

#[derive(Deserialize, Serialize)]
struct WireCandidate {
    track_id: i32,
    bits: [u64; 3],
}

impl From<WireCandidate> for Candidate {
    fn from(value: WireCandidate) -> Self {
        let [d_rel, y_rel, v_rel] = value.bits.map(f64::from_bits);
        Self {
            track_id: value.track_id,
            d_rel,
            y_rel,
            v_rel,
        }
    }
}

impl From<Candidate> for WireCandidate {
    fn from(value: Candidate) -> Self {
        Self {
            track_id: value.track_id,
            bits: [value.d_rel, value.y_rel, value.v_rel].map(f64::to_bits),
        }
    }
}

#[derive(Deserialize)]
struct Request {
    candidates: Vec<WireCandidate>,
    lead_two: Option<WireCandidate>,
    enabled: bool,
    reset: bool,
    promote: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut tracker = Tracker::default();
    let mut output = io::BufWriter::new(io::stdout().lock());
    for line in io::stdin().lock().lines() {
        let request: Request = serde_json::from_str(&line?)?;
        if request.reset {
            tracker.reset();
        }
        let candidates: Vec<_> = request
            .candidates
            .into_iter()
            .map(Candidate::from)
            .collect();
        let selected = if request.promote {
            promoted(&candidates, request.lead_two.map(Candidate::from))
        } else {
            candidates
        };
        let alert = tracker.update(&selected, request.enabled);
        let selected: Vec<_> = selected.into_iter().map(WireCandidate::from).collect();
        let previous: Vec<_> = tracker
            .previous()
            .iter()
            .copied()
            .map(WireCandidate::from)
            .collect();
        serde_json::to_writer(
            &mut output,
            &json!({"alert":alert,"selected":selected,"previous":previous}),
        )?;
        writeln!(output)?;
    }
    Ok(())
}
