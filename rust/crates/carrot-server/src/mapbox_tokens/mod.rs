//! Original features/mapbox_tokens.py local token policies and route results.
mod http;
mod online;
mod policy;
#[cfg(test)]
mod raw_test;
mod response;
mod text;
pub use http::{handle, matches};
pub use online::Online;

use crate::{params::Backend, Error, Value};
pub use policy::{format_result, mask_token};
use policy::{key_type, stripped, Key};

pub struct Reply {
    pub status: u16,
    pub body: Value,
}

pub enum Validation {
    Complete(Reply),
    Public { token: Value, result: Value },
}

fn reject(status: u16, message: impl AsRef<str>) -> Reply {
    Reply {
        status,
        body: Value::object([
            ("ok", Value::Bool(false)),
            ("error", Value::text(message.as_ref())),
        ]),
    }
}

fn read(backend: &Backend, key: Key) -> Result<Value, Error> {
    stripped(&backend.get(key.param(), &Value::text("")), true)
}

pub fn status(backend: &Backend) -> Result<Value, Error> {
    let public = read(backend, Key::Public)?;
    let secret = read(backend, Key::Secret)?;
    Ok(Value::object([
        ("public", policy::status(Key::Public, &public)?),
        ("secret", policy::status(Key::Secret, &secret)?),
    ]))
}

fn result(backend: &Backend, key: Option<Key>) -> Result<Reply, Error> {
    let mut fields = vec![("ok".chars().map(u32::from).collect(), Value::Bool(true))];
    if let Some(key) = key {
        fields.push((
            "key_type".chars().map(u32::from).collect(),
            Value::text(key.label()),
        ));
    }
    if let Value::Object(status) = status(backend)? {
        fields.extend(status);
    }
    Ok(Reply {
        status: 200,
        body: Value::Object(fields),
    })
}

pub fn get(backend: &Backend) -> Result<Reply, Error> {
    result(backend, None)
}

fn body_field<'a>(
    body: &'a Value,
    key: &str,
    alias: &str,
    default: &'a Value,
) -> Result<&'a Value, Error> {
    if !matches!(body, Value::Object(_)) {
        return Err(Error::Source(format!(
            "'{}' object has no attribute 'get'",
            body.type_name()
        )));
    }
    Ok(if body.has(key) {
        body.get(key)
    } else if body.has(alias) {
        body.get(alias)
    } else {
        default
    })
}

pub fn set(body: &Value, backend: &mut Backend) -> Result<Reply, Error> {
    let public = Value::text("public");
    let key = key_type(body_field(body, "key_type", "type", &public)?, true)?;
    let Some(key) = key else {
        return Ok(reject(400, "invalid key_type"));
    };
    let token = stripped(body_field(body, "token", "value", &Value::Null)?, true)?;
    let format = format_result(key.label(), &token)?;
    if !format.get("format_ok").truth() {
        let mut response = reject(400, format.get("message").string()?).body;
        if let Value::Object(format) = format {
            for (name, value) in format {
                crate::json_fields::set(&mut response, &Value::Text(name).string()?, value)?;
            }
        }
        return Ok(Reply {
            status: 400,
            body: response,
        });
    }
    let written = if backend.has_params() {
        text::utf8(&token).map(|_| ())
    } else {
        Ok(())
    }
    .and_then(|()| backend.put(key.param(), &token, None));
    if let Err(error) = written {
        return Ok(reject(500, error.to_string()));
    }
    result(backend, Some(key))
}

pub fn clear(query: &Value, backend: &mut Backend) -> Result<Reply, Error> {
    let public = Value::text("public");
    let Some(key) = key_type(body_field(query, "key_type", "type", &public)?, true)? else {
        return Ok(reject(400, "invalid key_type"));
    };
    if let Err(error) = backend.put(key.param(), &Value::text(""), None) {
        return Ok(reject(500, error.to_string()));
    }
    result(backend, Some(key))
}

pub fn prepare_validation(body: &Value, backend: &Backend) -> Result<Validation, Error> {
    let public = Value::text("public");
    let key = if matches!(body, Value::Object(_)) {
        body_field(body, "key_type", "type", &public)?
    } else {
        &public
    };
    let Some(key) = key_type(key, false)? else {
        return Ok(Validation::Complete(reject(400, "invalid key_type")));
    };
    let explicit = matches!(body, Value::Object(_)) && (body.has("token") || body.has("value"));
    let token = if explicit {
        stripped(body_field(body, "token", "value", &Value::Null)?, true)?
    } else {
        read(backend, key)?
    };
    let mut result = format_result(key.label(), &token)?;
    if result.get("format_ok").truth() {
        match key {
            Key::Public => return Ok(Validation::Public { token, result }),
            Key::Secret => {
                crate::json_fields::set(&mut result, "online_ok", Value::Null)?;
                crate::json_fields::set(
                    &mut result,
                    "message",
                    Value::text(
                        "Secret key format looks valid. This build does not use MapboxSecretKey at runtime.",
                    ),
                )?;
            }
        }
    }
    Ok(Validation::Complete(Reply {
        status: if result.get("ok").truth() { 200 } else { 409 },
        body: result,
    }))
}

pub fn complete_validation(mut result: Value, online: Value) -> Result<Reply, Error> {
    if let Value::Object(online) = online {
        for (name, value) in online {
            crate::json_fields::set(&mut result, &Value::Text(name).string()?, value)?;
        }
    }
    let ok = result.get("format_ok").truth() && result.get("online_ok").truth();
    crate::json_fields::set(&mut result, "ok", Value::Bool(ok))?;
    Ok(Reply {
        status: if ok { 200 } else { 409 },
        body: result,
    })
}
