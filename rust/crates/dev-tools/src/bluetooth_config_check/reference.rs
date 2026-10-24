use crate::seal::Result;
use openpilot_log_format::{JsonValue, JsonView};
use std::collections::BTreeSet;

use super::cases::ACTIONS;

fn error(message: &str) -> Result<JsonValue> {
    Ok(JsonValue::parse(&format!("{{\"error\": \"{message}\"}}"))?)
}
fn field(value: &JsonValue, name: &str, default: &str) -> Result<JsonValue> {
    Ok(value.get(name).unwrap_or(JsonValue::parse(default)?))
}
fn token(value: &[u32]) -> bool {
    let Some(text) = value
        .iter()
        .copied()
        .map(char::from_u32)
        .collect::<Option<String>>()
    else {
        return false;
    };
    let base = text
        .strip_suffix("@double")
        .or_else(|| text.strip_suffix("@long"))
        .unwrap_or(&text);
    if ["up", "down", "left", "right", "center", "1", "2"].contains(&base) {
        return true;
    }
    if let Some(number) = base.strip_prefix("key:") {
        return (1..=4).contains(&number.len()) && number.bytes().all(|c| c.is_ascii_digit());
    }
    if let Some(direction) = base.strip_prefix("swipe:") {
        return ["x+", "x-", "y+", "y-"].contains(&direction);
    }
    if let Some(coords) = base.strip_prefix("tap:") {
        let Some((x, y)) = coords.split_once(':') else {
            return false;
        };
        return [x, y].into_iter().all(|number| {
            (1..=5).contains(&number.len()) && number.bytes().all(|c| c.is_ascii_digit())
        });
    }
    false
}
fn key_code(value: &[u32]) -> bool {
    let Some(text) = value
        .iter()
        .copied()
        .map(char::from_u32)
        .collect::<Option<String>>()
    else {
        return true;
    };
    let base = text.split('@').next().unwrap_or("");
    !base.starts_with("key:") || base[4..].parse::<u32>().is_ok_and(|code| code <= 767)
}
fn address(value: &[u32]) -> Option<String> {
    let text = value
        .iter()
        .copied()
        .map(char::from_u32)
        .collect::<Option<String>>()?
        .to_uppercase();
    let bytes = text.as_bytes();
    (bytes.len() == 17
        && bytes.iter().enumerate().all(|(i, c)| {
            if i % 3 == 2 {
                *c == b':'
            } else {
                c.is_ascii_hexdigit() && !c.is_ascii_lowercase()
            }
        }))
    .then_some(text)
}
pub fn expected(input: &JsonValue) -> Result<JsonValue> {
    let JsonView::Object(_) = input.view() else {
        return error("devices must be an object");
    };
    let devices = field(input, "devices", "{}")?;
    let JsonView::Object(entries) = devices.view() else {
        return error("devices must be an object");
    };
    if entries.len() > 16 {
        return error("at most 16 remotes");
    }
    let mut output: Vec<(String, JsonValue)> = Vec::new();
    for (key, device) in entries {
        let Some(mac) = address(key) else {
            return error("invalid Bluetooth address");
        };
        let JsonView::Object(_) = device.view() else {
            return error("unknown input profile");
        };
        let profile = device.get("profile");
        let Some(profile) = profile.filter(|v| v.text_eq("generic") || v.text_eq("yiser-j6"))
        else {
            return error("unknown input profile");
        };
        let mapping = field(&device, "mapping", "{}")?;
        let JsonView::Object(items) = mapping.view() else {
            return error("invalid mapping");
        };
        let unique = items
            .iter()
            .map(|(key, _)| {
                key.iter()
                    .copied()
                    .take_while(|c| *c != u32::from(b'@'))
                    .collect::<Vec<_>>()
            })
            .collect::<BTreeSet<_>>();
        if items.len() > 192 || unique.len() > 64 {
            return error("invalid mapping");
        }
        for (key, action) in &items {
            if !token(key) || !ACTIONS.iter().any(|name| action.text_eq(name)) {
                return error("invalid button or action");
            }
            if !key_code(key) {
                return error("invalid Linux key code");
            }
        }
        let enabled = field(&device, "enabled", "false")?;
        if !matches!(enabled.view(), JsonView::Bool(_)) {
            return error("enabled must be boolean");
        }
        let name = device.get("name").unwrap_or(JsonValue::text(&mac));
        let name = JsonValue::codepoints(
            openpilot_runtime_version::python_str(&name)?
                .into_iter()
                .take(80)
                .collect(),
        )
        .ok_or("invalid Python name")?;
        let normalized = JsonValue::parse(&format!(
            "{{\"name\": {}, \"profile\": {}, \"enabled\": {}, \"mapping\": {}}}",
            name.to_json()?,
            profile.to_json()?,
            enabled.to_json()?,
            mapping.to_json()?
        ))?;
        if let Some((_, previous)) = output.iter_mut().find(|(key, _)| key == &mac) {
            *previous = normalized;
        } else {
            output.push((mac, normalized));
        }
    }
    let devices = output
        .into_iter()
        .map(|(mac, value)| {
            Ok(format!(
                "{}: {}",
                JsonValue::text(&mac).to_json()?,
                value.to_json()?
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(JsonValue::parse(&format!(
        "{{\"version\": 1, \"devices\": {{{}}}}}",
        devices.join(", ")
    ))?)
}
