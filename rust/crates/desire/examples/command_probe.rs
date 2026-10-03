use openpilot_desire::command::CommandReader;
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::BTreeMap,
    error::Error,
    fs,
    io::{BufRead, BufReader, BufWriter, Write},
    path::PathBuf,
};

#[derive(Deserialize)]
struct Step {
    now: f64,
    allowed: bool,
    writes: BTreeMap<String, Value>,
    #[serde(default)]
    raw_writes: BTreeMap<String, String>,
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args_os().skip(1);
    let request = args.next().ok_or("expected request.jsonl")?;
    let output = args.next().ok_or("expected output.jsonl")?;
    let root = PathBuf::from(args.next().ok_or("expected new journal directory")?);
    if args.next().is_some() {
        return Err("unexpected arguments".into());
    }
    fs::create_dir(&root)?;
    let mut reader = CommandReader::new(&root, "lane", 10.0);
    let mut output = BufWriter::new(fs::File::create(output)?);
    for line in BufReader::new(fs::File::open(request)?).lines() {
        let step: Step = serde_json::from_str(&line?)?;
        for (name, value) in step.writes {
            if !matches!(name.as_str(), "lane" | "learn" | "cancelled") {
                return Err("unknown journal file".into());
            }
            fs::write(
                root.join(format!("{name}.json")),
                serde_json::to_vec(&value)?,
            )?;
        }
        for (name, value) in step.raw_writes {
            if !matches!(name.as_str(), "lane" | "learn" | "cancelled") {
                return Err("unknown raw journal file".into());
            }
            fs::write(root.join(format!("{name}.json")), value)?;
        }
        let action = reader.read(step.allowed, step.now);
        writeln!(
            output,
            "{{\"action\":{},\"last_id\":{},\"repeat\":{}}}",
            serde_json::to_string(&action)?,
            reader
                .last_id
                .as_ref()
                .map_or_else(|| Ok("null".to_owned()), |id| id.to_json())?,
            reader.is_repeat
        )?;
    }
    output.flush()?;
    Ok(())
}
