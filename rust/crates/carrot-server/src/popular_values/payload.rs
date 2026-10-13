//! Snapshot policy from server/services/popular_values.py.
use crate::{json_fields::set, params::Backend, settings::Catalog, Error, Value};
use openpilot_logmessaged::JsonValue;
use sha2::{Digest, Sha256};

pub fn param_text(params: &Backend, name: &str) -> String {
    let Some(native) = params.native_params() else {
        return String::new();
    };
    native
        .get(name)
        .ok()
        .flatten()
        .map(|bytes| strip(&String::from_utf8_lossy(&bytes)).to_owned())
        .unwrap_or_default()
}

pub fn device_id(params: &Backend, hostname: &str) -> String {
    for name in ["DongleId", "HardwareSerial"] {
        let value = param_text(params, name);
        if !value.is_empty()
            && !matches!(
                value.to_lowercase().as_str(),
                "unknown" | "none" | "null" | "unregistereddevice"
            )
        {
            return value;
        }
    }
    if hostname.is_empty() {
        "comma"
    } else {
        hostname
    }
    .into()
}

pub fn repo_id(remote: &str) -> String {
    let remote = strip(remote);
    let normalized = remote
        .strip_prefix("git@github.com:")
        .map(|path| format!("https://github.com/{path}"));
    let text = normalized
        .as_deref()
        .unwrap_or(remote)
        .replace(['\r', '\n', '\t'], "");
    // urlparse also accepts bare paths and arbitrary schemes, unlike Url::parse.
    let path = if let Some((_, rest)) = text.split_once("://") {
        rest.find('/').map_or("", |index| &rest[index..])
    } else if let Some(rest) = text.strip_prefix("//") {
        rest.find('/').map_or("", |index| &rest[index..])
    } else {
        text.split_once(':')
            .filter(|(scheme, _)| {
                scheme.starts_with(|c: char| c.is_ascii_alphabetic())
                    && scheme
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
            })
            .map_or(text.as_str(), |(_, path)| path)
    };
    let path = path
        .split(['?', '#'])
        .next()
        .unwrap_or("")
        .trim_matches('/');
    let path = if let Some((head, tail)) = path.rsplit_once('/') {
        tail.find(';').map_or_else(
            || path.to_owned(),
            |index| format!("{head}/{}", &tail[..index]),
        )
    } else {
        path.split(';').next().unwrap_or("").to_owned()
    };
    let path = path.strip_suffix(".git").unwrap_or(&path);
    let parts: Vec<_> = path.split('/').filter(|part| !part.is_empty()).collect();
    if parts.len() < 2 {
        String::new()
    } else {
        format!("{}/{}", parts[parts.len() - 2], parts[parts.len() - 1])
    }
}

pub(super) fn strip(text: &str) -> &str {
    text.trim_matches(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c))
}

fn names(catalog: &Catalog) -> Result<Vec<String>, Error> {
    let Value::Object(fields) = &catalog.by_name else {
        return Err(Error::Source("invalid setting catalog".into()));
    };
    fields
        .iter()
        .filter(|(key, _)| !key.is_empty())
        .map(|(key, _)| Value::Text(key.clone()).string().map_err(Error::from))
        .collect()
}

fn descriptor(setting: &Value) -> Value {
    Value::object([
        ("min", setting.get("min").clone()),
        ("max", setting.get("max").clone()),
        ("default", setting.get("default").clone()),
        ("unit", setting.get("unit").clone()),
    ])
}

fn canonical(value: &Value) -> Result<String, Error> {
    match value {
        Value::Text(points) => JsonValue::codepoints(points.clone())
            .ok_or_else(|| Error::Source("invalid Python text".into()))?
            .to_json_utf8()
            .map_err(|_| Error::Source("cannot encode settings hash as UTF-8".into())),
        Value::Object(fields) => {
            let mut fields: Vec<_> = fields.iter().collect();
            fields.sort_by(|(left, _), (right, _)| left.cmp(right));
            let items = fields
                .into_iter()
                .map(|(key, value)| {
                    Ok(format!(
                        "{}:{}",
                        canonical(&Value::Text(key.clone()))?,
                        canonical(value)?
                    ))
                })
                .collect::<Result<Vec<_>, Error>>()?;
            Ok(format!("{{{}}}", items.join(",")))
        }
        Value::Array(items) => Ok(format!(
            "[{}]",
            items
                .iter()
                .map(canonical)
                .collect::<Result<Vec<_>, _>>()?
                .join(",")
        )),
        Value::Null | Value::Bool(_) | Value::Integer(_) | Value::Float(_) => {
            value.encode().map_err(Error::from)
        }
    }
}

pub fn settings_hash(catalog: &Catalog) -> Result<String, Error> {
    let mut by_name = Value::Object(Vec::new());
    if let Value::Array(items) = catalog.data.get("params") {
        for item in items.iter().filter(|item| matches!(item, Value::Object(_))) {
            let name = if item.get("name").truth() {
                item.get("name").string()?
            } else {
                String::new()
            };
            set(&mut by_name, &name, item.clone())?;
        }
    }
    let mut items = Vec::new();
    for name in names(catalog)? {
        let mut item = Value::object([("name", Value::text(&name))]);
        let Value::Object(fields) = descriptor(by_name.get(&name)) else {
            return Err(Error::Source("invalid catalog descriptor".into()));
        };
        for (key, value) in fields {
            set(&mut item, &Value::Text(key).string()?, value)?;
        }
        items.push(item);
    }
    let payload = Value::object([
        ("apilot", catalog.data.get("apilot").clone()),
        ("params", Value::Array(items)),
    ]);
    Ok(format!(
        "{:x}",
        Sha256::digest(canonical(&payload)?.as_bytes())
    ))
}

pub fn coerce(value: &Value, setting: &Value) -> Result<Value, Error> {
    match crate::params::infer_type(setting) {
        "bool" => Ok(Value::integer(u8::from(match value {
            Value::Text(_) => matches!(
                strip(&value.string()?).to_lowercase().as_str(),
                "1" | "true" | "on" | "yes"
            ),
            _ => value.truth(),
        }))),
        "int" => value
            .float()
            .and_then(|number| Value::Float(number).int())
            .or_else(|_| {
                let default = setting.get("default");
                if default.truth() {
                    default.int()
                } else {
                    Value::integer(0).int()
                }
            })
            .map(Value::Integer)
            .map_err(Error::from),
        "float" => value
            .float()
            .or_else(|_| {
                let default = setting.get("default");
                if default.truth() {
                    default.float()
                } else {
                    Ok(0.)
                }
            })
            .map(Value::Float)
            .map_err(Error::from),
        _ => Ok(value.clone()),
    }
}

pub fn snapshot(
    params: &Backend,
    catalog: &Catalog,
    hostname: &str,
) -> Result<Option<Value>, Error> {
    if !params.has_params() {
        return Ok(None);
    }
    let names = names(catalog)?;
    if names.is_empty() {
        return Ok(None);
    }
    let car_key = param_text(params, "CarSelected3");
    if car_key.is_empty() {
        return Ok(None);
    }
    let mut values = Value::Object(Vec::new());
    let mut descriptors = Value::Object(Vec::new());
    for name in names {
        let setting = catalog.by_name.get(&name);
        let default = if setting.has("default") {
            setting.get("default").clone()
        } else {
            Value::integer(0)
        };
        let value = crate::param_restore::read_setting_value(params, &name, &default);
        set(&mut values, &name, coerce(&value, setting)?)?;
        set(&mut descriptors, &name, descriptor(setting))?;
    }
    let remote = param_text(params, "GitRemote");
    Ok(Some(Value::object([
        ("schema_version", Value::integer(1)),
        ("device_id", Value::text(&device_id(params, hostname))),
        ("repo_id", Value::text(&repo_id(&remote))),
        ("repo_remote", Value::text(&remote)),
        ("car_key_type", Value::text("CarSelected3")),
        ("car_key", Value::text(&car_key)),
        (
            "settings_version",
            catalog
                .data
                .get("apilot")
                .int()
                .map(Value::Integer)
                .unwrap_or(Value::Null),
        ),
        ("settings_hash", Value::text(&settings_hash(catalog)?)),
        ("app_commit", Value::text(&param_text(params, "GitCommit"))),
        ("param_catalog", descriptors),
        ("values", values),
    ])))
}
