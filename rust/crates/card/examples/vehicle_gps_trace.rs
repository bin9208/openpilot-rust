use openpilot_card::brands::hyundai::vehicle_gps::VehicleGps;
use serde_json::{json, Value};
use std::io::{self, BufRead};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut gps = VehicleGps::default();
    for line in io::stdin().lock().lines() {
        let v: Value = serde_json::from_str(&line?)?;
        if v["reset"] == true {
            gps = VehicleGps::default();
        }
        let raw: Vec<u8> = serde_json::from_value(v["raw"].clone())?;
        let fix = gps.update(
            v["now"].as_u64().ok_or("now")?,
            v["stamp"].as_u64().ok_or("stamp")?,
            v["timeout"] == true,
            &raw,
            v["speed"].as_f64().unwrap_or(f64::NAN),
            v["reverse"] == true,
        );
        println!(
            "{}",
            json!({"available":fix.available,"valid":fix.valid,"latitude":fix.latitude,"longitude":fix.longitude,"sampleMonoTime":fix.sample_mono_time,"headingDeg":fix.heading_deg,"headingValid":fix.heading_valid})
        );
    }
    Ok(())
}
