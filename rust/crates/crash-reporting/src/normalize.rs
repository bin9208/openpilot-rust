//! The pinned Python SDK's max_value_length behavior for this adapter's JSON fields.
//! Tags retain their JSON types; project extras are flat strings, not arbitrary SDK databags.
use crate::Error;
use openpilot_logmessaged::{JsonValue, JsonView};

#[derive(Default)]
struct Meta {
    children: Vec<(Vec<u32>, Meta)>,
    annotation: Option<serde_json::Value>,
}
fn text(points: Vec<u32>) -> Result<JsonValue, Error> {
    JsonValue::codepoints(points).ok_or(Error::Unicode("JSON codepoint"))
}
impl Meta {
    fn annotate(&mut self, path: &[Vec<u32>], length: usize, limit: usize) {
        let Some((first, rest)) = path.split_first() else {
            self.annotation =
                Some(serde_json::json!({"len":length,"rem":[["!limit","x",limit-3,limit]]}));
            return;
        };
        let index = self
            .children
            .iter()
            .position(|(key, _)| key == first)
            .unwrap_or_else(|| {
                self.children.push((first.clone(), Self::default()));
                self.children.len() - 1
            });
        self.children[index].1.annotate(rest, length, limit);
    }
    fn json(&self) -> Result<String, Error> {
        let mut fields = self
            .children
            .iter()
            .map(|(key, value)| {
                Ok(format!(
                    "{}:{}",
                    text(key.clone())?.to_json()?,
                    value.json()?
                ))
            })
            .collect::<Result<Vec<_>, Error>>()?;
        if let Some(annotation) = &self.annotation {
            fields.push(format!("\"\":{annotation}"));
        }
        Ok(format!("{{{}}}", fields.join(",")))
    }
}
fn string(
    points: &[u32],
    path: &[Vec<u32>],
    limit: usize,
    meta: &mut Meta,
) -> Result<String, Error> {
    let byte_size = points
        .iter()
        .copied()
        .map(|point| char::from_u32(point).map(char::len_utf8))
        .sum::<Option<usize>>();
    let length = byte_size.unwrap_or(points.len());
    let value = if length > limit {
        let mut used = 0;
        let kept = if byte_size.is_some() {
            points
                .iter()
                .take_while(|&&point| {
                    used += char::from_u32(point).map_or(0, char::len_utf8);
                    used <= limit - 3
                })
                .copied()
                .collect::<Vec<_>>()
        } else {
            points.iter().take(limit - 3).copied().collect()
        };
        meta.annotate(path, length, limit);
        text(kept.into_iter().chain([46, 46, 46]).collect())?
    } else {
        text(points.to_vec())?
    };
    Ok(value.to_json()?)
}
fn render(
    value: &JsonValue,
    path: &mut Vec<Vec<u32>>,
    limit: usize,
    meta: &mut Meta,
) -> Result<String, Error> {
    match value.view() {
        JsonView::Text(points) => string(points, path, limit, meta),
        JsonView::Float(value_float) if !value_float.is_finite() => {
            Ok(text(openpilot_runtime_version::python_str(value)?)?.to_json()?)
        }
        JsonView::Null | JsonView::Bool(_) | JsonView::Integer(_) | JsonView::Float(_) => {
            Ok(value.to_json()?)
        }
        JsonView::Array(values) => {
            let mut items = Vec::new();
            for (index, value) in values.iter().enumerate() {
                path.push(index.to_string().chars().map(u32::from).collect());
                items.push(render(value, path, limit, meta)?);
                path.pop();
            }
            Ok(format!("[{}]", items.join(",")))
        }
        JsonView::Object(values) => {
            let mut fields = Vec::new();
            for (key, value) in values {
                path.push(key.to_vec());
                fields.push(format!(
                    "{}:{}",
                    text(key.to_vec())?.to_json()?,
                    render(&value, path, limit, meta)?
                ));
                path.pop();
            }
            Ok(format!("{{{}}}", fields.join(",")))
        }
    }
}
pub(crate) fn serialize(value: &JsonValue, limit: usize) -> Result<String, Error> {
    if limit < 3 || !value.is_object() {
        return Err(Error::Sdk {
            operation: "serialize",
            detail: "event object and limit >=3 required".into(),
        });
    }
    let mut meta = Meta::default();
    let mut result = render(value, &mut Vec::new(), limit, &mut meta)?;
    if !meta.children.is_empty() {
        result.pop();
        result.push_str(",\"_meta\":");
        result.push_str(&meta.json()?);
        result.push('}');
    }
    Ok(result)
}
