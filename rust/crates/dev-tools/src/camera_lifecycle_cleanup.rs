//! Port of the independent camera resource ledger and permitted extra cleanup.
use super::values::*;
use crate::{control_tools_policy_check::json, seal::Result};
use openpilot_log_format::JsonValue;
#[derive(Clone)]
enum Key {
    Atom(JsonValue),
    Tuple(Vec<JsonValue>),
}
impl Key {
    fn equal(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Atom(a), Self::Atom(b)) => key_equal(a, b),
            (Self::Tuple(a), Self::Tuple(b)) => {
                a.len() == b.len() && a.iter().zip(b).all(|(a, b)| json::equal(a, b))
            }
            _ => false,
        }
    }
    fn repr(&self) -> Result<String> {
        match self {
            Self::Atom(a) => repr(a),
            Self::Tuple(parts) => Ok(format!(
                "({}{})",
                parts
                    .iter()
                    .map(repr)
                    .collect::<Result<Vec<_>>>()?
                    .join(", "),
                if parts.len() == 1 { "," } else { "" }
            )),
        }
    }
}
#[derive(Default)]
struct Counter(Vec<(Key, usize)>);
impl Counter {
    fn add(&mut self, key: Key) {
        if let Some((_, count)) = self.0.iter_mut().find(|(prior, _)| prior.equal(&key)) {
            *count += 1
        } else {
            self.0.push((key, 1))
        }
    }
    fn contains(&self, key: &Key) -> bool {
        self.0.iter().any(|(prior, _)| prior.equal(key))
    }
    fn equal(&self, other: &Self) -> bool {
        self.0.len() == other.0.len()
            && self
                .0
                .iter()
                .all(|(key, count)| other.0.iter().any(|(k, c)| key.equal(k) && count == c))
    }
    fn repr(&self) -> Result<String> {
        Ok(format!(
            "{{{}}}",
            self.0
                .iter()
                .map(|(key, count)| Ok(format!("{}: {count}", key.repr()?)))
                .collect::<Result<Vec<_>>>()?
                .join(", ")
        ))
    }
}
fn unique(values: Vec<JsonValue>) -> Result<Vec<JsonValue>> {
    let mut out = vec![];
    for value in values {
        hashable(&value)?;
        if !out.iter().any(|v| key_equal(v, &value)) {
            out.push(value)
        }
    }
    Ok(out)
}
fn member(value: &JsonValue, values: &[JsonValue]) -> Result<bool> {
    hashable(value)?;
    Ok(values.iter().any(|v| key_equal(v, value)))
}
fn text(row: &JsonValue, key: &str, expected: &str) -> Result<bool> {
    Ok(get(row, key)?.text_eq(expected))
}
fn equal_num(row: &JsonValue, key: &str, value: i64) -> Result<bool> {
    Ok(json::equal(&get(row, key)?, &number(value)))
}
fn tuple(values: &[u32]) -> Key {
    Key::Tuple(values.iter().map(number).collect())
}
type Call = (usize, JsonValue, JsonValue);
pub fn calls(records: &JsonValue) -> Result<Vec<Call>> {
    let mut target = number(-1);
    let mut out = vec![];
    for (index, row) in sequence(records)?.into_iter().enumerate() {
        if text(&row, "op", "target")? {
            target = get(&row, "fd")?;
        }
        if text(&row, "op", "ioctl")? {
            out.push((index, target.clone(), row))
        }
    }
    Ok(out)
}
pub fn audit(records: &JsonValue) -> Result<JsonValue> {
    let records_vec = sequence(records)?;
    let categories = [
        "devices",
        "sessions",
        "started",
        "links",
        "deactivation",
        "buffers",
        "fences",
        "fds",
        "mappings",
    ];
    let mut owned = (0..9).map(|_| Counter::default()).collect::<Vec<_>>();
    let mut released = (0..9).map(|_| Counter::default()).collect::<Vec<_>>();
    let mut image_fds = vec![];
    for row in &records_vec {
        if text(row, "op", "image-import")? {
            image_fds.push(get(row, "fd")?);
        }
    }
    let image_fds = unique(image_fds)?;
    for row in &records_vec {
        let op = get(row, "op")?;
        if op.text_eq("open") || op.text_eq("vision-open") {
            let fd = get(row, "fd")?;
            hashable(&fd)?;
            owned[7].add(Key::Atom(fd));
        }
        if op.text_eq("close") {
            let fd = get(row, "fd")?;
            hashable(&fd)?;
            released[7].add(Key::Atom(fd));
        }
        if (op.text_eq("mmap") || op.text_eq("vision-mmap")) && truth(&get(row, "ok")?) {
            let fd = get(row, "fd")?;
            hashable(&fd)?;
            owned[8].add(Key::Atom(fd));
        }
        if op.text_eq("munmap") || op.text_eq("vision-munmap") {
            let fd = get(row, "fd")?;
            hashable(&fd)?;
            released[8].add(Key::Atom(fd));
        }
    }
    for fd in image_fds {
        let key = Key::Atom(fd);
        if !owned[7].contains(&key) {
            owned[7].add(key)
        }
    }
    let mut imports = vec![];
    for (_, target, row) in calls(records)? {
        let name = get(&row, "name")?;
        let success = equal_num(&row, "ret", 0)?;
        if (name.text_eq("camera:258") || name.text_eq("camera:259")) && success
            || name.text_eq("camera:262")
            || name.text_eq("camera:260")
        {
            hashable(&target)?;
        }
        let pair = Key::Tuple(vec![
            target.clone(),
            number(word(&row, "before", 0)?),
            number(word(&row, "before", 4)?),
        ]);
        if name.text_eq("camera:258") && success {
            owned[0].add(Key::Tuple(vec![
                target.clone(),
                number(word(&row, "after", 0)?),
                number(word(&row, "after", 4)?),
            ]));
        }
        if name.text_eq("camera:262") {
            released[0].add(pair.clone());
        }
        if name.text_eq("camera:267") && success {
            owned[1].add(Key::Atom(number(word(&row, "after", 0)?)));
        }
        if name.text_eq("camera:268") {
            released[1].add(Key::Atom(number(word(&row, "before", 0)?)));
        }
        if name.text_eq("camera:259") && success {
            owned[2].add(pair.clone());
        }
        if name.text_eq("camera:260") {
            released[2].add(pair);
        }
        if name.text_eq("camera:269") && success {
            let after = hex(&get(&row, "after")?)?;
            let key = tuple(&[
                word_bytes(&after, 0),
                word_bytes(&after, after.len().saturating_sub(4)),
            ]);
            owned[3].add(key.clone());
            owned[4].add(key);
        }
        if name.text_eq("camera:270") {
            released[3].add(tuple(&[word(&row, "before", 0)?, word(&row, "before", 4)?]));
        }
        if name.text_eq("camera:278") && word(&row, "before", 0)? == 1 {
            released[4].add(tuple(&[
                word(&row, "before", 4)?,
                word(&row, "before", 16)?,
            ]));
        }
        if name.text_eq("camera:274") && word(&row, "after", 88)? != 0 {
            owned[5].add(Key::Atom(number(word(&row, "after", 88)?)));
            owned[7].add(Key::Atom(number(word(&row, "after", 92)?)));
        }
        if name.text_eq("camera:275") && success {
            imports.push(number(word(&row, "after", 80)?));
        }
        if name.text_eq("camera:276") {
            released[5].add(Key::Atom(number(word(&row, "before", 0)?)));
        }
        if (name.text_eq("sync:0") || name.text_eq("camera:0")) && success {
            owned[6].add(Key::Atom(number(word(&row, "after", 64)?)));
        }
        if name.text_eq("sync:1") {
            released[6].add(Key::Atom(number(word(&row, "before", 64)?)));
        }
    }
    for imported in unique(imports)? {
        owned[5].add(Key::Atom(imported));
    }
    for (index, category) in categories.iter().enumerate() {
        assert(
            owned[index].equal(&released[index]),
            format!(
                "({}, {}, {})",
                repr(&JsonValue::text(category))?,
                owned[index].repr()?,
                released[index].repr()?
            ),
        )?;
    }
    object(
        &categories
            .iter()
            .enumerate()
            .map(|(index, category)| {
                (
                    category.to_string(),
                    number(owned[index].0.iter().map(|(_, n)| n).sum::<usize>()),
                )
            })
            .collect::<Vec<_>>(),
    )
}
pub fn normalize_disabled(
    source: &JsonValue,
    native: &JsonValue,
    kind: &str,
) -> Result<(JsonValue, JsonValue)> {
    let targets = calls(native)?;
    let source_calls = calls(source)?;
    let native_rows = sequence(native)?;
    let mut selected: Vec<Call> = vec![];
    match kind {
        "probe-none" => return Ok((native.clone(), array(&[])?)),
        "disabled" | "sensor-init" => {
            for (target, name) in [(502, "camera:262"), (501, "camera:268")] {
                let matches = targets
                    .iter()
                    .filter_map(|(i, fd, row)| match text(row, "name", name) {
                        Ok(true) if json::equal(fd, &number(target)) => {
                            Some(Ok((*i, fd.clone(), row.clone())))
                        }
                        Ok(_) => None,
                        Err(e) => Some(Err(e)),
                    })
                    .collect::<Result<Vec<_>>>()?;
                let tuples = matches
                    .iter()
                    .map(|(i, fd, row)| Ok(format!("({}, {}, {})", i, repr(fd)?, repr(row)?)))
                    .collect::<Result<Vec<_>>>()?
                    .join(", ");
                assert(
                    matches.len() == 1,
                    format!("({target}, {}, [{tuples}])", repr(&JsonValue::text(name))?),
                )?;
                let mut found = false;
                for (_, fd, row) in &source_calls {
                    if json::equal(fd, &number(target)) && text(row, "name", name)? {
                        found = true;
                        break;
                    }
                }
                assert(!found, "")?;
                selected.extend(matches);
            }
        }
        "sensor-poke" => {
            let mut imports = vec![];
            for (_, _, row) in &targets {
                if text(row, "name", "camera:275")? && equal_num(row, "ret", 0)? {
                    imports.push(number(word(row, "after", 80)?));
                }
            }
            let imports = unique(imports)?;
            for (index, target, row) in &targets {
                let name = get(row, "name")?;
                let remove = name.text_eq("sync:1")
                    || ((name.text_eq("camera:260") || name.text_eq("camera:262"))
                        && [503, 504, 506]
                            .iter()
                            .any(|v| json::equal(target, &number(v))))
                    || (name.text_eq("camera:278") && word(row, "before", 0)? == 1)
                    || name.text_eq("camera:270")
                    || (name.text_eq("camera:276")
                        && member(&number(word(row, "before", 0)?), &imports)?);
                if remove {
                    selected.push((*index, target.clone(), row.clone()));
                }
            }
            for (target, name) in [(501, "camera:272"), (504, "camera:264")] {
                let mut matches = vec![];
                let mut prior = vec![];
                for (index, fd, row) in &targets {
                    if json::equal(fd, &number(target)) && text(row, "name", name)? {
                        matches.push((*index, fd.clone(), row.clone()));
                    }
                }
                for (_, fd, row) in &source_calls {
                    if json::equal(fd, &number(target)) && text(row, "name", name)? {
                        prior.push(row);
                    }
                }
                assert(
                    matches.len() == prior.len() + usize::from(!matches.is_empty()),
                    "",
                )?;
                if let Some(last) = matches.last() {
                    let old = prior
                        .last()
                        .ok_or_else(|| error("IndexError", "list index out of range"))?;
                    assert(json::equal(&last.2, old), "")?;
                    selected.push(last.clone());
                }
            }
        }
        other => {
            return Err(error(
                "AssertionError",
                format!(
                    "Expected code to be unreachable, but got: {}",
                    repr(&JsonValue::text(other))?
                ),
            ))
        }
    }
    let mut removed = std::collections::BTreeSet::new();
    let mut receipt = vec![];
    for (index, target, row) in selected {
        let preceding = if index == 0 {
            native_rows.last()
        } else {
            native_rows.get(index - 1)
        }
        .ok_or_else(|| error("IndexError", "list index out of range"))?;
        assert(
            json::equal(
                preceding,
                &object(&[
                    ("op".into(), JsonValue::text("target")),
                    ("fd".into(), target.clone()),
                ])?,
            ),
            "",
        )?;
        let name = get(&row, "name")?
            .to_utf8()
            .ok_or_else(|| error("KeyError", repr(&get(&row, "name").unwrap()).unwrap()))?;
        let length = match name.as_str() {
            "sync:1" => 68,
            "camera:260" | "camera:262" | "camera:268" | "camera:270" | "camera:276" => 8,
            "camera:264" => 32,
            "camera:272" | "camera:278" => 24,
            _ => return Err(error("KeyError", repr(&JsonValue::text(&name))?)),
        };
        let payload = hex(&get(&row, "before")?)?;
        let mut envelope = vec![];
        for value in [
            name.split(':').nth(1).unwrap().parse::<u32>()?,
            length as u32,
            u32::from(name.starts_with("camera:")),
            0,
        ] {
            envelope.extend(value.to_le_bytes());
        }
        envelope.extend(1u64.to_le_bytes());
        let envelope = JsonValue::text(&hex_text(&envelope));
        assert(
            payload.len() == length && json::equal(&get(&row, "after")?, &get(&row, "before")?),
            "",
        )?;
        assert(
            json::equal(&get(&row, "outer_before")?, &get(&row, "outer_after")?)
                && json::equal(&get(&row, "outer_after")?, &envelope),
            "",
        )?;
        assert(
            equal_num(&row, "ret", 0)?
                && equal_num(&row, "errno", 0)?
                && get(&row, "nested")?.text_eq(""),
            "",
        )?;
        if name == "sync:1" {
            assert(payload.get(..64) == Some(&[0u8; 64]), "")?;
        }
        if name == "camera:268" || name == "camera:276" {
            assert(payload.get(4..) == Some(&[0u8; 4]), "")?;
        }
        if name == "camera:278" {
            let mut active = vec![];
            for (_, fd, other) in &source_calls {
                if json::equal(fd, &target)
                    && get(other, "name")?.text_eq(&name)
                    && word(other, "before", 0)? == 0
                {
                    active.push(other);
                }
            }
            if active.len() != 1 {
                return Err(error(
                    "ValueError",
                    if active.is_empty() {
                        "not enough values to unpack (expected 1, got 0)"
                    } else {
                        "too many values to unpack (expected 1)"
                    },
                ));
            }
            let mut expected = vec![1, 0, 0, 0];
            expected.extend(hex(&get(active[0], "before")?)?.into_iter().skip(4));
            assert(payload == expected, "")?;
        }
        removed.insert(index as isize - 1);
        removed.insert(index as isize);
        receipt.push(object(&[
            ("fd".into(), target),
            ("name".into(), JsonValue::text(&name)),
            ("payload".into(), get(&row, "before")?),
        ])?);
    }
    Ok((
        array(
            &native_rows
                .into_iter()
                .enumerate()
                .filter(|(i, _)| !removed.contains(&(*i as isize)))
                .map(|(_, r)| r)
                .collect::<Vec<_>>(),
        )?,
        array(&receipt)?,
    ))
}
