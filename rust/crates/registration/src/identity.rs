use crate::Error;
use openpilot_logmessaged::{JsonValue, JsonView};

pub(crate) fn text(value: &JsonValue) -> Result<String, Error> {
    utf8(&openpilot_runtime_version::python_str(value)?)
}

fn utf8(points: &[u32]) -> Result<String, Error> {
    points
        .iter()
        .copied()
        .map(char::from_u32)
        .collect::<Option<String>>()
        .ok_or(Error::UnicodeText)
}

pub(crate) fn query_values(value: &JsonValue) -> Result<Vec<String>, Error> {
    let values = match value.view() {
        JsonView::Null => return Ok(Vec::new()),
        JsonView::Object(fields) => return fields.iter().map(|(key, _)| utf8(key)).collect(),
        JsonView::Array(values) => values,
        _ => vec![value.clone()],
    };
    let mut result = Vec::new();
    // Requests expands iterable values, then urllib.urlencode(doseq=True)
    // expands any iterable elements once more; only the first pass omits None.
    for value in values {
        match value.view() {
            JsonView::Null => {}
            JsonView::Array(values) => {
                for value in values {
                    result.push(text(&value)?);
                }
            }
            JsonView::Object(fields) => {
                for (key, _) in fields {
                    result.push(utf8(key)?);
                }
            }
            _ => result.push(text(&value)?),
        }
    }
    Ok(result)
}
