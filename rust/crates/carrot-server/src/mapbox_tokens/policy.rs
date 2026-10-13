use crate::{Error, Value};

#[derive(Clone, Copy)]
pub(super) enum Key {
    Public,
    Secret,
}
impl Key {
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Secret => "secret",
        }
    }
    pub(super) const fn param(self) -> &'static str {
        match self {
            Self::Public => "MapboxPublicKey",
            Self::Secret => "MapboxSecretKey",
        }
    }
}

pub(super) fn stripped(value: &Value, empty_when_false: bool) -> Result<Value, Error> {
    let value = if empty_when_false && !value.truth() {
        Value::text("")
    } else {
        value.py_string()?
    };
    let Value::Text(points) = value else {
        return Err(Error::Source("expected token text".into()));
    };
    Ok(Value::Text(crate::state::trim(&points).to_vec()))
}

pub(super) fn key_type(value: &Value, default_when_false: bool) -> Result<Option<Key>, Error> {
    let value = if default_when_false && !value.truth() {
        Value::text("public")
    } else {
        value.clone()
    };
    let lower = lower(&stripped(&value, false)?)?;
    Ok(if lower.text_eq("public") {
        Some(Key::Public)
    } else if lower.text_eq("secret") {
        Some(Key::Secret)
    } else {
        None
    })
}

pub(super) fn lower(value: &Value) -> Result<Value, Error> {
    let Value::Text(points) = value else {
        return Err(Error::Source("expected key type text".into()));
    };
    let points: Vec<_> = points
        .iter()
        .copied()
        .flat_map(|point| {
            char::from_u32(point)
                .map(|character| character.to_lowercase().map(u32::from).collect::<Vec<_>>())
                .unwrap_or_else(|| vec![point])
        })
        .collect();
    Ok(Value::Text(points))
}

pub fn mask_token(value: &Value) -> Result<Value, Error> {
    let Value::Text(points) = stripped(value, true)? else {
        return Err(Error::Source("expected token text".into()));
    };
    if points.is_empty() {
        return Ok(Value::text(""));
    }
    let mut masked = if points.len() <= 12 {
        if points.len() > 3 {
            points[..3].to_vec()
        } else {
            Vec::new()
        }
    } else {
        points[..8].to_vec()
    };
    masked.push(0x2026);
    if points.len() > 12 {
        masked.extend_from_slice(&points[points.len() - 4..]);
    }
    Ok(Value::Text(masked))
}

pub(super) fn status(key: Key, token: &Value) -> Result<Value, Error> {
    Ok(Value::object([
        ("configured", Value::Bool(token.truth())),
        ("masked", mask_token(token)?),
        ("param", Value::text(key.param())),
    ]))
}

pub fn format_result(key_type: &str, value: &Value) -> Result<Value, Error> {
    let Value::Text(points) = stripped(value, true)? else {
        return Err(Error::Source("expected token text".into()));
    };
    let (label, prefix) = if key_type == "public" {
        ("public", "pk.")
    } else {
        ("secret", "sk.")
    };
    let (valid, reason, message) = if points.is_empty() {
        (
            false,
            "required",
            format!("Mapbox {label} token is required."),
        )
    } else if points.iter().any(|point| {
        char::from_u32(*point).is_some_and(|character| {
            character.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&character)
        })
    }) {
        (
            false,
            "space",
            "Mapbox token must not contain spaces.".into(),
        )
    } else if !points.starts_with(&prefix.chars().map(u32::from).collect::<Vec<_>>()) {
        (
            false,
            "prefix",
            format!("Mapbox {label} token should start with {prefix}"),
        )
    } else if points.len() < 20 {
        (false, "short", "Mapbox token is too short.".into())
    } else {
        (true, "ok", "Mapbox token format looks valid.".into())
    };
    Ok(Value::object([
        ("ok", Value::Bool(valid)),
        ("format_ok", Value::Bool(valid)),
        ("reason", Value::text(reason)),
        ("message", Value::text(&message)),
    ]))
}
