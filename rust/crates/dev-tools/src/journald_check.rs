//! Independent policy of the retained journald QA source.
//! Does not call the candidate journald decoder or packet builder.
use crate::argparse_scan::decimal_value;
#[cfg(not(feature = "camera-sensor-native"))]
#[path = "camera_random.rs"]
mod random;
#[cfg(feature = "camera-sensor-native")]
pub use crate::camera_sensor_check::Random;
use num_bigint::BigInt;
use openpilot_log_format::{JsonValue, JsonView};
#[cfg(not(feature = "camera-sensor-native"))]
pub use random::Random;
use std::fmt;
#[derive(Debug)]
pub struct PolicyError {
    pub kind: &'static str,
    pub message: String,
}
impl fmt::Display for PolicyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.kind, self.message)
    }
}
impl std::error::Error for PolicyError {}
type Result<T> = std::result::Result<T, PolicyError>;
fn error(kind: &'static str, message: impl Into<String>) -> PolicyError {
    PolicyError {
        kind,
        message: message.into(),
    }
}
fn typename(value: &JsonValue) -> &'static str {
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
fn repr(points: &[u32]) -> String {
    let quote = if points.contains(&39) && !points.contains(&34) {
        '"'
    } else {
        '\''
    };
    let mut text = String::from(quote);
    for &point in points {
        if point == u32::from(quote) || point == 92 {
            text.push('\\');
            text.push(char::from_u32(point).unwrap());
            continue;
        }
        match point {
            9 => text.push_str("\\t"),
            10 => text.push_str("\\n"),
            13 => text.push_str("\\r"),
            0..=31 | 127..=159 => text.push_str(&format!("\\x{point:02x}")),
            0xd800..=0xdfff => text.push_str(&format!("\\u{point:04x}")),
            _ => text.push(char::from_u32(point).unwrap()),
        }
    }
    text.push(quote);
    text
}
fn integer(value: &JsonValue) -> Result<String> {
    match value.view() {
        JsonView::Integer(text) => Ok(text.into()),
        JsonView::Bool(value) => Ok(if value { "1" } else { "0" }.into()),
        JsonView::Float(value) if value.is_nan() => {
            Err(error("ValueError", "cannot convert float NaN to integer"))
        }
        JsonView::Float(value) if value.is_infinite() => Err(error(
            "OverflowError",
            "cannot convert float infinity to integer",
        )),
        JsonView::Float(value) => Ok(format!("{:.0}", value.trunc())
            .parse::<BigInt>()
            .map_err(|_| error("ValueError", "invalid finite integer"))?
            .to_string()),
        JsonView::Text(points) => {
            let invalid = || {
                error(
                    "ValueError",
                    format!("invalid literal for int() with base 10: {}", repr(points)),
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
                .map(|ch| decimal_value(ch).map_or(ch, |digit| char::from(b'0' + digit)))
                .collect::<Vec<_>>();
            let digits = normalized.iter().filter(|c| c.is_ascii_digit()).count();
            if digits > 4300 {
                return Err(error("ValueError",format!("Exceeds the limit (4300 digits) for integer string conversion: value has {digits} digits; use sys.set_int_max_str_digits() to increase the limit")));
            }
            let mut output = String::new();
            for (i, &c) in normalized.iter().enumerate() {
                if c == '_' {
                    if i == 0
                        || !normalized[i - 1].is_ascii_digit()
                        || !normalized.get(i + 1).is_some_and(char::is_ascii_digit)
                    {
                        return Err(invalid());
                    }
                } else if c.is_ascii_digit() || i == 0 && matches!(c, '+' | '-') {
                    output.push(c)
                } else {
                    return Err(invalid());
                }
            }
            output
                .parse::<BigInt>()
                .map(|v| v.to_string())
                .map_err(|_| invalid())
        }
        _ => Err(error(
            "TypeError",
            format!(
                "int() argument must be a string, a bytes-like object or a real number, not '{}'",
                typename(value)
            ),
        )),
    }
}
fn parse(text: &str) -> Result<JsonValue> {
    JsonValue::parse(text).map_err(|e|match e {
        openpilot_log_format::JsonError::IntegerLimit=>{
            // CPython reports the actual token length, excluding its sign.
            let digits=text.split(|c:char|!c.is_ascii_digit()).map(str::len).max().unwrap_or(0);
            error("ValueError",format!("Exceeds the limit (4300 digits) for integer string conversion: value has {digits} digits; use sys.set_int_max_str_digits() to increase the limit"))
        }
        _=>error("JSONDecodeError",e.to_string()),
    })
}
/// Original checker's entry() including arbitrary Python ints and optional tag.
/// Schema narrowing is deliberately a separate actual wire validation stage.
pub fn entry_json(text: &str) -> Result<String> {
    let value = parse(text)?;
    if !matches!(value.view(), JsonView::Object(_)) {
        return Err(error(
            "AttributeError",
            format!("'{}' object has no attribute 'get'", typename(&value)),
        ));
    }
    let zero = JsonValue::parse("0").unwrap();
    let ts = integer(
        &value
            .get("__REALTIME_TIMESTAMP")
            .unwrap_or_else(|| zero.clone()),
    )?;
    let priority = integer(&value.get("PRIORITY").unwrap_or_else(|| zero.clone()))?;
    let pid = integer(&value.get("_PID").unwrap_or(zero))?;
    let tag = value
        .get("SYSLOG_IDENTIFIER")
        .map(|v| v.to_json().map(|s| format!(", \"tag\": {s}")))
        .transpose()
        .map_err(|e| error("ValueError", e.to_string()))?
        .unwrap_or_default();
    let message = value
        .to_json()
        .map_err(|e| error("ValueError", e.to_string()))?;
    let message = JsonValue::text(&message)
        .to_json()
        .map_err(|e| error("ValueError", e.to_string()))?;
    Ok(format!("{{\"valid\": false, \"androidLog\": {{\"id\": 0, \"ts\": {ts}, \"priority\": {priority}, \"pid\": {pid}, \"tid\": 0{tag}, \"message\": {message}}}}}"))
}
/// Same MT19937 draw protocol as Random(65), generated at runtime, never loaded
/// from a fixture. Six consecutive 64-bit draws equal one little-endian384 draw.
pub fn valid_records() -> Result<Vec<String>> {
    let mut records=vec![
        "{}".into(),
        r#"{"MESSAGE":"plain", "__REALTIME_TIMESTAMP":"1723456789012345", "_PID":"123", "PRIORITY":"6", "SYSLOG_IDENTIFIER":"kernel"}"#.into(),
        r#"{"z":"한😀\n\t\u0000", "a":[0,255,{"b":true,"a":null}], "float":181449526438435.12, "negzero":-0.0}"#.into(),
        r#"{"z":1,"a":2,"z":3,"_PID":-2147483648,"PRIORITY":255,"__REALTIME_TIMESTAMP":18446744073709551615}"#.into(),
        r#"{"_PID":1.9,"PRIORITY":true,"__REALTIME_TIMESTAMP":12.8,"SYSLOG_IDENTIFIER":"한"}"#.into(),
        r#"{"_PID":" +1_234 ","PRIORITY":"٠٧","__REALTIME_TIMESTAMP":"１２３"}"#.into(),
        r#"{"large":1234567890123456789012345678901234567890,"tiny":1e-7,"huge":1e300}"#.into(),
        r#"{"special":[NaN,Infinity,-Infinity,1e999],"surrogate":"\ud800"}"#.into(),
        r#"{"\ud800":1,"slash":"\/","pair":"\ud800\udc00","negative":-0,"SYSLOG_IDENTIFIER":""}"#.into(),
        format!("{{\"nested\":{}0{}}}","[".repeat(5000),"]".repeat(5000)),
    ];
    let mut rng = Random::new(65);
    for index in 0..128 {
        let number = f64::from_bits(rng.getrandbits(64));
        let number = if number.is_nan() {
            "NaN".into()
        } else if number == f64::INFINITY {
            "Infinity".into()
        } else if number == f64::NEG_INFINITY {
            "-Infinity".into()
        } else {
            number.to_string()
        };
        let number = if number.contains(['.', 'e', 'E'])
            || matches!(number.as_str(), "NaN" | "Infinity" | "-Infinity")
        {
            number
        } else {
            number + ".0"
        };
        let mut bytes = vec![];
        for _ in 0..6 {
            bytes.extend(rng.getrandbits(64).to_le_bytes())
        }
        let wide = num_bigint::BigUint::from_bytes_le(&bytes);
        let raw=format!("{{\"first\": {index}, \"number\": {number}, \"wide\": {wide}, \"nested\": [null, true, false, \"한😀\\u0000\\u007f\\udfff\", {{\"z\": 1, \"a\": 2}}]}}");
        records.push(
            parse(&raw)?
                .to_json()
                .map_err(|e| error("ValueError", e.to_string()))?,
        );
    }
    Ok(records)
}
pub fn malformed_records() -> Vec<Vec<u8>> {
    [
        b"{bad\n".as_slice(),
        b"{\"a\":]\n",
        b"{\"x\":1} trailing\n",
        b"{\"x\":01}\n",
        b"{\"x\":+1}\n",
        b"{\"x\":.1}\n",
        b"{\"x\":1.}\n",
        b"{\"x\":true,}\n",
        b"{\"x\":[1,]}\n",
        b"{\"x\":\"\\q\"}\n",
        b"{\"x\":\"\\uZZZZ\"}\n",
        b"{\"x\":Inf}\n",
        b"{\"x\":\"literal\x01\"}\n",
    ]
    .iter()
    .map(|v| v.to_vec())
    .collect()
}
pub fn fatal_records() -> Vec<Vec<u8>> {
    let mut records = [
        "[]",
        "null",
        r#"{"_PID":null}"#,
        r#"{"_PID":"1.5"}"#,
        r#"{"_PID":2147483648}"#,
        r#"{"PRIORITY":256}"#,
        r#"{"PRIORITY":-1}"#,
        r#"{"__REALTIME_TIMESTAMP":-1}"#,
        r#"{"__REALTIME_TIMESTAMP":18446744073709551616}"#,
        r#"{"SYSLOG_IDENTIFIER":4}"#,
        r#"{"_PID":NaN}"#,
        r#"{"_PID":{}}"#,
        r#"{"_PID":"1__2"}"#,
        r#"{"SYSLOG_IDENTIFIER":"\ud800"}"#,
        r#"{"_PID":"²"}"#,
        r#"{"_PID":"\u001c1"}"#,
    ]
    .iter()
    .map(|v| v.as_bytes().to_vec())
    .collect::<Vec<_>>();
    records.push(format!("{{\"x\":{}}}", "1".repeat(4301)).into_bytes());
    records.extend([
        b"{\"x\":\"\xff\"}".to_vec(),
        b"{\"x\":\"\xed\xa0\x80\"}".to_vec(),
    ]);
    records
}
