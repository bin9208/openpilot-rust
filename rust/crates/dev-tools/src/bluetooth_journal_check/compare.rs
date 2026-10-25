use crate::{control_tools_policy_check::json as python_json, seal::Result};
use openpilot_log_format::{JsonValue, JsonView};
use std::collections::{BTreeMap, BTreeSet};
pub struct Normalized {
    pub rows: Vec<JsonValue>,
    pub sessions: BTreeSet<String>,
    pub max_events: usize,
}
fn field(value: &JsonValue, key: &str) -> Result<JsonValue> {
    value
        .get(key)
        .ok_or_else(|| format!("journal field missing: {key}").into())
}
fn array(value: &JsonValue) -> Result<Vec<JsonValue>> {
    match value.view() {
        JsonView::Array(rows) => Ok(rows),
        _ => Err("journal value is not an array".into()),
    }
}
fn replace(value: &JsonValue, key: &str, new: &JsonValue) -> Result<JsonValue> {
    let JsonView::Object(fields) = value.view() else {
        return Err("journal value is not an object".into());
    };
    let mut text = vec![];
    for (name, value) in fields {
        let name = JsonValue::codepoints(name.to_vec()).ok_or("invalid JSON key")?;
        let value = if name.text_eq(key) {
            new.clone()
        } else {
            value
        };
        text.push(format!("{}:{}", name.to_json()?, value.to_json()?));
    }
    Ok(JsonValue::parse(&format!("{{{}}}", text.join(",")))?)
}
pub fn normalize_trace(trace: &[JsonValue]) -> Result<Normalized> {
    let mut result = Normalized {
        rows: vec![],
        sessions: BTreeSet::new(),
        max_events: 0,
    };
    let mut previous: BTreeMap<String, JsonValue> = BTreeMap::new();
    for snapshot in trace {
        let mut contents = vec![];
        for file in array(snapshot)? {
            if !python_json::equal(&field(&file, "mode")?, &JsonValue::parse("384")?) {
                return Err("journal file permission is not0600".into());
            }
            let journal = python_json::parse_text(
                &field(&file, "bytes")?
                    .to_utf8()
                    .ok_or("journal bytes must be Unicode text")?,
            )?;
            let events = array(&field(&journal, "events")?)?;
            result.max_events = result.max_events.max(events.len());
            if events.len() > 64 {
                return Err("journal exceeds64events".into());
            }
            let mut normalized = vec![];
            for event in events {
                let id = field(&event, "id")?
                    .to_utf8()
                    .ok_or("journal ID must be text")?;
                let (session, sequence) =
                    id.split_once(':').ok_or("journal ID has no separator")?;
                if session.len() != 32
                    || !session
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                    || !sequence
                        .as_bytes()
                        .first()
                        .is_some_and(|b| (b'1'..=b'9').contains(b))
                    || !sequence.bytes().all(|b| b.is_ascii_digit())
                {
                    return Err("journal ID differs from32lowerhex:positive-decimal".into());
                }
                result.sessions.insert(session.into());
                normalized.push(replace(&event, "id", &JsonValue::text(sequence))?.to_json()?);
            }
            let journal = replace(
                &journal,
                "events",
                &JsonValue::parse(&format!("[{}]", normalized.join(",")))?,
            )?;
            let channel = field(&file, "channel")?;
            let name = channel.to_utf8().ok_or("journal channel must be text")?;
            let inode = field(&file, "inode")?;
            let rewritten = previous
                .insert(name, inode.clone())
                .map(|old| !python_json::equal(&old, &inode));
            contents.push(format!(
                "{{\"channel\":{},\"journal\":{},\"rewritten\":{}}}",
                channel.to_json()?,
                journal.to_json()?,
                serde_json::to_string(&rewritten)?
            ));
        }
        result
            .rows
            .push(JsonValue::parse(&format!("[{}]", contents.join(",")))?);
    }
    if result.sessions.len() != 1 {
        return Err("journal must retain one session across all channels".into());
    }
    Ok(result)
}
pub fn compare(source: &[JsonValue], native: &[JsonValue]) -> Result<()> {
    let source = normalize_trace(source)?;
    let native = normalize_trace(native)?;
    if source.rows.len() != native.rows.len() {
        return Err("journal operation cardinality differs".into());
    }
    for (index, (left, right)) in source.rows.iter().zip(&native.rows).enumerate() {
        if !python_json::equal(left, right) {
            return Err(
                format!("journal values/rewrite decisions differ at operation{index}").into(),
            );
        }
    }
    if source.max_events.max(native.max_events) != 64 {
        return Err("journal corpus did not reach64events".into());
    }
    Ok(())
}
