use openpilot_params::{metadata, Params};

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub volume: f64,
    pub engage_volume: f64,
    pub sound_directory: &'static str,
}

fn percent(params: &Params, key: &str) -> f64 {
    let bytes = match params.get(key) {
        Ok(bytes) => bytes.unwrap_or_default(),
        Err(openpilot_params::Error::Io(_)) => Vec::new(),
        Err(openpilot_params::Error::UnknownKey(_) | openpilot_params::Error::InvalidPrefix) => {
            return 1.
        }
    };
    match openpilot_beepd::integer(&bytes) {
        Ok(value) => (f64::from(value) / 100.).clamp(0., 2.),
        Err(error) => crate::param_native::fatal(key, &error),
    }
}

fn text(params: &Params, key: &str) -> Option<String> {
    let bytes = match params.get(key) {
        Ok(bytes) => bytes.unwrap_or_default(),
        Err(openpilot_params::Error::Io(_)) => Vec::new(),
        Err(openpilot_params::Error::UnknownKey(_) | openpilot_params::Error::InvalidPrefix) => {
            return None
        }
    };
    let default = metadata(key).and_then(|info| info.default).unwrap_or("");
    if bytes.is_empty() {
        return Some(default.into());
    }
    Some(std::str::from_utf8(&bytes).unwrap_or(default).into())
}

fn strip(text: &str) -> &str {
    text.trim_matches(|character: char| {
        character.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&character)
    })
}

impl Settings {
    /// Read the source's direct get_int volumes and STRING return_default languages.
    pub fn read(params: &Params) -> Self {
        let volumes = (
            percent(params, "SoundVolumeAdjust"),
            percent(params, "SoundVolumeAdjustEngage"),
        );
        let Some(requested) = text(params, "SoundLanguageSetting") else {
            return Self::from_values(volumes, ("en", ""));
        };
        let fallback = if strip(&requested).is_empty() || strip(&requested).to_lowercase() == "auto"
        {
            let Some(value) = text(params, "LanguageSetting") else {
                return Self::from_values(volumes, ("en", ""));
            };
            value
        } else {
            String::new()
        };
        Self::from_values(volumes, (&requested, &fallback))
    }

    pub fn from_values(volumes: (f64, f64), languages: (&str, &str)) -> Self {
        let requested = strip(languages.0);
        let language = if requested.is_empty() || requested.to_lowercase() == "auto" {
            let fallback = strip(languages.1);
            if fallback.is_empty() {
                "en"
            } else {
                fallback
            }
        } else {
            requested
        };
        let normalized = language.replace('_', "-").to_lowercase();
        let normalized = normalized.strip_prefix("main-").unwrap_or(&normalized);
        let sound_directory = if normalized == "ko" || normalized.starts_with("ko-") {
            "sounds"
        } else if normalized.starts_with("zh") {
            "sounds_chs"
        } else {
            "sounds_eng"
        };
        Self {
            volume: volumes.0,
            engage_volume: volumes.1,
            sound_directory,
        }
    }
}
