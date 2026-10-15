use super::{paths, Failure};
use crate::{param_changes::text, Error, Value};

pub(super) fn object(body: &Value) -> Result<(), Failure> {
    if matches!(body, Value::Object(_)) {
        Ok(())
    } else {
        Err(Error::Source(format!(
            "'{}' object has no attribute 'get'",
            body.type_name()
        ))
        .into())
    }
}
pub(super) fn segments(body: &Value) -> Result<Vec<String>, Failure> {
    if !matches!(body, Value::Object(_)) {
        return Err(Failure::http(400, "invalid upload request"));
    }
    include_all_files(body)?;
    let values = match body.get("segments") {
        Value::Array(values) => values.as_slice(),
        _ => {
            let one = body.get("segment");
            if one.truth() {
                std::slice::from_ref(one)
            } else {
                &[]
            }
        }
    };
    let segments = values
        .iter()
        .filter(|value| value.truth())
        .map(|value| {
            paths::safe_segment(&text::string(value, false)?)?
                .string()
                .map_err(Error::from)
                .map_err(Failure::from)
        })
        .collect::<Result<Vec<_>, _>>()?;
    if segments.is_empty() {
        return Err(Failure::http(400, "missing segments"));
    }
    Ok(segments)
}
pub(super) fn cancel_id(body: &Value) -> Result<String, Failure> {
    object(body)?;
    let id = body.get("id");
    let id = if id.truth() { id } else { body.get("job_id") };
    text::stripped(id, true)?
        .string()
        .map_err(Error::from)
        .map_err(Failure::from)
}

pub(super) fn include_all_files(body: &Value) -> Result<bool, Failure> {
    if !matches!(body, Value::Object(_)) {
        return Err(Failure::http(400, "invalid upload request"));
    }
    if !body.has("includeAllFiles") {
        return Ok(false);
    }
    match body.get("includeAllFiles") {
        Value::Bool(value) => Ok(*value),
        _ => Err(Failure::http(400, "includeAllFiles must be a boolean")),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scope_rejects_nonboolean_before_segment_validation() {
        for input in ["null", "[]", "1", "true"] {
            assert!(matches!(
                segments(&Value::parse(input).unwrap()),
                Err(Failure::Http { status: 400, .. })
            ));
        }
        for input in ["null", "0", "1", "\"true\"", "[]", "{}"] {
            let body = Value::parse(&format!("{{\"includeAllFiles\":{input}}}")).unwrap();
            assert!(
                matches!(segments(&body),Err(Failure::Http{status:400,message}) if message=="includeAllFiles must be a boolean")
            );
        }
        assert!(!include_all_files(&Value::parse("{}").unwrap()).unwrap());
        assert!(include_all_files(&Value::parse("{\"includeAllFiles\":true}").unwrap()).unwrap());
    }
}
