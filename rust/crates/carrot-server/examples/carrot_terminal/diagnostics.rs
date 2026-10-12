use openpilot_carrot_server::{http::Application, vision_diag::Config, Value};
use std::path::Path;

pub(super) fn configure(
    app: &mut Application,
    root: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = root.join("diagnostic-config.json");
    if !path.is_file() {
        return Ok(());
    }
    let input = Value::parse(&std::fs::read_to_string(path)?)?;
    let mut config = Config::original(app)?;
    config.proc_root = input.get("proc_root").string()?.into();
    if !config.proc_root.starts_with(root) && config.proc_root != Path::new("/proc") {
        return Err("diagnostic proc input is not owned".into());
    }
    config.params_available = true;
    let Value::Array(matches) = input.get("process_matches") else {
        return Err("process matches absent".into());
    };
    if matches.len() != config.process_matches.len() {
        return Err("process matches incomplete".into());
    }
    for ((name, pattern), item) in config.process_matches.iter_mut().zip(matches) {
        let Value::Array(item) = item else {
            return Err("process match is not a pair".into());
        };
        if item.len() != 2 || item[0].string()? != *name {
            return Err("process match name differs".into());
        }
        *pattern = item[1].string()?;
    }
    app.vision_diag = Some(config);
    Ok(())
}
