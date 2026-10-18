use openpilot_signal_observer::{
    lead_brake::{Context, Detector},
    pixels::Rgb,
};
use serde_json::Value;
use std::io::{self, BufRead};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    openpilot_opencv_runtime::initialize(1)?;
    let mut detector = Detector::default();
    for line in io::stdin().lock().lines() {
        let row: Value = serde_json::from_str(&line?)?;
        if row["reset"] == true {
            detector.reset();
        }
        let context: Context = serde_json::from_value(row["context"].clone())?;
        let mut rgb = Rgb {
            width: 1344,
            height: 760,
            data: vec![0; 1344 * 760 * 3],
        };
        for rect in row["lamps"].as_array().ok_or("lamps")? {
            let r: Vec<usize> = serde_json::from_value(rect.clone())?;
            for y in r[1]..r[3] {
                for x in r[0]..r[2] {
                    rgb.data[(y * 1344 + x) * 3] = 255;
                }
            }
        }
        println!(
            "{}",
            detector.process(&rgb, row["stamp"].as_f64().ok_or("stamp")?, &context)?
        );
    }
    Ok(())
}
