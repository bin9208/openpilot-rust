//! Source comparator normalization, retaining Python Unicode and JSON equality.
use crate::seal::Result;
use openpilot_log_format::{JsonValue as J, JsonView};
use std::path::Path;
pub(super) fn array(values: &[J]) -> Result<J> {
    Ok(J::parse(&format!(
        "[{}]",
        values
            .iter()
            .map(J::to_json)
            .collect::<std::result::Result<Vec<_>, _>>()?
            .join(",")
    ))?)
}
pub(super) fn object(fields: &[(&str, J)]) -> Result<J> {
    object_points(
        fields
            .iter()
            .map(|(key, value)| (key.chars().map(u32::from).collect(), value.clone()))
            .collect(),
    )
}
fn object_points(fields: Vec<(Vec<u32>, J)>) -> Result<J> {
    Ok(J::parse(&format!(
        "{{{}}}",
        fields
            .into_iter()
            .map(|(key, value)| Ok(format!(
                "{}:{}",
                J::codepoints(key).ok_or("invalid JSON key")?.to_json()?,
                value.to_json()?
            )))
            .collect::<Result<Vec<_>>>()?
            .join(",")
    ))?)
}
pub(super) fn without_logs(value: &J) -> Result<J> {
    let JsonView::Object(fields) = value.view() else {
        return Err("source record is not an object".into());
    };
    object_points(
        fields
            .into_iter()
            .filter(|(key, _)| *key != [108, 111, 103, 115])
            .map(|(key, value)| (key.to_vec(), value))
            .collect(),
    )
}
fn replace(points: &[u32], old: &[u32], new: &[u32]) -> Vec<u32> {
    let mut out = Vec::new();
    let mut at = 0;
    while at < points.len() {
        if points[at..].starts_with(old) {
            out.extend_from_slice(new);
            at += old.len();
        } else {
            out.push(points[at]);
            at += 1;
        }
    }
    out
}
pub(super) fn normalize(value: &J, root: &Path) -> Result<J> {
    match value.view() {
        JsonView::Text(points) => {
            let replacement = "$FIXTURE".chars().map(u32::from).collect::<Vec<_>>();
            let mut points = points.to_vec();
            for side in ["source", "native"] {
                let old = crate::python_fs::codepoints(root.join(side).as_os_str());
                points = replace(&points, &old, &replacement);
            }
            Ok(J::codepoints(points).ok_or("invalid source string")?)
        }
        JsonView::Array(values) => array(
            &values
                .iter()
                .map(|v| normalize(v, root))
                .collect::<Result<Vec<_>>>()?,
        ),
        JsonView::Object(fields) => {
            let mut out = Vec::new();
            for (key, value) in fields {
                if key == [99, 114, 101, 97, 116, 101, 100] {
                    continue;
                }
                let value = normalize(&value, root)?;
                let value = if key == [118, 97, 108, 117, 101, 95, 106, 115, 111, 110] {
                    let JsonView::Text(points) = value.view() else {
                        return Err("SDK value_json is not text".into());
                    };
                    normalize(&J::parse_codepoints(points)?, root)?
                } else {
                    value
                };
                out.push((key.to_vec(), value));
            }
            object_points(out)
        }
        _ => Ok(value.clone()),
    }
}
pub(super) fn equal(a: &J, b: &J) -> bool {
    crate::control_tools_policy_check::json::equal(a, b)
}
fn sorted_key(value: &J) -> Result<String> {
    match value.view() {
        JsonView::Object(fields) => {
            let mut fields = fields;
            fields.sort_by(|(a, _), (b, _)| a.cmp(b));
            Ok(format!(
                "{{{}}}",
                fields
                    .into_iter()
                    .map(|(key, value)| Ok(format!(
                        "{}: {}",
                        J::codepoints(key.to_vec())
                            .ok_or("invalid key")?
                            .to_json()?,
                        sorted_key(&value)?
                    )))
                    .collect::<Result<Vec<_>>>()?
                    .join(", ")
            ))
        }
        JsonView::Array(values) => Ok(format!(
            "[{}]",
            values
                .iter()
                .map(sorted_key)
                .collect::<Result<Vec<_>>>()?
                .join(", ")
        )),
        _ => Ok(value.to_json()?),
    }
}
pub(super) fn cycle_logs(value: &J) -> Result<J> {
    let JsonView::Array(values) = value.view() else {
        return Err("source logs are not an array".into());
    };
    let mut values = values
        .into_iter()
        .map(|v| Ok((sorted_key(&v)?, v)))
        .collect::<Result<Vec<_>>>()?;
    values.sort_by(|(a, _), (b, _)| a.cmp(b));
    array(&values.into_iter().map(|(_, v)| v).collect::<Vec<_>>())
}
pub(super) fn expand(value: &J, root: &Path) -> Result<J> {
    match value.view() {
        JsonView::Text(points) => {
            let old = "$FIXTURE".chars().map(u32::from).collect::<Vec<_>>();
            Ok(J::codepoints(replace(
                points,
                &old,
                &crate::python_fs::codepoints(root.as_os_str()),
            ))
            .ok_or("invalid path string")?)
        }
        JsonView::Array(values) => array(
            &values
                .iter()
                .map(|v| expand(v, root))
                .collect::<Result<Vec<_>>>()?,
        ),
        JsonView::Object(fields) => object_points(
            fields
                .into_iter()
                .map(|(key, v)| Ok((key.to_vec(), expand(&v, root)?)))
                .collect::<Result<Vec<_>>>()?,
        ),
        _ => Ok(value.clone()),
    }
}
pub(super) fn scalar(text: &str) -> J {
    J::parse(text).expect("literal JSON")
}
