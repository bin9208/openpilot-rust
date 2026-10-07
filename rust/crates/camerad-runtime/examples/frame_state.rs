use openpilot_camerad::{
    exposure::{CameraId, ManualExposure},
    geometry::Geometry,
    requests::FrameMetadata,
    sensor::SensorKind,
};
use openpilot_camerad_runtime::FrameState;
use serde::Deserialize;
use std::{
    error::Error,
    io::{self, BufRead, Write},
};

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Input {
    Reset {
        sensor: u32,
        camera: u32,
        width: i32,
        height: i32,
        focal: f32,
    },
    Step {
        frame_id: u32,
        request_id: u32,
        sof: u64,
        eof: u64,
        processing: f32,
        log_time: u64,
        seed: u8,
        pattern: bool,
        log_raw: bool,
        enabled: bool,
        gain: String,
        time: String,
    },
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut output = io::BufWriter::new(io::stdout().lock());
    let mut state = None;
    let mut pixels = Vec::new();
    let mut width = 0;
    for line in io::stdin().lock().lines() {
        match serde_json::from_str::<Input>(&line?)? {
            Input::Reset {
                sensor,
                camera,
                width: w,
                height,
                focal,
            } => {
                let sensor = match sensor {
                    1 => SensorKind::Ar0231,
                    2 => SensorKind::Ox03c10,
                    3 => SensorKind::Os04c10,
                    _ => return Err("sensor".into()),
                };
                let camera = match camera {
                    0 => CameraId::Wide,
                    1 => CameraId::Road,
                    2 => CameraId::Driver,
                    _ => return Err("camera".into()),
                };
                state = Some(FrameState::new(
                    sensor,
                    camera,
                    Geometry {
                        width: w,
                        height,
                        focal_mm: focal,
                    },
                )?);
                width = usize::try_from(w)?;
                pixels = vec![0; width * usize::try_from(height)?];
            }
            Input::Step {
                frame_id,
                request_id,
                sof,
                eof,
                processing,
                log_time,
                seed,
                pattern,
                log_raw,
                enabled,
                gain,
                time,
            } => {
                let state = state.as_mut().ok_or("missing reset")?;
                for (index, pixel) in pixels.iter_mut().enumerate() {
                    *pixel = if pattern {
                        ((index / width + index % width * 3 + usize::from(seed)) % 256) as u8
                    } else {
                        seed
                    };
                }
                let raw: Vec<u8> = (0..32u8).map(|value| value.wrapping_add(seed)).collect();
                let metadata = FrameMetadata {
                    slot: 0,
                    frame_id,
                    request_id,
                    timestamp_sof: sof,
                    timestamp_eof: eof,
                    processing_time: processing,
                };
                let mut actions = vec!["vision"];
                let wire = state.encode(metadata, log_time, log_raw, Some(&raw))?;
                let registers = match state.adjust(
                    frame_id,
                    &pixels,
                    enabled,
                    ManualExposure {
                        gain: &gain,
                        time: &time,
                    },
                ) {
                    Ok(registers) => registers,
                    Err(error) => {
                        serde_json::to_writer(
                            &mut output,
                            &serde_json::json!({"error":error.to_string(), "actions":actions}),
                        )?;
                        writeln!(output)?;
                        continue;
                    }
                };
                if registers.is_some() {
                    actions.push("registers");
                }
                actions.push("publish");
                let mut snapshot = serde_json::to_value(state.exposure())?;
                let region = state.region();
                snapshot["rect"] =
                    serde_json::json!([region.x, region.y, region.width, region.height]);
                snapshot["writes"] =
                    serde_json::json!(registers.as_ref().map_or(&[][..], |value| value.as_slice()));
                serde_json::to_writer(
                    &mut output,
                    &serde_json::json!({"wire":wire,"state":snapshot,"actions":actions}),
                )?;
                writeln!(output)?;
            }
        }
    }
    output.flush()?;
    Ok(())
}
