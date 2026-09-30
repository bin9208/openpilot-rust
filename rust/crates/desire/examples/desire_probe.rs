use openpilot_desire::{
    helper::DesireHelper,
    types::{Config, Input},
};
use serde::Deserialize;
use serde_json::json;
use std::{
    error::Error,
    fs::File,
    io::{BufRead, BufReader, BufWriter, Write},
};

#[derive(Deserialize)]
struct Frame {
    reset: bool,
    input: Input,
    config: Config,
    remote: Option<String>,
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args_os().skip(1);
    let request = args.next().ok_or("expected request.jsonl")?;
    let output = args.next().ok_or("expected output.jsonl")?;
    if args.next().is_some() {
        return Err("unexpected arguments".into());
    }
    let mut helper = DesireHelper::default();
    let mut output = BufWriter::new(File::create(output)?);
    for line in BufReader::new(File::open(request)?).lines() {
        let frame: Frame = serde_json::from_str(&line?)?;
        if frame.reset {
            helper = DesireHelper::default();
        }
        let mut refreshed = false;
        let mut allowed = false;
        helper.update(
            &frame.input,
            || {
                refreshed = true;
                frame.config.clone()
            },
            |permitted| {
                allowed = permitted;
                if permitted {
                    frame.remote.clone()
                } else {
                    None
                }
            },
        )?;
        serde_json::to_writer(
            &mut output,
            &json!({"state":helper,"remote_allowed":allowed,"params_refreshed":refreshed}),
        )?;
        writeln!(output)?;
    }
    output.flush()?;
    Ok(())
}
