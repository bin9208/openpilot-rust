use openpilot_micd::analysis::{raw_audio, Analyzer};
use serde::Deserialize;
use std::io::{self, Read};

#[derive(Deserialize)]
struct Request {
    callbacks: Vec<Vec<u32>>,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let request: Request = serde_json::from_str(&input)?;
    let mut analyzer = Analyzer::default();
    let mut rows = Vec::new();
    let mut bytes = Vec::new();
    for bits in request.callbacks {
        let samples: Vec<_> = bits.into_iter().map(f32::from_bits).collect();
        raw_audio(&samples, &mut bytes);
        analyzer.append(&samples);
        let pressure = analyzer.pressure();
        rows.push(serde_json::json!({
            "raw": bytes,
            "pending": analyzer.pending_len(),
            "pressure_bits": [pressure.unweighted.to_bits(), pressure.weighted.to_bits(), pressure.weighted_db.to_bits()],
        }));
    }
    println!("{}", serde_json::to_string(&rows)?);
    Ok(())
}
