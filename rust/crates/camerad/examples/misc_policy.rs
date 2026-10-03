use std::error::Error;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;

use openpilot_camerad::{
    cdm,
    geometry::{luminance, Region, Sampling},
    nv12::Nv12Layout,
    timing::CameraEventTiming,
};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Input {
    Timing {
        sof: u64,
        received: u64,
    },
    Nv12 {
        width: u32,
        height: u32,
    },
    Dmi {
        length: u32,
        address: u32,
        selector: u8,
        opcode: u8,
    },
    Cont {
        address: u32,
        values: Vec<u32>,
    },
    Random {
        values: Vec<u32>,
    },
    Luminance {
        width: usize,
        region: [i32; 4],
        x_skip: usize,
        y_skip: usize,
        file: PathBuf,
    },
}

#[derive(Serialize)]
struct Packed<'a> {
    bytes: &'a [u8],
    patch: i32,
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut output = io::BufWriter::new(io::stdout().lock());
    let mut timing = CameraEventTiming::default();
    for line in io::stdin().lock().lines() {
        let input: Input = serde_json::from_str(&line?)?;
        match input {
            Input::Timing { sof, received } => {
                serde_json::to_writer(&mut output, &timing.observe(sof, received))?
            }
            Input::Nv12 { width, height } => {
                serde_json::to_writer(&mut output, &Nv12Layout::new(width, height)?)?
            }
            Input::Dmi {
                length,
                address,
                selector,
                opcode,
            } => {
                let mut bytes = [0_u8; 12];
                let patch = i32::try_from(cdm::write_dmi(
                    &mut bytes,
                    cdm::Dmi {
                        length,
                        address,
                        selector,
                        opcode,
                    },
                )?)?;
                serde_json::to_writer(
                    &mut output,
                    &Packed {
                        bytes: &bytes,
                        patch,
                    },
                )?;
            }
            Input::Cont { address, values } => {
                let mut bytes = vec![0xcc; values.len() * 4 + 8];
                let size = cdm::write_cont(&mut bytes, address, &values)?;
                serde_json::to_writer(
                    &mut output,
                    &Packed {
                        bytes: &bytes[..size],
                        patch: -1,
                    },
                )?;
            }
            Input::Random { values } => {
                let mut bytes = vec![0xcc; values.len() * 4 + 4];
                let size = cdm::write_random(&mut bytes, &values)?;
                serde_json::to_writer(
                    &mut output,
                    &Packed {
                        bytes: &bytes[..size],
                        patch: -1,
                    },
                )?;
            }
            Input::Luminance {
                width,
                region: [x, y, width_region, height],
                x_skip,
                y_skip,
                file,
            } => {
                let pixels = std::fs::read(file)?;
                let region = Region {
                    x,
                    y,
                    width: width_region,
                    height,
                };
                serde_json::to_writer(
                    &mut output,
                    &luminance(
                        &pixels,
                        region,
                        Sampling {
                            width,
                            x_skip,
                            y_skip,
                        },
                    )?,
                )?;
            }
        }
        writeln!(output)?;
    }
    output.flush()?;
    Ok(())
}
