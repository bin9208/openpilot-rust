//! Lossless admission for the pinned shared-widget render helper.
//! Only fields consumed by this helper are projected into the drawing policy.
use crate::{control_tools_policy_check::json, seal::Result};
use openpilot_log_format::{JsonValue, JsonView};
use serde_json::{Map, Value};

pub struct Document {
    root: JsonValue,
    bootstrap: Value,
}
pub struct Element {
    pub kind: String,
    pub text: JsonValue,
    pub rect: crate::ui_forms_input::Rect,
    pub props: Value,
    pub value: JsonValue,
    original: JsonValue,
}
pub struct Event {
    pub x: JsonValue,
    pub y: JsonValue,
    pub slot: JsonValue,
    pub pressed: bool,
    pub released: bool,
}
impl Event {
    pub fn primary(&self) -> bool {
        json::equal(&self.slot, &JsonValue::parse("0").unwrap())
    }
    pub fn position(&self) -> Result<(f64, f64)> {
        fn coordinate(value: &JsonValue) -> Result<f64> {
            let number = match value.view() {
                JsonView::Float(value) => value,
                _ => number(value)?,
            };
            // Raylib's Vector2 stores C floats, including NaN and infinities.
            Ok(f64::from(number as f32))
        }
        Ok((coordinate(&self.x)?, coordinate(&self.y)?))
    }
    pub fn float_slot(&self) -> bool {
        matches!(self.slot.view(), JsonView::Float(_))
    }
}
fn required(value: &JsonValue, key: &str) -> Result<JsonValue> {
    value
        .get(key)
        .ok_or_else(|| format!("render required field {key} missing").into())
}
fn text(value: &JsonValue) -> Result<String> {
    value
        .to_utf8()
        .ok_or_else(|| "render selector must encode as Python text".into())
}
fn number(value: &JsonValue) -> Result<f64> {
    let result = match value.view() {
        JsonView::Bool(v) => f64::from(u8::from(v)),
        JsonView::Integer(v) => v.parse()?,
        JsonView::Float(v) => v,
        _ => return Err("render C argument must numeric".into()),
    };
    if !result.is_finite() {
        return Err("render nonfinite C argument".into());
    }
    Ok(result)
}
fn property(props: &JsonValue, key: &str) -> Result<Option<JsonValue>> {
    if !matches!(props.view(), JsonView::Object(_)) {
        return Err("render props must support get".into());
    }
    Ok(props.get(key))
}
fn float_text(points: &[u32]) -> Result<f64> {
    let text = crate::ui_forms_input::Input::utf8(points)?;
    let points = text
        .trim()
        .chars()
        .map(|ch| crate::argparse_scan::decimal_value(ch).map_or(ch, |v| char::from(b'0' + v)))
        .collect::<Vec<_>>();
    let mut normalized = String::new();
    for (index, &ch) in points.iter().enumerate() {
        if ch == '_' {
            if index == 0
                || !points[index - 1].is_ascii_digit()
                || !points.get(index + 1).is_some_and(char::is_ascii_digit)
            {
                return Err("render invalid SCALE separator".into());
            }
        } else {
            normalized.push(ch);
        }
    }
    Ok(match normalized.to_ascii_lowercase().as_str() {
        "inf" | "+inf" | "infinity" | "+infinity" => f64::INFINITY,
        "-inf" | "-infinity" => f64::NEG_INFINITY,
        "nan" | "+nan" => f64::NAN,
        "-nan" => -f64::NAN,
        _ => normalized.parse()?,
    })
}
fn multiply_admitted(left: &JsonValue, right: &JsonValue) -> bool {
    let numeric = |value: &JsonValue| {
        matches!(
            value.view(),
            JsonView::Bool(_) | JsonView::Integer(_) | JsonView::Float(_)
        )
    };
    let integer =
        |value: &JsonValue| matches!(value.view(), JsonView::Bool(_) | JsonView::Integer(_));
    let sequence =
        |value: &JsonValue| matches!(value.view(), JsonView::Text(_) | JsonView::Array(_));
    (numeric(left) && numeric(right))
        || (integer(left) && sequence(right))
        || (sequence(left) && integer(right))
}
impl Document {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let root = json::parse_text(std::str::from_utf8(bytes)?)?;
        let config = required(&root, "config")?;
        let raw_scale = required(&config, "scale")?;
        let scale = match raw_scale.view() {
            JsonView::Integer(v) => v.parse()?,
            JsonView::Float(v) => v,
            JsonView::Text(points) => float_text(points)?,
            _ => return Err("render SCALE=str(value) is not a float".into()),
        };
        let big = crate::ui_forms_input::python_truth(&required(&config, "big")?);
        let viewport = required(&config, "large_viewport")?;
        let language = root
            .get("language")
            .and_then(|v| v.to_utf8())
            .unwrap_or_else(|| "en".into());
        let bootstrap = serde_json::json!({"config":{"big":big,"large_viewport":crate::ui_forms_input::python_truth(&viewport),"scale":scale},"language":language});
        Ok(Self { root, bootstrap })
    }
    pub fn bootstrap(&self) -> &Value {
        &self.bootstrap
    }
    pub fn frame_count(&self) -> Result<usize> {
        Ok(match required(&self.root, "frames")?.view() {
            JsonView::Bool(v) => usize::from(v),
            JsonView::Integer(v) if v.starts_with('-') => 0,
            JsonView::Integer(v) => v
                .parse::<usize>()
                .map_err(|_| "render frame count exceeds address space")?,
            _ => return Err("render frames must Python integer".into()),
        })
    }
    pub fn elements(&self) -> Result<Vec<Element>> {
        let mut result = Vec::new();
        for element in json::sequence(&required(&self.root, "elements")?)? {
            let kind = text(&required(&element, "kind")?)?;
            let props = element.get("props").unwrap_or(JsonValue::parse("{}")?);
            if kind == "unified" {
                let size = property(&props, "size")?.unwrap_or(JsonValue::parse("60")?);
                let spacing = property(&props, "letter_spacing")?.unwrap_or(JsonValue::parse("0")?);
                if !multiply_admitted(&size, &spacing) {
                    return Err("original font_size multiplication is not supported".into());
                }
            } else if matches!(kind.as_str(), "label" | "button") {
                if let Some(size) = property(&props, "size")? {
                    if matches!(size.view(), JsonView::Array(_) | JsonView::Object(_)) {
                        return Err("original label size cache key is unhashable".into());
                    }
                }
                if kind == "label" {
                    if let Some(padding) = property(&props, "padding")? {
                        number(&padding)?;
                    }
                }
            }
            let mut projected = Map::new();
            // The real helper passes different constructor options per kind;
            // ignored props never cross a Unicode/number conversion boundary.
            let names: &[&str] = match kind.as_str() {
                "label" => &[
                    "size",
                    "font",
                    "padding",
                    "elide",
                    "horizontal",
                    "vertical",
                    "enabled",
                ],
                "unified" => &[
                    "size",
                    "font",
                    "padding",
                    "elide",
                    "wrap",
                    "scroll",
                    "shimmer",
                    "letter_spacing",
                    "line_height",
                    "horizontal",
                    "vertical",
                    "enabled",
                ],
                "button" => &["size", "style", "enabled"],
                "toggle" => &["enabled"],
                _ => return Err("render original widget kind missing".into()),
            };
            for &name in names {
                if let Some(value) = property(&props, name)? {
                    let value =
                        if matches!(name, "enabled" | "elide" | "wrap" | "scroll" | "shimmer") {
                            Value::Bool(crate::ui_forms_input::python_truth(&value))
                        } else if matches!(name, "font" | "style" | "horizontal" | "vertical") {
                            Value::String(text(&value)?)
                        } else {
                            // These numeric arguments may be sequences at construction.
                            // Keep their type invalid for later numeric consumption,
                            // without encoding a sequence's ignored surrogate contents.
                            match value.view() {
                                JsonView::Text(_) => {
                                    value.to_utf8().map(Value::String).unwrap_or(Value::Null)
                                }
                                JsonView::Array(_) | JsonView::Object(_) => Value::Null,
                                _ => serde_json::from_str(&value.to_json()?)?,
                            }
                        };
                    projected.insert(name.into(), value);
                }
            }
            let input = if kind == "toggle" {
                JsonValue::text("")
            } else {
                required(&element, "text")?
            };
            let rect = required(&element, "rect")?;
            let rect = crate::ui_forms_input::Rect {
                x: number(&required(&rect, "x")?)?,
                y: number(&required(&rect, "y")?)?,
                width: number(&required(&rect, "width")?)?,
                height: number(&required(&rect, "height")?)?,
            };
            let value = if kind == "toggle" {
                property(&props, "value")?.unwrap_or(JsonValue::parse("false")?)
            } else {
                JsonValue::parse("false")?
            };
            result.push(Element {
                kind,
                text: input,
                rect,
                props: Value::Object(projected),
                value,
                original: element,
            });
        }
        Ok(result)
    }
}
impl Element {
    pub fn events(&self) -> Result<Vec<Event>> {
        let Some(events) = self.original.get("events") else {
            return Ok(Vec::new());
        };
        let mut result = Vec::new();
        for event in json::sequence(&events)? {
            let pos = required(&event, "pos")?;
            // The source constructs every MouseEvent even when disabled.
            // NamedTuple construction preserves arbitrary values; only a
            // consumed primary collision converts the coordinates to C floats.
            let x = required(&pos, "x")?;
            let y = required(&pos, "y")?;
            let slot = required(&event, "slot")?;
            let pressed = required(&event, "pressed")?;
            let released = required(&event, "released")?;
            required(&event, "down")?;
            required(&event, "time")?;
            result.push(Event {
                x,
                y,
                slot,
                pressed: crate::ui_forms_input::python_truth(&pressed),
                released: crate::ui_forms_input::python_truth(&released),
            });
        }
        Ok(result)
    }
}
