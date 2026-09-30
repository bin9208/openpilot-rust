use openpilot_logmessaged::{format_record, LogFiles, RotationSettings};
use serde::Deserialize;
use std::{
    cell::Cell,
    error::Error,
    fs::File,
    io::{BufRead, BufReader, BufWriter, Write},
    path::PathBuf,
};

#[derive(Deserialize)]
struct Step {
    at: f64,
    record: String,
}
#[derive(Deserialize)]
struct Sequence {
    directory: PathBuf,
    interval: f64,
    max_bytes: u64,
    backup_count: usize,
    steps: Vec<Step>,
    clock_reads: Option<Vec<f64>>,
}
fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 3 {
        return Err("file_probe format|rotate INPUT OUTPUT".into());
    }
    let input = BufReader::new(File::open(&args[1])?);
    let mut output = BufWriter::new(File::create(&args[2])?);
    match args[0].as_str() {
        "format" => {
            for line in input.lines() {
                let record: String = serde_json::from_str(&line?)?;
                match format_record(&record, uuid::Uuid::nil()) {
                    Ok(value) => {
                        serde_json::to_writer(&mut output, &serde_json::json!({"record": value}))?
                    }
                    Err(_) => {
                        serde_json::to_writer(&mut output, &serde_json::json!({"error": true}))?
                    }
                }
                output.write_all(b"\n")?;
            }
        }
        "rotate" => {
            let sequence: Sequence = serde_json::from_reader(input)?;
            let settings = RotationSettings {
                interval: sequence.interval,
                max_bytes: sequence.max_bytes,
                backup_count: sequence.backup_count,
            };
            let at = Cell::new(0.);
            let exhausted = Cell::new(false);
            let mut reads = sequence.clock_reads.map(|values| values.into_iter());
            let clock = || match &mut reads {
                None => at.get(),
                Some(reads) => reads.next().unwrap_or_else(|| {
                    exhausted.set(true);
                    0.
                }),
            };
            match LogFiles::new(&sequence.directory.join("swaglog"), settings, clock) {
                Err(error) => serde_json::to_writer(
                    &mut output,
                    &serde_json::json!({"initialization_error": error.to_string()}),
                )?,
                Ok(mut handler) => {
                    for step in sequence.steps {
                        at.set(step.at);
                        serde_json::to_writer(
                            &mut output,
                            &serde_json::json!({"emitted": handler.emit(&step.record).is_ok()}),
                        )?;
                        output.write_all(b"\n")?;
                    }
                    serde_json::to_writer(
                        &mut output,
                        &serde_json::json!({"closed": handler.close().is_ok()}),
                    )?;
                    output.write_all(b"\n")?;
                }
            }
            if exhausted.get() {
                return Err("clock fixture exhausted".into());
            }
        }
        _ => return Err("unknown probe mode".into()),
    }
    output.flush()?;
    Ok(())
}
