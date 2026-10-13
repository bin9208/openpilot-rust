use super::Intro;
use crate::{params::Backend, Error, Value};

fn text(value: Option<Value>) -> Result<String, Error> {
    let value = value.unwrap_or(Value::Null);
    let value = if matches!(value, Value::Null) {
        Value::text("")
    } else {
        value.py_string()?
    };
    let Value::Text(points) = value else {
        return Err(Error::Source("intro text conversion failed".into()));
    };
    let points = crate::state::trim(&points);
    Ok(points
        .iter()
        .copied()
        .map(|point| char::from_u32(point).unwrap_or('\u{fffd}'))
        .collect())
}

impl Intro {
    pub(super) fn existing_reason(&self, params: &Backend) -> Result<String, Error> {
        for (file, label) in [
            ("web_settings.json", "web_settings_exists"),
            ("setting_profiles.json", "setting_profiles_exists"),
            ("setting_favorites.json", "setting_favorites_exists"),
            ("youtube_live.json", "youtube_live_exists"),
        ] {
            if self.config.state.join(file).is_file() {
                return Ok(label.into());
            }
        }
        if !params.has_params() {
            return Ok(String::new());
        }
        let selected = text(params.typed_value("CarSelected3", false))?;
        if !selected.is_empty() && selected != "-" && !selected.to_lowercase().contains("mock") {
            return Ok("car_already_selected".into());
        }
        if let Ok(Value::Object(definitions)) = self.definitions(params) {
            for (name, _) in definitions {
                let Ok(name) = Value::Text(name).string() else {
                    continue;
                };
                if name.is_empty() {
                    continue;
                }
                let Some(default) = params.registered_default(&name) else {
                    continue;
                };
                let current = text(params.typed_value(&name, false));
                let default = text(Some(default));
                if let (Ok(current), Ok(default)) = (current, default) {
                    if current != default {
                        return Ok(format!("setting_changed:{name}"));
                    }
                }
            }
        }
        let fingerprint = text(params.typed_value("CarName", false))?;
        if !fingerprint.is_empty() && !fingerprint.to_lowercase().contains("mock") {
            return Ok("car_fingerprinted".into());
        }
        Ok(String::new())
    }
}
