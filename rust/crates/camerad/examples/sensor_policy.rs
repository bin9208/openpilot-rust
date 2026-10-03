use std::error::Error;
use std::io::{self, BufRead, Write};
use std::str::{FromStr, SplitWhitespace};

use openpilot_camerad::sensor::{Exposure, ExposureScore, SensorKind};

fn next<T: FromStr>(words: &mut SplitWhitespace<'_>) -> Result<T, Box<dyn Error>>
where
    T::Err: Error + 'static,
{
    Ok(words.next().ok_or("missing input")?.parse()?)
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut output = io::BufWriter::new(io::stdout().lock());
    for line in io::stdin().lock().lines() {
        let line = line?;
        let mut words = line.split_whitespace();
        let operation = words.next().ok_or("missing operation")?;
        let sensor = match next::<u32>(&mut words)? {
            1 => SensorKind::Ar0231,
            2 => SensorKind::Ox03c10,
            3 => SensorKind::Os04c10,
            _ => return Err("unknown sensor".into()),
        };
        match operation {
            "config" => serde_json::to_writer(&mut output, sensor.config())?,
            "address" => {
                serde_json::to_writer(&mut output, &sensor.slave_address(next(&mut words)?)?)?
            }
            "exposure" => {
                let registers = sensor.exposure_registers(Exposure {
                    time: next(&mut words)?,
                    gain_index: next(&mut words)?,
                    dc_gain: next::<u32>(&mut words)? != 0,
                })?;
                serde_json::to_writer(&mut output, registers.as_slice())?;
            }
            "score" => {
                let score = sensor.exposure_score(ExposureScore {
                    desired_ev: next(&mut words)?,
                    time: next(&mut words)?,
                    gain_index: next(&mut words)?,
                    gain: next(&mut words)?,
                    previous_gain_index: next(&mut words)?,
                });
                serde_json::to_writer(&mut output, &score)?;
            }
            _ => return Err("unknown operation".into()),
        }
        if words.next().is_some() {
            return Err("extra input".into());
        }
        writeln!(output)?;
    }
    output.flush()?;
    Ok(())
}
