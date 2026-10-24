//! Python representation mechanics shared with the existing Forms formatter.
//! The Unicode table is Python character classification data, not an oracle.
use openpilot_log_format::{JsonValue, JsonView};
fn repr_text(points: &[u32]) -> String {
    static NONPRINTABLE: std::sync::OnceLock<Vec<[u32; 2]>> = std::sync::OnceLock::new();
    let nonprintable = NONPRINTABLE.get_or_init(|| {
        serde_json::from_str(include_str!(
            "../../tests/fixtures/ui-forms/repr-nonprintable.json"
        ))
        .expect("captured Python Unicode table")
    });
    let quote = if points.contains(&39) && !points.contains(&34) {
        '"'
    } else {
        '\''
    };
    let mut out = String::from(quote);
    for &point in points {
        if point == u32::from(quote) || point == 92 {
            out.push('\\');
            out.push(char::from_u32(point).unwrap());
        } else {
            match point {
                9 => out.push_str("\\t"),
                10 => out.push_str("\\n"),
                13 => out.push_str("\\r"),
                32..=126 => out.push(char::from_u32(point).unwrap()),
                _ if nonprintable
                    .iter()
                    .any(|&[lo, hi]| (lo..=hi).contains(&point)) =>
                {
                    if point <= 255 {
                        out.push_str(&format!("\\x{point:02x}"));
                    } else if point <= 0xffff {
                        out.push_str(&format!("\\u{point:04x}"));
                    } else {
                        out.push_str(&format!("\\U{point:08x}"));
                    }
                }
                _ => out.push(char::from_u32(point).unwrap_or('\u{fffd}')),
            }
        }
    }
    out.push(quote);
    out
}
pub(crate) fn repr(value: &JsonValue) -> String {
    enum Part {
        Value(JsonValue),
        Text(String),
    }
    let mut out = String::new();
    let mut pending = vec![Part::Value(value.clone())];
    while let Some(part) = pending.pop() {
        let Part::Value(value) = part else {
            if let Part::Text(text) = part {
                out.push_str(&text);
            }
            continue;
        };
        match value.view() {
            JsonView::Null => out.push_str("None"),
            JsonView::Bool(v) => out.push_str(if v { "True" } else { "False" }),
            JsonView::Integer(v) => out.push_str(v),
            JsonView::Float(v) if v.is_nan() => out.push_str("nan"),
            JsonView::Float(v) if v.is_infinite() => {
                out.push_str(if v > 0.0 { "inf" } else { "-inf" });
            }
            JsonView::Float(_) => out.push_str(&value.to_json().unwrap()),
            JsonView::Text(v) => out.push_str(&repr_text(v)),
            JsonView::Array(values) => {
                out.push('[');
                pending.push(Part::Text("]".into()));
                for (index, value) in values.into_iter().enumerate().rev() {
                    pending.push(Part::Value(value));
                    if index != 0 {
                        pending.push(Part::Text(", ".into()));
                    }
                }
            }
            JsonView::Object(values) => {
                out.push('{');
                pending.push(Part::Text("}".into()));
                for (index, (key, value)) in values.into_iter().enumerate().rev() {
                    pending.push(Part::Value(value));
                    pending.push(Part::Text(format!("{}: ", repr_text(key))));
                    if index != 0 {
                        pending.push(Part::Text(", ".into()));
                    }
                }
            }
        }
    }
    out
}
