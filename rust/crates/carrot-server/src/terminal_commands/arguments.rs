use crate::{tools::shell, Value};

pub(super) fn parse(argv: &[String]) -> Result<Vec<String>, String> {
    let mut args = argv.iter().peekable();
    let mut line = None;
    let mut parts = Vec::new();
    let mut extras = Vec::new();
    while let Some(arg) = args.next() {
        if arg == "--" {
            parts.extend(args.cloned());
            break;
        }
        let (name, inline) = arg
            .split_once('=')
            .map_or((arg.as_str(), None), |(name, value)| (name, Some(value)));
        if name.starts_with("--") && "--line".starts_with(name) {
            line = Some(match inline {
                Some(value) => value,
                None => args
                    .next_if(|value| {
                        !value.starts_with('-') || Value::text(value.as_str()).int().is_ok()
                    })
                    .map(String::as_str)
                    .ok_or_else(|| "argument --line: expected one argument".to_owned())?,
            });
        } else if arg.starts_with('-') && Value::text(arg).int().is_err() && arg != "-" {
            extras.push(arg.as_str());
        } else {
            parts.push(arg.clone());
            parts.extend(args.cloned());
            break;
        }
    }
    if !extras.is_empty() {
        return Err(format!("unrecognized arguments: {}", extras.join(" ")));
    }
    match line {
        Some(line) => shell::split(line)
            .ok_or_else(|| format!("[terminal] parse error: {}", super::parse_error(line))),
        None => Ok(parts),
    }
}
pub(super) fn error(message: &str) {
    if message.starts_with("[terminal]") {
        eprintln!("{message}");
    } else {
        let executable = std::env::args().next().unwrap_or_default();
        let program = std::path::Path::new(&executable)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        eprintln!("usage: {program} [--line LINE] [command] ...\n{program}: error: {message}");
    }
}
