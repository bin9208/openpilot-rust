//! Python-compatible JSON equality and wire assembly, including normal(NaN).
use crate::{control_tools_policy_check::json::equal, seal::Result};
use openpilot_log_format::{JsonValue as J, JsonView};
pub(super) fn array(values: &[J]) -> Result<J> {
    let values = values
        .iter()
        .map(J::to_json)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(J::parse(&format!("[{}]", values.join(",")))?)
}
pub(super) fn object(fields: &[(&str, J)]) -> Result<J> {
    let values = fields
        .iter()
        .map(|(key, v)| Ok(format!("{}:{}", J::text(key).to_json()?, v.to_json()?)))
        .collect::<Result<Vec<String>>>()?;
    Ok(J::parse(&format!("{{{}}}", values.join(",")))?)
}
pub(super) fn without_logs(value: &J) -> Result<J> {
    let JsonView::Object(fields) = value.view() else {
        return Err("source response must be an object".into());
    };
    let fields = fields
        .into_iter()
        .filter(|(k, _)| *k != [108, 111, 103, 115])
        .map(|(k, v)| {
            Ok((
                J::codepoints(k.to_vec())
                    .ok_or("invalid source key")?
                    .to_json()?,
                v.to_json()?,
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(J::parse(&format!(
        "{{{}}}",
        fields
            .into_iter()
            .map(|(k, v)| format!("{k}:{v}"))
            .collect::<Vec<_>>()
            .join(",")
    ))?)
}
pub(super) fn normal_equal(a: &J, b: &J) -> bool {
    let mut pending = vec![(a.clone(), b.clone())];
    while let Some((mut a, mut b)) = pending.pop() {
        if matches!(a.view(),JsonView::Float(v) if v.is_nan()) {
            a = J::text("<NaN>");
        }
        if matches!(b.view(),JsonView::Float(v) if v.is_nan()) {
            b = J::text("<NaN>");
        }
        match (a.view(), b.view()) {
            (JsonView::Array(a), JsonView::Array(b)) => {
                if a.len() != b.len() {
                    return false;
                }
                pending.extend(a.into_iter().zip(b));
            }
            (JsonView::Object(a), JsonView::Object(b)) => {
                if a.len() != b.len() {
                    return false;
                }
                for (key, value) in a {
                    let Some((_, other)) = b.iter().find(|(k, _)| *k == key) else {
                        return false;
                    };
                    pending.push((value, other.clone()));
                }
            }
            _ => {
                if !equal(&a, &b) {
                    return false;
                }
            }
        }
    }
    true
}
