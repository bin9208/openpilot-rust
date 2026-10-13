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
    object(body)?;
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
