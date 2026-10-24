//! Lossless JSON admission for the unchanged ui_forms_source.py caller.
//! Retain arbitrary integers and Python Unicode until the field is consumed;
//! serde is only a bridge for the existing typed drawing policy, not a decoder.
use crate::{control_tools_policy_check::json, seal::Result};
use openpilot_log_format::{JsonValue, JsonView};
use serde_json::{Map, Value};

pub struct Document {
    root: JsonValue,
}
pub struct Action {
    pub value: Value,
    pub events: Vec<JsonValue>,
    pub text_codepoints: Option<Vec<u32>>,
    pub text_value: Option<JsonValue>,
    pub selection: Option<JsonValue>,
}
pub fn truth(value: &JsonValue) -> bool {
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
fn required(value: &JsonValue, key: &str) -> Result<JsonValue> {
    value
        .get(key)
        .ok_or_else(|| format!("Forms required field {key} missing or not a mapping").into())
}
fn python_text(value: &JsonValue) -> Result<Vec<u32>> {
    match value.view() {
        JsonView::Text(points) => Ok(points.to_vec()),
        _ => Err("Forms text must Python string".into()),
    }
}
fn string(value: &JsonValue) -> Result<String> {
    value
        .to_utf8()
        .ok_or_else(|| "Forms accessed text is not UTF8-compatible Python text".into())
}
// Both Python slices and practical frame counters can only observe values up to
// addressable string/state lengths. Keep sign and integer-vs-float kind intact.
fn bounded_integer(value: &str) -> i64 {
    value.parse().unwrap_or(if value.starts_with('-') {
        i64::MIN
    } else {
        i64::MAX
    })
}
fn scalar(value: &JsonValue) -> Result<Value> {
    Ok(match value.view() {
        JsonView::Null => Value::Null,
        JsonView::Bool(value) => Value::Bool(value),
        JsonView::Integer(value) => bounded_integer(value).into(),
        JsonView::Float(value) => serde_json::Number::from_f64(value)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        JsonView::Text(_) => Value::String(string(value)?),
        // Unknown key/down/event scalar kinds do not equal integer key constants.
        JsonView::Array(_) | JsonView::Object(_) => Value::Null,
    })
}
fn numeric(value: &JsonValue) -> Result<f64> {
    let result = match value.view() {
        JsonView::Bool(value) => {
            if value {
                1.0
            } else {
                0.0
            }
        }
        JsonView::Integer(value) => value.parse()?,
        JsonView::Float(value) => value,
        _ => return Err("Forms C numeric argument is not a number".into()),
    };
    if !result.is_finite() {
        return Err("Forms nonfinite C numeric argument".into());
    }
    Ok(result)
}
fn float_text(points: &[u32]) -> Result<f64> {
    let text = points
        .iter()
        .copied()
        .map(char::from_u32)
        .collect::<Option<String>>()
        .ok_or("Forms scale contains a lone surrogate")?;
    // Python float strips Unicode whitespace, translates Unicode Nd digits and
    // permits an underscore only between two decimal digits.
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
                return Err("Forms scale has invalid float separator".into());
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
fn source_integer(value: &JsonValue) -> Result<f64> {
    match value.view() {
        JsonView::Bool(value) => Ok(if value { 1.0 } else { 0.0 }),
        JsonView::Integer(value) => Ok(value.parse()?),
        JsonView::Float(value) if value.is_finite() => Ok(value.trunc()),
        JsonView::Text(points) => {
            let text = points
                .iter()
                .copied()
                .map(char::from_u32)
                .collect::<Option<String>>()
                .ok_or("Forms integer contains lone surrogate")?;
            let text = text.trim();
            let (sign, body) = if let Some(body) = text.strip_prefix('-') {
                ("-", body)
            } else {
                ("", text.strip_prefix('+').unwrap_or(text))
            };
            let mut digits = String::new();
            let mut previous = false;
            for ch in body.chars() {
                if ch == '_' && previous {
                    previous = false;
                    continue;
                }
                let digit =
                    crate::argparse_scan::decimal_value(ch).ok_or("Forms invalid integer digit")?;
                digits.push(char::from(b'0' + digit));
                previous = true;
            }
            if !previous || digits.len() > 4300 {
                return Err("Forms integer conversion limit or separator".into());
            }
            Ok(format!("{sign}{digits}").parse()?)
        }
        _ => Err("Forms int() argument is invalid".into()),
    }
}
fn source_scale(value: &JsonValue) -> Result<f64> {
    // The source writes str(scene.config.scale) into SCALE before float(os.getenv).
    // bool therefore becomes "True"/"False" and is not a numeric 1/0 scale.
    match value.view() {
        JsonView::Integer(value) => Ok(value.parse()?),
        JsonView::Float(value) => Ok(value),
        JsonView::Text(points) => float_text(points),
        _ => Err("Forms scale str(value) is not a float".into()),
    }
}
impl Document {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        // Path.read_text(), then json.loads(str), rather than JSON bytes BOM/
        // UTF16 autodetection. Shared parser preserves ignored Unicode/depth.
        let text = std::str::from_utf8(bytes)?;
        Ok(Self {
            root: json::parse_text(text)?,
        })
    }
    fn configuration(&self) -> Result<Value> {
        let config = required(&self.root, "config")?;
        let scale = source_scale(&required(&config, "scale")?)?;
        let big = truth(&required(&config, "big")?);
        // Original assigns this value to stub hardware.TICI, but no module in
        // this pinned widget/import closure reads TICI. Preserve its required
        // key access; actual viewport uses HARDWARE(pc) or BIG exclusively.
        let large_viewport = truth(&required(&config, "large_viewport")?);
        Ok(serde_json::json!({"big":big,"large_viewport":large_viewport,"scale":scale}))
    }
    fn language(&self) -> String {
        self.root
            .get("language")
            .and_then(|v| v.to_utf8())
            .unwrap_or_else(|| "en".into())
    }
    /// init_window/startup profiling precedes widget, rect, frame and props access.
    pub fn bootstrap(&self) -> Result<Value> {
        Ok(serde_json::json!({"config":self.configuration()?,
            "kind":self.root.get("kind").and_then(|v|v.to_utf8()).unwrap_or_default(),
            "language":self.language(),"text":"","frames":0,"props":{},
            "rect":{"x":0,"y":0,"width":0,"height":0}}))
    }
    pub fn text_codepoints(&self) -> Result<Vec<u32>> {
        self.root
            .get("text")
            .map(|value| python_text(&value))
            .transpose()
            .map(|value| value.unwrap_or_default())
    }
    pub fn text_value(&self) -> JsonValue {
        self.root.get("text").unwrap_or_else(|| JsonValue::text(""))
    }
    pub fn property(&self, name: &str) -> Option<JsonValue> {
        self.root.get("props").and_then(|props| props.get(name))
    }
    pub fn description_codepoints(&self) -> Result<Vec<u32>> {
        match self.property("description") {
            None => Ok(Vec::new()),
            Some(value) if matches!(value.view(), JsonView::Null) => Ok(Vec::new()),
            Some(value) => python_text(&value),
        }
    }
    pub fn prepare(&self) -> Result<Value> {
        let config = self.configuration()?;
        let kind = string(&required(&self.root, "kind")?)?;
        let rect = required(&self.root, "rect")?;
        let mut rectangle = Map::new();
        for name in ["x", "y", "width", "height"] {
            rectangle.insert(name.into(), numeric(&required(&rect, name)?)?.into());
        }
        // Python values for titles/Mici remain untouched until the widget's
        // actual consumption boundary. String-only setters retain raw Unicode.
        if matches!(kind.as_str(), "html" | "confirm") {
            let _ = self.text_codepoints()?;
        }
        let text = String::new();
        // The source only tests language membership against its font fallback
        // names; an unrelated JSON value or lone surrogate cannot match them.
        let language = self.language();
        let frames = required(&self.root, "frames")?;
        let frames = match frames.view() {
            JsonView::Integer(value) => bounded_integer(value).into(),
            JsonView::Float(value) => {
                // Nonfinite frame counts still have float kind and fail range().
                serde_json::Number::from_f64(value)
                    .map(Value::Number)
                    .unwrap_or(Value::Null)
            }
            JsonView::Bool(value) => value.into(),
            _ => Value::Null,
        };
        let props = self.properties(&kind)?;
        Ok(serde_json::json!({"config":config,
            "kind":kind,"rect":rectangle,"text":text,"language":language,
            "frames":frames,"props":props,"actions":[]}))
    }
    fn properties(&self, kind: &str) -> Result<Value> {
        let Some(props) = self.root.get("props") else {
            return Ok(serde_json::json!({}));
        };
        if !matches!(props.view(), JsonView::Object(_)) {
            return Ok(Value::Null);
        }
        let mut result = Map::new();
        for name in [
            "show_touches",
            "password",
            "toggle",
            "center",
            "rich",
            "green",
        ] {
            if let Some(value) = props.get(name) {
                result.insert(name.into(), truth(&value).into());
            }
        }
        let names: &[&str] = match kind {
            "input" => &["max"],
            "html" => &["size"],
            "list" => &["action"],
            _ => &[],
        };
        for &name in names {
            if let Some(value) = props.get(name) {
                let value = if name == "size" {
                    let integer = source_integer(&value)?;
                    serde_json::Number::from_f64(integer)
                        .map(Value::Number)
                        .ok_or("Forms integer too large for C font argument")?
                } else if name == "action" {
                    let action = string(&value)?;
                    if !["toggle", "button", "text", "multiple"].contains(&action.as_str()) {
                        return Err("original Forms list action does not create an item".into());
                    }
                    action.into()
                } else {
                    scalar(&value)?
                };
                result.insert(name.into(), value);
            }
        }
        Ok(result.into())
    }
    pub fn actions(&self) -> Result<Vec<JsonValue>> {
        let value = self
            .root
            .get("actions")
            .unwrap_or_else(|| JsonValue::parse("[]").unwrap());
        json::sequence(&value)
    }
    pub fn frame_matches(action: &JsonValue, index: usize) -> Result<bool> {
        Ok(json::equal(
            &required(action, "frame")?,
            &JsonValue::parse(&index.to_string())?,
        ))
    }
    pub fn action(action: &JsonValue, kind: &str) -> Result<Action> {
        let mut result = Map::new();
        let mut text_codepoints = None;
        let mut text_value = None;
        let mut selection = None;
        if let Some(value) = action.get("key").filter(truth) {
            // Preserve a truthy unknown first key as an unrecognized numeric
            // sentinel: the original consumes it before the next queued key.
            let key = match value.view() {
                JsonView::Bool(_) | JsonView::Integer(_) => scalar(&value)?,
                JsonView::Float(v) if v.is_finite() && v.fract() == 0.0 => scalar(&value)?,
                _ => i64::MIN.into(),
            };
            result.insert("key".into(), key);
        }
        if let Some(value) = action.get("down") {
            let values = json::sequence(&value)?;
            result.insert(
                "down".into(),
                Value::Array(
                    values
                        .iter()
                        .map(|value| match value.view() {
                            JsonView::Bool(_) | JsonView::Integer(_) | JsonView::Float(_) => {
                                scalar(value)
                            }
                            _ => Ok(Value::Null),
                        })
                        .collect::<Result<_>>()?,
                ),
            );
        }
        let events = action
            .get("events")
            .map(|v| json::sequence(&v))
            .transpose()?
            .unwrap_or_default();
        let operation = action
            .get("operation")
            .and_then(|v| v.to_utf8())
            .unwrap_or_default();
        result.insert("operation".into(), operation.clone().into());
        match operation.as_str() {
            "character" => {
                let value = required(action, "text")?;
                let mut points = Vec::new();
                for character in json::sequence(&value)? {
                    let character = python_text(&character)?;
                    if character.len() != 1 {
                        return Err("Forms ord() needs a one-character Python string".into());
                    }
                    points.push(character[0]);
                }
                text_codepoints = Some(points);
            }
            "text" if kind != "html" => {
                text_value = Some(required(action, "text")?);
            }
            "key" if kind == "keyboard" => {
                text_value = Some(required(action, "text")?);
            }
            "text" | "key" => {
                let value = required(action, "text")?;
                text_codepoints = Some(python_text(&value)?);
            }
            "selection" => {
                let value = required(action, "text")?;
                if kind == "options" {
                    selection = Some(value);
                }
            }
            "cursor" => {
                let value = required(action, "value")?;
                // NaN/inf cursor comparisons cannot enter the source setter.
                if kind == "input" && matches!(value.view(),JsonView::Float(v) if !v.is_finite()) {
                    result.insert("operation".into(), "".into());
                } else {
                    result.insert("value".into(), scalar(&value)?);
                }
            }
            _ => {}
        }
        Ok(Action {
            value: result.into(),
            events,
            text_codepoints,
            text_value,
            selection,
        })
    }
    pub fn event(event: &JsonValue) -> Result<Value> {
        let pos = required(event, "pos")?;
        let mut result = Map::new();
        result.insert(
            "pos".into(),
            serde_json::json!({
                "x":scalar(&required(&pos,"x")?)?,
                "y":scalar(&required(&pos,"y")?)?
            }),
        );
        // A bool index is valid Python; a float with equal numeric value is
        // not a valid list index. Keep that kind for the dispatch boundary.
        result.insert("slot".into(), scalar(&required(event, "slot")?)?);
        for name in ["pressed", "released", "down"] {
            result.insert(name.into(), truth(&required(event, name)?).into());
        }
        result.insert("time".into(), scalar(&required(event, "time")?)?);
        Ok(result.into())
    }
}
