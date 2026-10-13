use crate::Value;
use num_traits::ToPrimitive;

pub(super) enum Action {
    Start,
    Stop,
    Status,
    Logs,
}
pub(super) struct Options {
    pub action: Action,
    pub lines: usize,
}
pub(super) fn parse(args: &[String]) -> Result<Options, String> {
    let mut action = None;
    let mut lines = 80;
    let mut extras = Vec::new();
    let mut literal = false;
    let mut args = args.iter().peekable();
    while let Some(arg) = args.next() {
        if arg == "--" && !literal {
            literal = true;
            continue;
        }
        let (name, inline) = arg
            .split_once('=')
            .map_or((arg.as_str(), None), |(name, value)| (name, Some(value)));
        if !literal && name.starts_with("--") && "--lines".starts_with(name) {
            let value = match inline {
                Some(value) => value,
                None => args
                    .next_if(|value| {
                        !value.starts_with('-') || Value::text(value.as_str()).int().is_ok()
                    })
                    .map(String::as_str)
                    .ok_or_else(|| "argument --lines: expected one argument".to_owned())?,
            };
            let parsed = Value::text(value).int().map_err(|_| {
                format!(
                    "argument --lines: invalid int value: {}",
                    Value::text(value).repr().unwrap_or_default()
                )
            })?;
            lines = parsed.clamp(1.into(), 500.into()).to_usize().unwrap_or(80);
        } else if (literal || !arg.starts_with('-') || Value::text(arg).int().is_ok())
            && action.is_none()
        {
            action = Some(match arg.as_str() {
                "start" => Action::Start,
                "stop" => Action::Stop,
                "status" => Action::Status,
                "logs" => Action::Logs,
                _ => {
                    return Err(format!(
                    "argument action: invalid choice: {} (choose from start, status, logs, stop)",
                    Value::text(arg).repr().unwrap_or_default()
                ))
                }
            });
        } else {
            extras.push(arg.as_str());
        }
    }
    if !extras.is_empty() {
        return Err(format!("unrecognized arguments: {}", extras.join(" ")));
    }
    Ok(Options {
        action: action.unwrap_or(Action::Start),
        lines,
    })
}
pub(super) fn error(message: &str) {
    eprintln!("usage: carrot vision [--lines LINES] [{{start,status,logs,stop}}]\ncarrot vision: error: {message}");
}
