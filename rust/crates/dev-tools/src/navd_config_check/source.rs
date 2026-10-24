//! Independent RouteEngine initialization / Api policy over actual Params/files.
use super::{jwt, values as v};
use crate::seal::Result;
use openpilot_log_format::{JsonValue, JsonView};
use openpilot_logging::{
    log_site,
    producer::{Factory, Logger},
    record::{Level, Record},
};
use openpilot_params::Params;
use std::{
    fs::File,
    io::{self, BufRead, Read},
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
type Policy<T> = std::result::Result<T, String>;
fn repr(text: &str) -> String {
    crate::plot_samples_source::text_repr(&text.chars().map(u32::from).collect::<Vec<_>>())
}
fn null() -> JsonValue {
    JsonValue::parse("null").expect("null")
}
fn string(params: &Params, key: &str, logger: &mut Logger) -> Policy<Option<String>> {
    let raw = params.get(key).ok().flatten().unwrap_or_default();
    if raw.is_empty() {
        return Ok(None);
    }
    match String::from_utf8(raw.clone()) {
        Ok(s) => Ok(Some(s)),
        Err(_) => {
            let bytes_repr = bytes_repr(&raw);
            logger.emit(log_site!(),Record::text(Level::Warning,format!("Failed to cast param {key} with value={bytes_repr} from type t=<ParamKeyType.STRING: 0>"))).map_err(|e|format!("RuntimeError({})",repr(&e.to_string())))?;
            Ok(None)
        }
    }
}
fn integer(params: &Params) -> i32 {
    let raw = params.get("PrimeType").ok().flatten().unwrap_or_default();
    if raw.is_empty() {
        return 0;
    }
    let mut i = 0;
    while raw.get(i).is_some_and(|b| matches!(b, 9..=13 | 32)) {
        i += 1
    }
    let negative = raw.get(i) == Some(&b'-');
    if raw.get(i).is_some_and(|b| *b == b'-' || *b == b'+') {
        i += 1
    }
    let start = i;
    let mut n = 0i64;
    while let Some(b) = raw.get(i).filter(|b| b.is_ascii_digit()) {
        n = match n
            .checked_mul(10)
            .and_then(|n| n.checked_add(i64::from(*b - b'0')))
        {
            Some(n) => n,
            None => std::process::abort(),
        };
        i += 1
    }
    if i == start {
        std::process::abort()
    }
    let signed = if negative { -n } else { n };
    i32::try_from(signed).unwrap_or_else(|_| std::process::abort())
}
fn gps(params: &Params, logger: &mut Logger) -> Policy<()> {
    let Some(text) = string(params, "LastGPSPosition", logger)? else {
        return Ok(());
    };
    let position = JsonValue::parse(&text).map_err(|e| {
        let (offset, reason) = match e {
            openpilot_log_format::JsonError::Syntax { offset, reason } => (offset, reason),
            _ => return format!("ValueError({})", repr(&e.to_string())),
        };
        let message = match reason {
            "expected value" => "Expecting value",
            "expected colon" => "Expecting ':' delimiter",
            "expected array delimiter" | "expected object delimiter" => "Expecting ',' delimiter",
            "trailing data" => "Extra data",
            other => other,
        };
        let prefix = &text[..offset.min(text.len())];
        let line = prefix.bytes().filter(|&b| b == b'\n').count() + 1;
        let column = prefix.rsplit('\n').next().unwrap_or("").chars().count() + 1;
        format!(
            "JSONDecodeError({})",
            repr(&format!(
                "{message}: line {line} column {column} (char {})",
                prefix.chars().count()
            ))
        )
    })?;
    let contains = |name: &str| match position.view() {
        JsonView::Object(_) => position.get(name).is_some(),
        JsonView::Text(p) => {
            let needle = name.chars().map(u32::from).collect::<Vec<_>>();
            p.windows(needle.len()).any(|w| w == needle)
        }
        JsonView::Array(rows) => rows.iter().any(|v| v.text_eq(name)),
        _ => false,
    };
    match position.view() {
        JsonView::Null => Err("TypeError(\"argument of type 'NoneType' is not iterable\")".into()),
        JsonView::Bool(_) => Err("TypeError(\"argument of type 'bool' is not iterable\")".into()),
        JsonView::Integer(_) => Err("TypeError(\"argument of type 'int' is not iterable\")".into()),
        JsonView::Float(_) => Err("TypeError(\"argument of type 'float' is not iterable\")".into()),
        // Original coordinate_from_param calls pos.get before indexing.
        JsonView::Text(_) if contains("latitude") && contains("longitude") => {
            Err("AttributeError(\"'str' object has no attribute 'get'\")".into())
        }
        JsonView::Array(_) if contains("latitude") && contains("longitude") => {
            Err("AttributeError(\"'list' object has no attribute 'get'\")".into())
        }
        _ => Ok(()),
    }
}
enum ReadError {
    Io(io::Error),
    Decode(String),
}
impl From<io::Error> for ReadError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}
fn bytes_repr(raw: &[u8]) -> String {
    let quote = if raw.contains(&b'\'') && !raw.contains(&b'"') {
        b'"'
    } else {
        b'\''
    };
    let mut text = String::from("b");
    text.push(char::from(quote));
    for &b in raw {
        match b {
            b'\t' => text.push_str("\\t"),
            b'\n' => text.push_str("\\n"),
            b'\r' => text.push_str("\\r"),
            b'\\' => text.push_str("\\\\"),
            b if b == quote => {
                text.push('\\');
                text.push(char::from(b));
            }
            32..=126 => text.push(char::from(b)),
            _ => text.push_str(&format!("\\x{b:02x}")),
        }
    }
    text.push(char::from(quote));
    text
}
fn decode(raw: Vec<u8>) -> std::result::Result<String, ReadError> {
    String::from_utf8(raw)
        .map(|s| s.replace("\r\n", "\n").replace('\r', "\n"))
        .map_err(|e| {
            let raw = e.as_bytes();
            let error = e.utf8_error();
            let start = error.valid_up_to();
            let end = start + error.error_len().unwrap_or(raw.len() - start);
            let reason = if error.error_len().is_none() {
                "unexpected end of data"
            } else if (0xc2..=0xf4).contains(&raw[start]) {
                "invalid continuation byte"
            } else {
                "invalid start byte"
            };
            ReadError::Decode(format!(
                "UnicodeDecodeError('utf-8', {}, {start}, {end}, {})",
                bytes_repr(raw),
                repr(reason)
            ))
        })
}
fn read_pair(root: &Path) -> std::result::Result<Option<(&'static str, String)>, ReadError> {
    for (name, alg) in [("id_rsa", "RS256"), ("id_ecdsa", "ES256")] {
        let private = root.join("comma").join(name);
        let public = root.join("comma").join(format!("{name}.pub"));
        if private.is_file() && public.is_file() {
            // Both context managers are entered before either Unicode read.
            let mut private = File::open(private)?;
            let mut public = File::open(public)?;
            let mut bytes = Vec::new();
            private.read_to_end(&mut bytes)?;
            let private = decode(bytes)?;
            let mut bytes = Vec::new();
            public.read_to_end(&mut bytes)?;
            decode(bytes)?;
            return Ok(Some((alg, private)));
        }
    }
    Ok(None)
}
fn load(params: &Params, persist: &Path, logger: &mut Logger) -> Policy<JsonValue> {
    gps(params, logger)?;
    let (host, token) = if let Some(token) = std::env::var_os("MAPBOX_TOKEN") {
        (
            "https://api.mapbox.com",
            crate::python_fs::json_value(&token),
        )
    } else if integer(params) == 0 {
        (
            "https://api.mapbox.com",
            string(params, "MapboxPublicKey", logger)?.map_or_else(null, |s| JsonValue::text(&s)),
        )
    } else {
        let identity =
            string(params, "DongleId", logger)?.map_or_else(null, |s| JsonValue::text(&s));
        let pair = match read_pair(persist) {
            Ok(pair) => pair,
            Err(ReadError::Io(e)) if e.kind() == io::ErrorKind::NotFound => {
                logger.emit(log_site!(),Record::text(Level::Error,"Failed to generate mapbox token due to missing private key. Ensure device is registered.".into()).with_exception(e.to_string())).map_err(|e|format!("RuntimeError({})",repr(&e.to_string())))?;
                return v::object([
                    ("ok".into(), v::value(true).map_err(|e| e.to_string())?),
                    ("host".into(), JsonValue::text("https://maps.comma.ai")),
                    ("token".into(), JsonValue::text("")),
                ])
                .map_err(|e| e.to_string());
            }
            Err(ReadError::Io(e)) => return Err(format!("OSError({})", repr(&e.to_string()))),
            Err(ReadError::Decode(e)) => return Err(e),
        };
        let (algorithm, private) = pair
            .as_ref()
            .map_or(("none", ""), |(a, p)| (*a, p.as_str()));
        let seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_secs() as i64;
        let token = jwt::sign(algorithm, private, &identity, seconds).map_err(|_| {
            "InvalidKeyError('Could not parse the provided public key.')".to_owned()
        })?;
        ("https://maps.comma.ai", JsonValue::text(&token))
    };
    v::object([
        ("ok".into(), v::value(true).map_err(|e| e.to_string())?),
        ("host".into(), JsonValue::text(host)),
        ("token".into(), token),
    ])
    .map_err(|e| e.to_string())
}
pub(super) fn run() -> Result<()> {
    super::no_core()?;
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    let persist = v::path(&JsonValue::parse(line.trim_end())?)?;
    let params = Params::for_runtime()?;
    let factory = Factory::for_runtime()?;
    let result = match load(&params, &persist, &mut factory.logger()) {
        Ok(value) => value,
        Err(error) => v::object([
            ("ok".into(), v::value(false)?),
            ("error".into(), JsonValue::text(&error)),
        ])?,
    };
    println!("{}", result.to_json()?);
    Ok(())
}
