use openpilot_plannerd::signal::{
    prepare::{Geometry, Input, Prepare},
    priority::Candidate,
};
use serde::Deserialize;
use std::io::{self, Read};
#[derive(Deserialize)]
struct Frame {
    #[serde(flatten)]
    input: Input,
    night: bool,
    range: [f64; 2],
    red_alignment: [f64; 2],
    green_alignment: [f64; 2],
}
struct Callbacks<'a>(&'a Frame);
impl Geometry for Callbacks<'_> {
    fn night(&self) -> bool {
        self.0.night
    }
    fn range(&mut self, _: [f64; 4], _: bool) -> Option<[f64; 2]> {
        Some(self.0.range)
    }
    fn alignment(&mut self, c: &Candidate) -> Option<[f64; 2]> {
        Some(if c.bounds[0] > 900. {
            self.0.green_alignment
        } else {
            self.0.red_alignment
        })
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut text = String::new();
    io::stdin().read_to_string(&mut text)?;
    let cases: Vec<Vec<Frame>> = serde_json::from_str(&text)?;
    let values: Vec<Vec<_>> = cases
        .iter()
        .map(|frames| {
            let mut prepare = Prepare::default();
            frames
                .iter()
                .map(|f| {
                    let decision = prepare.update(&f.input, &mut Callbacks(f));
                    serde_json::json!({"decision":decision,"state":prepare.snapshot()})
                })
                .collect()
        })
        .collect();
    println!("{}", serde_json::to_string(&values)?);
    Ok(())
}
