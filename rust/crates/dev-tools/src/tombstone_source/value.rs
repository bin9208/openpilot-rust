//! Generic Python value mechanics for the independent source, not runtime policy.
use super::{Error, Result};
use openpilot_log_format::{JsonValue as J, JsonView};
pub(super) fn text(points: Vec<u32>) -> J {
    J::codepoints(points).expect("valid Python codepoints")
}
pub(super) fn points(value: &J, kind: &'static str) -> Result<Vec<u32>> {
    match value.view() {
        JsonView::Text(points) => Ok(points.to_vec()),
        _ => Err(Error::new(kind, "expected str")),
    }
}
pub(super) fn field(value: &J, key: &str) -> Result<J> {
    if !value.is_object() {
        return Err(Error::new(
            "TypeError",
            "string-key subscription requires a mapping",
        ));
    }
    value.get(key).ok_or_else(|| Error::new("KeyError", key))
}
pub(super) fn parse(points: &[u32]) -> Result<J> {
    J::parse_codepoints(points).map_err(|error| {
        Error::new(
            match &error {
                openpilot_log_format::JsonError::IntegerLimit => "ValueError",
                _ => "JSONDecodeError",
            },
            error.to_string(),
        )
    })
}
pub(super) fn truth(value: &J) -> bool {
    match value.view() {
        JsonView::Null => false,
        JsonView::Bool(value) => value,
        JsonView::Integer(value) => value != "0" && value != "-0",
        JsonView::Float(value) => value != 0.0,
        JsonView::Text(value) => !value.is_empty(),
        JsonView::Array(value) => !value.is_empty(),
        JsonView::Object(value) => !value.is_empty(),
    }
}
// Assemble decoded points, so two independently stored adjacent surrogates do
// not combine merely because a response container is constructed.
fn json_points(value: &J) -> Vec<u32> {
    enum Part {
        Node(J),
        Text(Vec<u32>),
        Token(&'static str),
    }
    let mut pending = vec![Part::Node(value.clone())];
    let mut out = Vec::new();
    while let Some(part) = pending.pop() {
        let value = match part {
            Part::Node(value) => value,
            Part::Token(token) => {
                out.extend(token.chars().map(u32::from));
                continue;
            }
            Part::Text(points) => {
                out.push(34);
                for point in points {
                    match point {
                        34 | 92 => {
                            out.push(92);
                            out.push(point);
                        }
                        0..=31 => out.extend(format!("\\u{point:04x}").chars().map(u32::from)),
                        _ => out.push(point),
                    }
                }
                out.push(34);
                continue;
            }
        };
        match value.view() {
            JsonView::Text(points) => pending.push(Part::Text(points.to_vec())),
            JsonView::Array(values) => {
                out.push(91);
                pending.push(Part::Token("]"));
                for (index, value) in values.into_iter().enumerate().rev() {
                    pending.push(Part::Node(value));
                    if index > 0 {
                        pending.push(Part::Token(","));
                    }
                }
            }
            JsonView::Object(values) => {
                out.push(123);
                pending.push(Part::Token("}"));
                for (index, (key, value)) in values.into_iter().enumerate().rev() {
                    pending.push(Part::Node(value));
                    pending.push(Part::Token(":"));
                    pending.push(Part::Text(key.to_vec()));
                    if index > 0 {
                        pending.push(Part::Token(","));
                    }
                }
            }
            _ => out.extend(value.to_json().expect("scalar JSON").chars().map(u32::from)),
        }
    }
    out
}
pub(super) fn object(fields: &[(&str, J)]) -> J {
    let mut points = vec![123];
    for (index, (key, value)) in fields.iter().enumerate() {
        if index > 0 {
            points.push(44);
        }
        points.extend(json_points(&J::text(key)));
        points.push(58);
        points.extend(json_points(value));
    }
    points.push(125);
    J::parse_codepoints(&points).expect("valid source object")
}
pub(super) fn array(values: &[J]) -> J {
    let mut points = vec![91];
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            points.push(44);
        }
        points.extend(json_points(value));
    }
    points.push(93);
    J::parse_codepoints(&points).expect("valid source array")
}
pub(super) fn capture(value: Result<J>) -> J {
    match value {
        Ok(value) => object(&[("value", value)]),
        Err(error) => object(&[("error", J::text(error.kind()))]),
    }
}
/// Python str()/container repr() use the existing independent generic string
/// quoting and Unicode-printability mechanic, not any candidate tombstone code.
pub(super) fn string(value: &J) -> Vec<u32> {
    enum Part {
        Node(J, bool),
        Token(String),
    }
    let mut pending = vec![Part::Node(value.clone(), true)];
    let mut out = Vec::new();
    while let Some(part) = pending.pop() {
        let (value, top) = match part {
            Part::Node(value, top) => (value, top),
            Part::Token(token) => {
                out.extend(token.chars().map(u32::from));
                continue;
            }
        };
        match value.view() {
            JsonView::Text(points) => {
                if top {
                    out.extend_from_slice(points);
                } else {
                    out.extend(
                        crate::plot_samples_source::text_repr(points)
                            .chars()
                            .map(u32::from),
                    );
                }
            }
            JsonView::Null => out.extend("None".chars().map(u32::from)),
            JsonView::Bool(value) => {
                out.extend(if value { "True" } else { "False" }.chars().map(u32::from))
            }
            JsonView::Integer(value) => out.extend(value.chars().map(u32::from)),
            JsonView::Float(number) => {
                let token = if number.is_nan() {
                    "nan".into()
                } else if number == f64::INFINITY {
                    "inf".into()
                } else if number == f64::NEG_INFINITY {
                    "-inf".into()
                } else {
                    value.to_json().expect("float JSON")
                };
                out.extend(token.chars().map(u32::from));
            }
            JsonView::Array(values) => {
                out.push(91);
                pending.push(Part::Token("]".into()));
                for (index, value) in values.into_iter().enumerate().rev() {
                    pending.push(Part::Node(value, false));
                    if index > 0 {
                        pending.push(Part::Token(", ".into()));
                    }
                }
            }
            JsonView::Object(values) => {
                out.push(123);
                pending.push(Part::Token("}".into()));
                for (index, (key, value)) in values.into_iter().enumerate().rev() {
                    pending.push(Part::Node(value, false));
                    pending.push(Part::Token(": ".into()));
                    pending.push(Part::Token(crate::plot_samples_source::text_repr(key)));
                    if index > 0 {
                        pending.push(Part::Token(", ".into()));
                    }
                }
            }
        }
    }
    out
}
