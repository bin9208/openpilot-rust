//! Native local analysis entry point; no vehicle process or parameter writes.
use openpilot_radard::{
    controller::{Input, Options},
    replay::{identity, Inversion, Replay},
};
use serde_json::{json, Value};
use std::{
    env, fs,
    io::{self, BufRead, Write},
    path::Path,
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
fn embedded_identity() -> Value {
    json!({"protocol": identity::PROTOCOL, "upstream": identity::UPSTREAM, "source_fingerprint": env!("RADAR_REPLAY_SOURCE"), "source_revision": env!("RADAR_REPLAY_REVISION")})
}
fn prepared(path: &Path, choice: Inversion) -> Result<()> {
    let reader = io::BufReader::new(fs::File::open(path)?);
    let mut lines = reader.lines();
    let header: Value = serde_json::from_str(&lines.next().ok_or("missing replay header")??)?;
    let options: Options = serde_json::from_value(
        header
            .get("options")
            .cloned()
            .ok_or("header requires options")?,
    )?;
    let mut replay = Replay::new(options, choice);
    let mut output = io::BufWriter::new(io::stdout().lock());
    write_receipt(
        &mut output,
        json!({"identity": embedded_identity(), "input_sha256": identity::file_hash(path)?, "inversion": choice, "binary_sha256": identity::file_hash(&env::current_exe()?)?}),
    )?;
    for (index, line) in lines.enumerate() {
        let frame: Value = serde_json::from_str(&line?)?;
        let input: Input =
            serde_json::from_value(frame.get("input").cloned().ok_or("frame requires input")?)?;
        let flipped = frame
            .get("recorded_flipped")
            .and_then(Value::as_bool)
            .ok_or("frame requires recorded_flipped")?;
        writeln!(
            output,
            "{}",
            json!({"frame": index, "output": replay.update(input, flipped)?})
        )?;
    }
    output.flush()?;
    Ok(())
}
fn write_receipt(output: &mut impl Write, mut value: Value) -> Result<()> {
    value["cache_key"] = json!(identity::cache_key(&value)?);
    writeln!(output, "{value}")?;
    Ok(())
}
fn main() -> Result<()> {
    let args: Vec<_> = env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["identity"] => println!("{}", embedded_identity()),
        ["bundle", root, destination] => openpilot_radard::replay::package::bundle(Path::new(root), Path::new(destination), embedded_identity(), None)?,
        ["bundle", root, destination, numerics] => openpilot_radard::replay::package::bundle(Path::new(root), Path::new(destination), embedded_identity(), Some(Path::new(numerics)))?,
        ["stage-dbcs", source] => println!("{}", json!({"added":openpilot_radard::replay::package::stage_dbcs(Path::new(source))?})),
        ["dbc-assets", root, destination] => openpilot_radard::replay::package::dbcs(Path::new(root), Path::new(destination))?,
        ["raw", path, config] => {
            let config_path = Path::new(config);
            let config: Value = serde_json::from_reader(fs::File::open(config_path)?)?;
            let required = |name| -> Result<std::path::PathBuf> {
                let path = Path::new(config.get(name).and_then(Value::as_str).ok_or("raw configuration missing path")?);
                Ok(if path.is_absolute() { path.to_path_buf() } else { config_path.parent().unwrap_or(Path::new(".")).join(path) })
            };
            let cp = fs::read(required("car_params")?)?;
            let settings = serde_json::from_value(config.get("parameters").cloned().ok_or("raw configuration requires parameters")?)?;
            let options = serde_json::from_value(config.get("options").cloned().ok_or("raw configuration requires options")?)?;
            let flipped = config.get("radar_track_flip").and_then(Value::as_bool).ok_or("raw configuration requires radar_track_flip")?;
            let choice: Inversion = serde_json::from_value(config.get("inversion").cloned().unwrap_or(json!("recorded")))?;
            let mut raw = openpilot_radard::replay::raw::Raw::new(&cp, &required("dbc")?, &required("numerics")?, settings, flipped)?;
            let mut output = io::BufWriter::new(io::stdout().lock());
            write_receipt(&mut output, json!({"identity": embedded_identity(), "input_sha256": identity::file_hash(Path::new(path))?, "config_sha256": identity::file_hash(config_path)?, "binary_sha256": identity::file_hash(&env::current_exe()?)?, "car_params_sha256": identity::file_hash(&required("car_params")?)?, "dbc_sha256": identity::directory_hash(&required("dbc")?)?, "numerics_sha256": identity::directory_hash(&required("numerics")?)?}))?;
            openpilot_radard::replay::log::run(Path::new(path), options, choice, &mut output, Some(&mut raw))?;
            output.flush()?;
        }
        ["recorded", path, options_path] => {
            let options: Options = serde_json::from_reader(fs::File::open(options_path)?)?;
            let mut output = io::BufWriter::new(io::stdout().lock());
            write_receipt(&mut output, json!({"identity": embedded_identity(), "input_sha256": identity::file_hash(Path::new(path))?, "options_sha256": identity::file_hash(Path::new(options_path))?, "binary_sha256": identity::file_hash(&env::current_exe()?)?}))?;
            openpilot_radard::replay::log::run(Path::new(path), options, Inversion::Recorded, &mut output, None)?;
            output.flush()?;
        }
        ["prepared", path] => prepared(Path::new(path), Inversion::Recorded)?,
        ["prepared", path, mode] => prepared(Path::new(path), match *mode {"recorded" => Inversion::Recorded, "normal" => Inversion::Normal, "flipped" => Inversion::Flipped, _ => return Err("inversion must be recorded, normal or flipped".into())})?,
        _ => return Err("usage: radar-replay identity | stage-dbcs DBC_SOURCE | dbc-assets DBC_SOURCE NEW_DIRECTORY | bundle ROOT NEW_DIRECTORY [NUMERICS] | prepared FRAMES.jsonl [recorded|normal|flipped] | recorded RLOG OPTIONS.json | raw RLOG CONFIG.json".into()),
    }
    Ok(())
}
