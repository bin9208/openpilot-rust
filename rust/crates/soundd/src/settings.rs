use crate::Error;
use openpilot_logging::producer::Logger;
use openpilot_params::{metadata, Params};

pub fn integer(params: &Params, key: &str) -> Result<i32, Error> {
    let bytes = match params.get(key) {
        Ok(value) => value.unwrap_or_default(),
        Err(openpilot_params::Error::Io(_)) => Vec::new(),
        Err(error) => return Err(error.into()),
    };
    Ok(openpilot_beepd::integer(&bytes)?)
}
fn string(params: &Params, key: &str, logger: &mut Logger) -> Result<String, Error> {
    Ok(
        openpilot_params_typed::get_string(params, key, logger)?.unwrap_or_else(|| {
            metadata(key)
                .and_then(|info| info.default)
                .unwrap_or("")
                .into()
        }),
    )
}
pub fn language(params: &Params, logger: &mut Logger) -> Result<String, Error> {
    let sound = string(params, "SoundLanguageSetting", logger)?
        .trim()
        .to_owned();
    if !sound.is_empty() && !sound.eq_ignore_ascii_case("auto") {
        return Ok(sound);
    }
    let language = string(params, "LanguageSetting", logger)?.trim().to_owned();
    Ok(if language.is_empty() {
        "en".into()
    } else {
        language
    })
}
pub fn directory(language: &str) -> &'static str {
    let normalized = language.trim().replace('_', "-").to_lowercase();
    let normalized = normalized.strip_prefix("main-").unwrap_or(&normalized);
    if normalized == "ko" || normalized.starts_with("ko-") {
        "sounds"
    } else if normalized.starts_with("zh") {
        "sounds_chs"
    } else {
        "sounds_eng"
    }
}
