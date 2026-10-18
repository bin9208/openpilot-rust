//! Independent port of DebugPlot's extracted sampling gate and ring queues.
//! This module does not use the candidate UI Samples implementation.
use openpilot_log_format::{JsonValue, JsonView};
use std::{collections::VecDeque, fmt};

#[derive(Debug)]
pub struct SourceError {
    pub kind: &'static str,
    pub message: String,
}
impl fmt::Display for SourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.kind, self.message)
    }
}
impl std::error::Error for SourceError {}
type Result<T> = std::result::Result<T, SourceError>;
fn error(kind: &'static str, message: impl Into<String>) -> SourceError {
    SourceError {
        kind,
        message: message.into(),
    }
}
fn kind(value: &JsonValue) -> &'static str {
    match value.view() {
        JsonView::Null => "NoneType",
        JsonView::Bool(_) => "bool",
        JsonView::Integer(_) => "int",
        JsonView::Float(_) => "float",
        JsonView::Text(_) => "str",
        JsonView::Array(_) => "list",
        JsonView::Object(_) => "dict",
    }
}
fn truth(value: &JsonValue) -> bool {
    match value.view() {
        JsonView::Null => false,
        JsonView::Bool(v) => v,
        JsonView::Integer(v) => v != "0" && v != "-0",
        JsonView::Float(v) => v != 0.0,
        JsonView::Text(v) => !v.is_empty(),
        JsonView::Array(v) => !v.is_empty(),
        JsonView::Object(v) => !v.is_empty(),
    }
}
fn field(value: &JsonValue, key: &str) -> Result<JsonValue> {
    value
        .get(key)
        .ok_or_else(|| error("KeyError", format!("'{key}'")))
}
pub(crate) fn text_repr(points: &[u32]) -> String {
    static RANGES: std::sync::OnceLock<Vec<[u32; 2]>> = std::sync::OnceLock::new();
    let ranges = RANGES.get_or_init(|| {
        serde_json::from_str(include_str!(
            "../tests/fixtures/plot-samples/repr-nonprintable.json"
        ))
        .expect("original Unicode printability")
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
            continue;
        }
        match point {
            9 => out.push_str("\\t"),
            10 => out.push_str("\\n"),
            13 => out.push_str("\\r"),
            0..=31 | 127..=159 => out.push_str(&format!("\\x{point:02x}")),
            _ if ranges.iter().any(|&[lo, hi]| (lo..=hi).contains(&point)) => {
                if point <= 255 {
                    out.push_str(&format!("\\x{point:02x}"));
                } else if point <= 0xffff {
                    out.push_str(&format!("\\u{point:04x}"));
                } else {
                    out.push_str(&format!("\\U{point:08x}"));
                }
            }
            _ => out.push(char::from_u32(point).unwrap()),
        }
    }
    out.push(quote);
    out
}
fn float_text(points: &[u32]) -> Result<f64> {
    let invalid = || {
        error(
            "ValueError",
            format!("could not convert string to float: {}", text_repr(points)),
        )
    };
    let text = points
        .iter()
        .copied()
        .map(char::from_u32)
        .collect::<Option<String>>()
        .ok_or_else(invalid)?;
    let normalized = text
        .trim()
        .chars()
        .map(|ch| crate::argparse_scan::decimal_value(ch).map_or(ch, |v| char::from(b'0' + v)))
        .collect::<Vec<_>>();
    let mut text = String::new();
    for (index, &ch) in normalized.iter().enumerate() {
        if ch == '_' {
            if index == 0
                || !normalized[index - 1].is_ascii_digit()
                || !normalized.get(index + 1).is_some_and(char::is_ascii_digit)
            {
                return Err(invalid());
            }
        } else {
            text.push(ch);
        }
    }
    text.parse().map_err(|_| invalid())
}
fn number(value: &JsonValue) -> Result<f64> {
    match value.view() {
        JsonView::Float(v) => Ok(v),
        JsonView::Bool(v) => Ok(if v { 1.0 } else { 0.0 }),
        JsonView::Integer(v) => {
            let n = v
                .parse::<f64>()
                .map_err(|_| error("OverflowError", "int too large to convert to float"))?;
            if n.is_infinite() {
                Err(error("OverflowError", "int too large to convert to float"))
            } else {
                Ok(n)
            }
        }
        JsonView::Text(v) => float_text(v),
        _ => Err(error(
            "TypeError",
            format!(
                "float() argument must be a string or a real number, not '{}'",
                kind(value)
            ),
        )),
    }
}
fn series_value(value: &JsonValue, index: usize) -> Result<JsonValue> {
    match value.view() {
        JsonView::Array(v) => v
            .get(index)
            .cloned()
            .ok_or_else(|| error("IndexError", "list index out of range")),
        JsonView::Text(v) => v
            .get(index)
            .and_then(|p| JsonValue::codepoints(vec![*p]))
            .ok_or_else(|| error("IndexError", "string index out of range")),
        JsonView::Object(_) => Err(error("KeyError", index.to_string())),
        _ => Err(error(
            "TypeError",
            format!("'{}' object is not subscriptable", kind(value)),
        )),
    }
}
#[derive(Default)]
struct Bounds {
    minimum: VecDeque<(i64, f64)>,
    maximum: VecDeque<(i64, f64)>,
}
impl Bounds {
    fn push(&mut self, time: i64, value: f64, oldest: i64) {
        while self.minimum.back().is_some_and(|&(_, v)| v > value) {
            self.minimum.pop_back();
        }
        self.minimum.push_back((time, value));
        while self.maximum.back().is_some_and(|&(_, v)| v < value) {
            self.maximum.pop_back();
        }
        self.maximum.push_back((time, value));
        while self.minimum.front().is_some_and(|&(t, _)| t < oldest) {
            self.minimum.pop_front();
        }
        while self.maximum.front().is_some_and(|&(t, _)| t < oldest) {
            self.maximum.pop_front();
        }
    }
}
struct Plot {
    size: usize,
    index: usize,
    time: i64,
    last: Option<f64>,
    minimum: f64,
    maximum: f64,
    values: [[f64; 300]; 3],
    bounds: [Bounds; 3],
}
impl Default for Plot {
    fn default() -> Self {
        Self {
            size: 0,
            index: 299,
            time: -1,
            last: None,
            minimum: 0.0,
            maximum: 0.0,
            values: [[0.0; 300]; 3],
            bounds: std::array::from_fn(|_| Bounds::default()),
        }
    }
}
fn float_json(value: f64) -> String {
    if value.is_nan() {
        "NaN".into()
    } else if value == f64::INFINITY {
        "Infinity".into()
    } else if value == f64::NEG_INFINITY {
        "-Infinity".into()
    } else {
        format!("{value:?}")
    }
}
impl Plot {
    fn update(&mut self, values: &JsonValue) -> Result<()> {
        self.time += 1;
        self.index = (self.index + 1) % 300;
        self.size = (self.size + 1).min(300);
        let oldest = self.time - (self.size as i64 - 1);
        for i in 0..3 {
            let v = number(&series_value(values, i)?)?;
            self.values[i][self.index] = v;
            self.bounds[i].push(self.time, v, oldest);
        }
        let mut mn = self.bounds[0].minimum[0].1;
        let mut mx = self.bounds[0].maximum[0].1;
        // Python min/max retain the first value on equality or unordered NaN.
        for bound in &self.bounds[1..] {
            let lo = bound.minimum[0].1;
            let hi = bound.maximum[0].1;
            if lo < mn {
                mn = lo;
            }
            if hi > mx {
                mx = hi;
            }
        }
        if mn > -2.0 {
            mn = -2.0;
        }
        if mx < 2.0 {
            mx = 2.0;
        }
        self.minimum = mn;
        self.maximum = mx;
        Ok(())
    }
    fn step(&mut self, step: &JsonValue) -> Result<String> {
        if !step.is_object() {
            return Err(error(
                "AttributeError",
                format!("'{}' object has no attribute 'get'", kind(step)),
            ));
        }
        if step.get("reset").as_ref().is_some_and(truth) {
            *self = Self::default();
        }
        let now = number(&field(step, "now")?)?;
        let sample = self.last.is_none_or(|last| now - last >= 0.05);
        if sample {
            if let Some(last) = self.last {
                let steps = (now - last) / 0.05;
                if steps.is_infinite() {
                    return Err(error(
                        "OverflowError",
                        "cannot convert float infinity to integer",
                    ));
                }
                if steps.is_nan() {
                    return Err(error("ValueError", "cannot convert float NaN to integer"));
                }
                self.last = Some(last + steps.trunc().clamp(1.0, 5.0) * 0.05);
            } else {
                self.last = Some(now);
            }
        }
        let values = field(step, "values")?;
        if sample {
            self.update(&values)?;
        }
        let latest = (0..3)
            .map(|s| float_json(self.values[s][self.index]))
            .collect::<Vec<_>>()
            .join(", ");
        let history = if step.get("history").as_ref().is_some_and(truth) {
            format!(
                "[{}]",
                (0..3)
                    .map(|s| format!(
                        "[{}]",
                        (0..self.size)
                            .map(|back| float_json(self.values[s][(self.index + 300 - back) % 300]))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        } else {
            "null".into()
        };
        Ok(format!("{{\"sample\": {{\"size\": {}, \"index\": {}, \"time\": {}, \"minimum\": {}, \"maximum\": {}, \"last_sample\": {}, \"latest\": [{latest}]}}, \"history\": {history}}}",self.size,if self.size==0{-1}else{self.index as i64},self.time,float_json(self.minimum),float_json(self.maximum),self.last.map(float_json).unwrap_or_else(||"null".into())))
    }
}
/// Execute the generic original helper's independently ported sampling behavior.
pub fn render(steps: &JsonValue) -> Result<JsonValue> {
    let steps = match steps.view() {
        JsonView::Array(v) => v,
        JsonView::Text([]) => vec![],
        JsonView::Object(v) if v.is_empty() => vec![],
        JsonView::Text(_) | JsonView::Object(_) => {
            return Err(error(
                "AttributeError",
                "'str' object has no attribute 'get'",
            ))
        }
        _ => {
            return Err(error(
                "TypeError",
                format!("'{}' object is not iterable", kind(steps)),
            ))
        }
    };
    let mut plot = Plot::default();
    let mut outputs = Vec::with_capacity(steps.len());
    for step in steps {
        outputs.push(plot.step(&step)?);
    }
    JsonValue::parse(&format!("[{}]", outputs.join(", ")))
        .map_err(|e| error("ValueError", e.to_string()))
}
