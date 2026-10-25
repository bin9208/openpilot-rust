//! Original image-FD projection and source/native event JSON conversion.
use super::values::*;
use crate::{control_tools_policy_check::json, seal::Result};
use openpilot_log_format::{JsonValue, JsonView};
pub fn images(records: &JsonValue) -> Result<(JsonValue, JsonValue)> {
    let mut identities: Vec<(JsonValue, u32)> = vec![];
    let mut result = vec![];
    for mut row in sequence(records)? {
        let op = get(&row, "op")?;
        if op.text_eq("image-import") || op.text_eq("vision-open") {
            let fd = get(&row, "fd")?;
            hashable(&fd)?;
            let id =
                if let Some((_, id)) = identities.iter().find(|(prior, _)| key_equal(prior, &fd)) {
                    *id
                } else {
                    let id = u32::try_from(identities.len() + 1)?;
                    identities.push((fd, id));
                    id
                };
            row = set(&row, "fd", number(id))?;
        }
        let op = get(&row, "op")?;
        if op.text_eq("close") || op.text_eq("vision-mmap") || op.text_eq("vision-munmap") {
            let fd = get(&row, "fd")?;
            hashable(&fd)?;
            if let Some((_, id)) = identities.iter().find(|(prior, _)| key_equal(prior, &fd)) {
                row = set(&row, "fd", number(id))?;
            }
        }
        if optional(&row, "name")?.text_eq("camera:275") {
            for key in ["before", "after"] {
                let mut payload = hex(&get(&row, key)?)?;
                let fd = number(word_bytes(&payload, 72));
                let id = identities
                    .iter()
                    .find(|(prior, _)| key_equal(prior, &fd))
                    .map(|(_, id)| *id)
                    .ok_or_else(|| error("KeyError", repr(&fd).unwrap()))?;
                // bytearray slice assignment appends when the start exceeds current length.
                let start = 72.min(payload.len());
                let end = 76.min(payload.len());
                payload.splice(start..end, id.to_le_bytes());
                row = set(&row, key, JsonValue::text(&hex_text(&payload)))?;
            }
        }
        result.push(row);
    }
    let mut keys = vec![];
    for (fd, id) in identities {
        let key = match fd.view() {
            JsonView::Text(_) => fd,
            JsonView::Bool(v) => JsonValue::text(if v { "true" } else { "false" }),
            JsonView::Null => JsonValue::text("null"),
            _ => JsonValue::text(&fd.to_json()?),
        };
        keys.push(format!("{}: {id}", key.to_json()?));
    }
    Ok((
        array(&result)?,
        JsonValue::parse(&format!("{{{}}}", keys.join(", ")))?,
    ))
}
// This validator runs only after the lossless value parser rejects input. It
// supplies CPython's reportable JSONDecodeError position/message, including its
// permissive numeric prefix scan (e.g. 1. is Extra data at the decimal point).
fn diagnostic(text: &str) -> (usize, &'static str) {
    let points = text.chars().collect::<Vec<_>>();
    let mut at = 0;
    #[derive(Clone, Copy)]
    enum Task {
        Value,
        ArrayFirst,
        ArrayDelimiter,
        ObjectFirst,
        ObjectKey,
        ObjectColon,
        ObjectDelimiter,
        End,
    }
    fn ws(points: &[char], at: &mut usize) {
        while points
            .get(*at)
            .is_some_and(|c| matches!(c, ' ' | '\t' | '\n' | '\r'))
        {
            *at += 1;
        }
    }
    fn string(points: &[char], at: &mut usize) -> std::result::Result<(), (usize, &'static str)> {
        let start = *at;
        *at += 1;
        loop {
            match points.get(*at) {
                None => return Err((start, "Unterminated string starting at")),
                Some('"') => {
                    *at += 1;
                    return Ok(());
                }
                Some('\\') => {
                    let slash = *at;
                    *at += 1;
                    match points.get(*at) {
                        None => return Err((start, "Unterminated string starting at")),
                        Some('u') => {
                            let u = *at;
                            for index in 1..=4 {
                                if !points.get(u + index).is_some_and(char::is_ascii_hexdigit) {
                                    return Err((u, "Invalid \\uXXXX escape"));
                                }
                            }
                            *at += 5;
                        }
                        Some('"' | '\\' | '/' | 'b' | 'f' | 'n' | 'r' | 't') => *at += 1,
                        _ => return Err((slash, "Invalid \\escape")),
                    }
                }
                Some(c) if *c < ' ' => return Err((*at, "Invalid control character at")),
                _ => *at += 1,
            }
        }
    }
    let mut tasks = vec![Task::End, Task::Value];
    while let Some(task) = tasks.pop() {
        ws(&points, &mut at);
        let ch = points.get(at).copied();
        match task {
            Task::Value => match ch {
                Some('[') => {
                    at += 1;
                    tasks.push(Task::ArrayFirst);
                }
                Some('{') => {
                    at += 1;
                    tasks.push(Task::ObjectFirst);
                }
                Some('"') => {
                    if let Err(e) = string(&points, &mut at) {
                        return e;
                    }
                }
                _ => {
                    let tail = points[at..].iter().collect::<String>();
                    if let Some(literal) = ["null", "true", "false", "NaN", "Infinity", "-Infinity"]
                        .into_iter()
                        .find(|v| tail.starts_with(v))
                    {
                        at += literal.len();
                        continue;
                    }
                    let start = at;
                    let mut end = at;
                    if points.get(end) == Some(&'-') {
                        end += 1;
                    }
                    match points.get(end) {
                        Some('0') => end += 1,
                        Some('1'..='9') => {
                            while points.get(end).is_some_and(char::is_ascii_digit) {
                                end += 1;
                            }
                        }
                        _ => return (start, "Expecting value"),
                    }
                    if points.get(end) == Some(&'.')
                        && points.get(end + 1).is_some_and(char::is_ascii_digit)
                    {
                        end += 2;
                        while points.get(end).is_some_and(char::is_ascii_digit) {
                            end += 1;
                        }
                    }
                    if matches!(points.get(end), Some('e' | 'E')) {
                        let mut exponent = end + 1;
                        if matches!(points.get(exponent), Some('+' | '-')) {
                            exponent += 1;
                        }
                        if points.get(exponent).is_some_and(char::is_ascii_digit) {
                            end = exponent + 1;
                            while points.get(end).is_some_and(char::is_ascii_digit) {
                                end += 1;
                            }
                        }
                    }
                    at = end;
                }
            },
            Task::ArrayFirst => {
                if ch == Some(']') {
                    at += 1;
                } else {
                    tasks.push(Task::ArrayDelimiter);
                    tasks.push(Task::Value);
                }
            }
            Task::ArrayDelimiter => match ch {
                Some(']') => at += 1,
                Some(',') => {
                    at += 1;
                    tasks.push(Task::ArrayDelimiter);
                    tasks.push(Task::Value);
                }
                _ => return (at, "Expecting ',' delimiter"),
            },
            Task::ObjectFirst | Task::ObjectKey => {
                if ch == Some('}') && matches!(task, Task::ObjectFirst) {
                    at += 1;
                } else {
                    if ch != Some('"') {
                        return (at, "Expecting property name enclosed in double quotes");
                    }
                    if let Err(e) = string(&points, &mut at) {
                        return e;
                    }
                    tasks.push(Task::ObjectColon);
                }
            }
            Task::ObjectColon => {
                if ch != Some(':') {
                    return (at, "Expecting ':' delimiter");
                }
                at += 1;
                tasks.push(Task::ObjectDelimiter);
                tasks.push(Task::Value);
            }
            Task::ObjectDelimiter => match ch {
                Some('}') => at += 1,
                Some(',') => {
                    at += 1;
                    tasks.push(Task::ObjectKey);
                }
                _ => return (at, "Expecting ',' delimiter"),
            },
            Task::End => {
                if ch.is_some() {
                    return (at, "Extra data");
                }
            }
        }
    }
    (at, "Expecting value")
}
fn decode(text: &str) -> Result<JsonValue> {
    match json::parse_text(text) {
        Ok(value) => Ok(value),
        Err(cause) => {
            if let Some(openpilot_log_format::JsonError::IntegerLimit) =
                cause.downcast_ref::<openpilot_log_format::JsonError>()
            {
                let digits = text
                    .split(|c: char| !c.is_ascii_digit())
                    .find(|word| word.len() > 4300)
                    .map_or(4301, str::len);
                return Err(error("ValueError",format!("Exceeds the limit (4300 digits) for integer string conversion: value has {digits} digits; use sys.set_int_max_str_digits() to increase the limit")));
            }
            if cause.to_string().contains("nesting limit") {
                return Err(error("RecursionError","maximum recursion depth exceeded while decoding a JSON array from a unicode string"));
            }
            let (at, message) = if text.starts_with('\u{feff}') {
                (0, "Unexpected UTF-8 BOM (decode using utf-8-sig)")
            } else {
                diagnostic(text)
            };
            let points = text.chars().take(at).collect::<String>();
            let line = points.chars().filter(|c| *c == '\n').count() + 1;
            let column = points.rsplit('\n').next().unwrap_or("").chars().count() + 1;
            Err(error(
                "JSONDecodeError",
                format!("{message}: line {line} column {column} (char {at})"),
            ))
        }
    }
}
pub fn result_value(stdout: &str, native: bool) -> Result<JsonValue> {
    let mut value = decode(stdout)?;
    if native {
        let mut events = vec![];
        for mut event in sequence(&get(&value, "events")?)? {
            let frame = get(&event, "frame")?;
            if !matches!(frame.view(), JsonView::Null) {
                let mut fields = vec![];
                for key in [
                    "slot",
                    "frame_id",
                    "request_id",
                    "timestamp_sof",
                    "timestamp_eof",
                ] {
                    fields.push(get(&frame, key)?);
                }
                let time = get(&frame, "processing_time")?;
                let time = match time.view() {
                    JsonView::Bool(v) => f64::from(u8::from(v)),
                    JsonView::Integer(v) => {
                        let n = v.parse::<f64>()?;
                        if n.is_infinite() {
                            return Err(error(
                                "OverflowError",
                                "int too large to convert to float",
                            ));
                        }
                        n
                    }
                    JsonView::Float(v) => v,
                    _ => return Err(error("error", "required argument is not a float")),
                };
                let narrow = time as f32;
                if time.is_finite() && narrow.is_infinite() {
                    return Err(error(
                        "OverflowError",
                        "float too large to pack with f format",
                    ));
                }
                fields.push(number(narrow.to_bits()));
                event = set(&event, "frame", array(&fields)?)?;
            }
            events.push(event);
        }
        value = set(&value, "events", array(&events)?)?;
    }
    Ok(value)
}
