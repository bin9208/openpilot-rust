use crate::Error;
use openpilot_params::{Params, KEYS};
use openpilot_web_upload::{Fields, Value, DEFAULT_WEB_UPLOAD_URL};
use std::{fs, net::Ipv4Addr, path::Path};

pub fn utf8_ignore(mut bytes: &[u8]) -> String {
    let mut output = String::new();
    while !bytes.is_empty() {
        match std::str::from_utf8(bytes) {
            Ok(text) => {
                output.push_str(text);
                break;
            }
            Err(error) => {
                let valid = error.valid_up_to();
                if let Ok(text) = std::str::from_utf8(&bytes[..valid]) {
                    output.push_str(text);
                }
                bytes = &bytes[valid + error.error_len().unwrap_or(bytes.len() - valid)..];
            }
        }
    }
    output
}
pub fn private_ip(ip: Ipv4Addr) -> bool {
    let value = u32::from(ip);
    [
        ("0.0.0.0", 8),
        ("10.0.0.0", 8),
        ("127.0.0.0", 8),
        ("169.254.0.0", 16),
        ("172.16.0.0", 12),
        ("192.0.0.0", 29),
        ("192.0.0.170", 31),
        ("192.0.2.0", 24),
        ("192.168.0.0", 16),
        ("198.18.0.0", 15),
        ("198.51.100.0", 24),
        ("203.0.113.0", 24),
        ("240.0.0.0", 4),
    ]
    .into_iter()
    .any(|(network, prefix)| {
        network
            .parse::<Ipv4Addr>()
            .is_ok_and(|n| value >> (32 - prefix) == u32::from(n) >> (32 - prefix))
    })
}

pub fn web_settings(path: &Path) -> Fields {
    let raw = fs::read_to_string(path)
        .ok()
        .and_then(|s| openpilot_logmessaged::JsonValue::parse(&s).ok())
        .map(|value| crate::ingress::json::finite_value(&value));
    let value = raw
        .as_ref()
        .filter(|v| v.is_object())
        .and_then(|v| v.get("web_upload_url").or_else(|| v.get("toss_upload_url")));
    let text = value
        .filter(|v| crate::owner::packet::truth(v))
        .map(crate::owner::packet::py_text)
        .unwrap_or_default();
    let url = openpilot_web_upload::normalize_base_url(&text, DEFAULT_WEB_UPLOAD_URL)
        .unwrap_or_else(|_| DEFAULT_WEB_UPLOAD_URL.into());
    let url = if ["https://op.wjcloud.kr", "https://shind0.synology.me"]
        .contains(&url.to_lowercase().as_str())
    {
        DEFAULT_WEB_UPLOAD_URL.into()
    } else {
        url
    };
    [("web_upload_url".into(), Value::Text(url))]
        .into_iter()
        .collect()
}

fn cast(kind: u8, raw: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(raw).ok()?;
    match kind {
        0 => Some(text.into()),
        1 => Some(if raw == b"1" { "True" } else { "False" }.into()),
        2 => {
            let normalized = text.trim().replace('_', "");
            let negative = normalized.starts_with('-');
            let digits = normalized
                .trim_start_matches(['+', '-'])
                .trim_start_matches('0');
            (!normalized.trim_start_matches(['+', '-']).is_empty()
                && normalized
                    .trim_start_matches(['+', '-'])
                    .bytes()
                    .all(|b| b.is_ascii_digit()))
            .then(|| {
                format!(
                    "{}{}",
                    if negative && !digits.is_empty() {
                        "-"
                    } else {
                        ""
                    },
                    if digits.is_empty() { "0" } else { digits }
                )
            })
        }
        3 => {
            let value = openpilot_runtime_core::python_float::parse(text)?;
            let mut output = String::new();
            openpilot_runtime_core::python_float::write_float(value, &mut output).ok()?;
            Some(output)
        }
        4 => Some(text.replace('T', " ")),
        5 | 6 => None,
        _ => None,
    }
}
pub fn toggles(params: &Params, data: &Path) -> Result<(), Error> {
    let mut values = Fields::new();
    for key in KEYS {
        if matches!(key.kind, 5 | 6) {
            continue;
        }
        let Some(default) = key.default.and_then(|d| cast(key.kind, d.as_bytes())) else {
            continue;
        };
        let value = params
            .get(key.name)
            .ok()
            .flatten()
            .filter(|v| !v.is_empty())
            .and_then(|v| cast(key.kind, &v))
            .unwrap_or(default);
        values.insert(key.name.into(), Value::Text(value));
    }
    let compact = values
        .to_json()
        .map_err(|_| Error::Contract("toggle snapshot JSON"))?;
    fs::write(data.join("toggle_values.json"), pretty(&compact))?;
    Ok(())
}
pub fn backup(data: &Path) -> Result<(), Error> {
    let mut values = Vec::new();
    for entry in fs::read_dir(data.join("params/d"))? {
        let entry = entry?;
        let Ok(metadata) = entry.path().metadata() else {
            continue;
        };
        if metadata.is_file() && metadata.len() < 16 {
            values.push((
                entry.file_name().to_string_lossy().into_owned(),
                utf8_ignore(&fs::read(entry.path())?),
            ));
        }
    }
    values.sort_by(|a, b| a.0.cmp(&b.0));
    let entries = values
        .into_iter()
        .map(|(name, content)| {
            let fields: Fields = [
                ("filename".into(), Value::Text(name)),
                ("content".into(), Value::Text(content)),
            ]
            .into_iter()
            .collect();
            fields
                .to_json()
                .map_err(|_| Error::Contract("Params backup JSON"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let ascii = format!("[{}]", entries.join(", "));
    let text = openpilot_logmessaged::JsonValue::parse(&ascii)
        .map_err(|_| Error::Contract("Params backup JSON"))?
        .to_json_utf8()
        .map_err(|_| Error::Contract("Params backup encoding"))?;
    fs::write(data.join("backup_params.json"), pretty(&text))?;
    Ok(())
}
fn pretty(input: &str) -> String {
    let mut result = String::new();
    let mut depth = 0;
    let mut quoted = false;
    let mut escaped = false;
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if quoted {
            result.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                quoted = false;
            }
            continue;
        }
        match ch {
            '"' => {
                quoted = true;
                result.push(ch);
            }
            '{' | '[' => {
                result.push(ch);
                if !matches!(chars.peek(), Some('}' | ']')) {
                    depth += 1;
                    result.push('\n');
                    result.push_str(&"  ".repeat(depth));
                }
            }
            '}' | ']' => {
                if !matches!(result.chars().last(), Some('{' | '[')) {
                    depth = depth.saturating_sub(1);
                    result.push('\n');
                    result.push_str(&"  ".repeat(depth));
                }
                result.push(ch);
            }
            ',' => {
                result.push_str(",\n");
                result.push_str(&"  ".repeat(depth));
            }
            ':' => result.push_str(": "),
            ch if ch.is_whitespace() => {}
            _ => result.push(ch),
        }
    }
    result
}
