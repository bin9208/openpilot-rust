use std::error::Error;
use std::io::{self, BufRead, Write};

use openpilot_camerad::exposure::{CameraId, ExposureState, FrameMeasurement, ManualExposure};
use openpilot_camerad::geometry::Geometry;
use openpilot_camerad::sensor::{Register, SensorKind};
use serde::{Deserialize, Serialize};

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
        grey: f32,
        enabled: bool,
        gain: String,
        time: String,
    },
}

#[derive(Serialize)]
struct Output<'a> {
    #[serde(flatten)]
    state: &'a ExposureState,
    rect: [i32; 4],
    writes: &'a [Register],
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut output = io::BufWriter::new(io::stdout().lock());
    let mut state = ExposureState::new(SensorKind::Ar0231, CameraId::Wide);
    let mut rect = [0; 4];
    for line in io::stdin().lock().lines() {
        let input: Input = serde_json::from_str(&line?)?;
        let registers = match input {
            Input::Reset {
                sensor,
                camera,
                width,
                height,
                focal,
            } => {
                let sensor = match sensor {
                    1 => SensorKind::Ar0231,
                    2 => SensorKind::Ox03c10,
                    3 => SensorKind::Os04c10,
                    _ => return Err("unknown sensor".into()),
                };
                let camera = match camera {
                    0 => CameraId::Wide,
                    1 => CameraId::Road,
                    2 => CameraId::Driver,
                    _ => return Err("unknown camera".into()),
                };
                state = ExposureState::new(sensor, camera);
                let region = Geometry {
                    width,
                    height,
                    focal_mm: focal,
                }
                .exposure_region(sensor, camera);
                rect = [region.x, region.y, region.width, region.height];
                None
            }
            Input::Step {
                frame_id,
                grey,
                enabled,
                gain,
                time,
            } => {
                match state.update(
                    FrameMeasurement {
                        frame_id,
                        grey,
                        enabled,
                    },
                    ManualExposure {
                        gain: &gain,
                        time: &time,
                    },
                ) {
                    Ok(registers) => registers,
                    Err(error) => {
                        serde_json::to_writer(
                            &mut output,
                            &serde_json::json!({"error": error.to_string()}),
                        )?;
                        writeln!(output)?;
                        continue;
                    }
                }
            }
        };
        let writes = registers
            .as_ref()
            .map_or(&[][..], |values| values.as_slice());
        serde_json::to_writer(
            &mut output,
            &Output {
                state: &state,
                rect,
                writes,
            },
        )?;
        writeln!(output)?;
    }
    output.flush()?;
    Ok(())
}
