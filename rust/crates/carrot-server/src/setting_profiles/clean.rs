use crate::{param_changes::text, settings::Catalog, Error, Value};

pub(super) fn name(value: &Value) -> Result<Value, Error> {
    let Value::Text(points) = text::stripped(value, true)? else {
        return Err(Error::Source("expected profile name".into()));
    };
    let mut output = Vec::new();
    let mut pending_space = false;
    for point in points {
        if char::from_u32(point)
            .is_some_and(|c| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c))
        {
            pending_space = true;
            continue;
        }
        if pending_space {
            output.push(32);
            pending_space = false;
        }
        output.push(point);
    }
    output.truncate(40);
    Ok(Value::Text(output))
}
pub(super) fn values(raw: &Value, catalog: &Catalog) -> Value {
    let Value::Object(fields) = raw else {
        return Value::Object(Vec::new());
    };
    Value::Object(fields.iter().filter(|(key,_)|matches!(&catalog.by_name,Value::Object(allowed) if allowed.iter().any(|(name,_)|name==key))).cloned().collect())
}
pub(super) fn profile(raw: &Value, catalog: &Catalog) -> Result<Option<Value>, Error> {
    if !matches!(raw, Value::Object(_)) {
        return Ok(None);
    }
    let id = text::stripped(raw.get("id"), true)?;
    let name = name(raw.get("name"))?;
    let values = values(raw.get("values"), catalog);
    if !id.truth() || !name.truth() || !values.truth() {
        return Ok(None);
    }
    let created = text::stripped(raw.get("created_at"), true)?;
    let updated = text::stripped(
        if raw.get("updated_at").truth() {
            raw.get("updated_at")
        } else {
            &created
        },
        false,
    )?;
    let meta = raw.get("meta");
    let meta = Value::Object(
        [
            "branch",
            "commit",
            "commit_short",
            "commit_date",
            "remote",
            "commit_url",
        ]
        .into_iter()
        .map(|key| {
            Ok((
                key.chars().map(u32::from).collect(),
                text::string(meta.get(key), true)?,
            ))
        })
        .collect::<Result<_, Error>>()?,
    );
    Ok(Some(Value::object([
        ("id", id),
        ("name", name),
        ("created_at", created),
        ("updated_at", updated),
        ("meta", meta),
        ("values", values),
    ])))
}
pub(super) fn store(raw: &Value, catalog: &Catalog, writing: bool) -> Result<Value, Error> {
    let items = match raw.get("profiles") {
        Value::Array(items) => items.as_slice(),
        Value::Text(_) | Value::Object(_) => &[],
        Value::Null if !writing || !raw.has("profiles") => &[],
        other => {
            if writing && matches!(raw, Value::Object(_)) {
                return Err(Error::Source(format!(
                    "'{}' object is not iterable",
                    other.type_name()
                )));
            } else {
                &[]
            }
        }
    };
    let mut profiles = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for item in items {
        let Some(profile) = profile(item, catalog)? else {
            continue;
        };
        let Value::Text(id) = profile.get("id") else {
            return Err(Error::Source("expected profile id".into()));
        };
        if !seen.insert(id.clone()) {
            continue;
        }
        profiles.push(profile);
        if profiles.len() >= 40 {
            break;
        }
    }
    Ok(Value::object([("profiles", Value::Array(profiles))]))
}
