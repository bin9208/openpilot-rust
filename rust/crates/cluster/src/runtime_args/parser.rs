use super::{flags, ParseError, RuntimeArgs, VALUE_OPTIONS};
use std::path::Path;

fn all_options() -> impl Iterator<Item = &'static str> {
    VALUE_OPTIONS
        .iter()
        .copied()
        .chain(flags::OPTIONS.iter().copied())
        .chain(["--metric", "--imperial", "--help"])
}
fn resolve(option: &str) -> Result<&'static str, ParseError> {
    if option.starts_with("-h") {
        return Ok("--help");
    }
    if let Some(exact) = all_options().find(|&known| known == option) {
        return Ok(exact);
    }
    let matches: Vec<_> = all_options()
        .filter(|known| option.starts_with("--") && known.starts_with(option))
        .collect();
    match matches.as_slice() {
        [one] => Ok(one),
        [] => Err(ParseError::Invalid(format!(
            "unrecognized arguments: {option}"
        ))),
        _ => Err(ParseError::Invalid(format!(
            "ambiguous option: {option} could match {}",
            matches.join(", ")
        ))),
    }
}
fn looks_like_option(value: &str) -> bool {
    if !value.starts_with('-') || value == "-" || value.contains(' ') {
        return false;
    }
    let value = &value[1..];
    let negative_number = if let Some((whole, fraction)) = value.split_once('.') {
        whole.chars().all(is_decimal) && !fraction.is_empty() && fraction.chars().all(is_decimal)
    } else {
        !value.is_empty() && value.chars().all(is_decimal)
    };
    !negative_number
}
fn is_decimal(character: char) -> bool {
    openpilot_ui_application::params::typed::integer_text(&character.to_string()).is_some()
}
fn present<S: AsRef<str>>(argv: &[S], option: &str) -> bool {
    argv.iter().any(|arg| {
        arg.as_ref() == option
            || arg
                .as_ref()
                .strip_prefix(option)
                .is_some_and(|suffix| suffix.starts_with('='))
    })
}
pub(super) fn parse<S: AsRef<str>>(root: &Path, argv: &[S]) -> Result<RuntimeArgs, ParseError> {
    let mut parsed = RuntimeArgs {
        usb_h264_library: root
            .join("system/loggerd/libcluster_h264_encoder_bridge.so")
            .to_string_lossy()
            .into_owned(),
        ..RuntimeArgs::default()
    };
    let mut index = 0;
    let mut unknown = Vec::new();
    while index < argv.len() {
        let raw = argv[index].as_ref();
        index += 1;
        if raw == "--" {
            unknown.push(raw);
            unknown.extend(argv[index..].iter().map(AsRef::as_ref));
            break;
        }
        let (key, inline) = raw
            .split_once('=')
            .map_or((raw, None), |(key, value)| (key, Some(value)));
        let option = match resolve(key) {
            Ok(option) => option,
            Err(error) if error.to_string().starts_with("unrecognized") => {
                unknown.push(raw);
                continue;
            }
            Err(error) => return Err(error),
        };
        if VALUE_OPTIONS.contains(&option) {
            let value = if let Some(value) = inline {
                value
            } else {
                let value = argv
                    .get(index)
                    .map(AsRef::as_ref)
                    .filter(|value| !looks_like_option(value))
                    .ok_or_else(|| {
                        ParseError::Invalid(format!("argument {option}: expected one argument"))
                    })?;
                index += 1;
                value
            };
            parsed.apply_value(option, value)?;
            if option == "--camera-view-mode"
                && parsed
                    .camera_view_mode
                    .is_some_and(|mode| !(0..=4).contains(&mode))
            {
                return Err(ParseError::Invalid(format!("argument --camera-view-mode: invalid choice: {value} (choose from 0, 1, 2, 3, 4)")));
            }
            if option == "--fps" {
                parsed.rate_presence.fps_from_cli = true;
            }
        } else {
            if let Some(value) = inline {
                return Err(ParseError::Invalid(format!(
                    "argument {option}: ignored explicit argument {value:?}"
                )));
            }
            match option {
                "--help" => {
                    return Err(ParseError::Help(format!(
                        "Usage: cluster [options]\n\n{}\n",
                        all_options().collect::<Vec<_>>().join("\n")
                    )))
                }
                "--metric" | "--imperial" => {
                    let metric = option == "--metric";
                    if parsed.is_metric.is_some_and(|old| old != metric) {
                        return Err(ParseError::Invalid(format!(
                            "argument {option}: not allowed with argument {}",
                            if metric { "--imperial" } else { "--metric" }
                        )));
                    }
                    parsed.is_metric = Some(metric);
                }
                _ => flags::apply(&mut parsed, option),
            }
        }
    }
    if !unknown.is_empty() {
        return Err(ParseError::Invalid(format!(
            "unrecognized arguments: {}",
            unknown.join(" ")
        )));
    }
    parsed.rate_presence.usb_brightness_from_cli = parsed.usb_brightness.is_some();
    parsed.encoder_presence.usb_codec_from_cli = present(argv, "--usb-codec");
    parsed.encoder_presence.usb_h264_backend_from_cli = present(argv, "--usb-h264-backend");
    parsed.encoder_presence.usb_h264_ffmpeg_encoder_from_cli =
        present(argv, "--usb-h264-ffmpeg-encoder");
    parsed.validate()?;
    Ok(parsed)
}
