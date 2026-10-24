//! Compare only independently captured original values; no floating tolerance.
use super::{bytes, entries, field, Result, J, V};
use capnp::{dynamic_value::Reader as D, message::ReaderOptions, serialize};
use openpilot_cereal::car_capnp::car_params;
use serde_json::{json, Value};
use std::{
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::Path,
};

pub fn canonical_inputs(value: &J) -> Result<Value> {
    normalize(value)
}

fn normalize(value: &J) -> Result<Value> {
    Ok(match value.view() {
        V::Null => Value::Null,
        V::Bool(v) => json!(v),
        V::Integer(v) => serde_json::from_str(v)?,
        V::Float(v) => json!({"$f64": format!("{:016x}",v.to_bits())}),
        V::Text(_) => json!(super::text(value)?),
        V::Array(rows) => Value::Array(rows.iter().map(normalize).collect::<Result<_>>()?),
        V::Object(_) => {
            let mut fields = serde_json::Map::new();
            for (key, value) in entries(value)? {
                let normalized = if key == "messages" {
                    let V::Array(rows) = value.view() else {
                        return Err("packet roster not array".into());
                    };
                    Value::Array(
                        rows.iter()
                            .map(|p| normalize(&super::project_event(&bytes(p)?)?))
                            .collect::<Result<_>>()?,
                    )
                } else if key == "CarParams" {
                    normalize(&project_car(&bytes(&value)?)?)?
                } else {
                    normalize(&value)?
                };
                fields.insert(key, normalized);
            }
            Value::Object(fields)
        }
    })
}
fn project_car(bytes: &[u8]) -> Result<J> {
    let mut input = bytes;
    let message = serialize::read_message(&mut input, ReaderOptions::new())?;
    if !input.is_empty() {
        return Err("trailing fixture CarParams bytes".into());
    }
    let value: D<'_> = message.get_root::<car_params::Reader<'_>>()?.into();
    super::wire::project(value)
}
fn result(input: &J, output: &J) -> Result<Value> {
    match super::text(&field(input, "op")?)?.as_str() {
        "parameters" => normalize(&project_car(&bytes(output)?)?),
        "event" => normalize(&super::project_event(&bytes(output)?)?),
        _ => normalize(output),
    }
}
fn compare(a: &Value, b: &Value, path: &str) -> Result<()> {
    match (a, b) {
        (Value::Object(a), Value::Object(b)) => {
            if a.keys().collect::<Vec<_>>() != b.keys().collect::<Vec<_>>() {
                return Err(format!("full-schema keys differ at {path}").into());
            }
            for (key, v) in a {
                compare(v, &b[key], &format!("{path}.{key}"))?;
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            if a.len() != b.len() {
                return Err(format!("list length differs at {path}").into());
            }
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                compare(a, b, &format!("{path}[{i}]"))?;
            }
        }
        _ if a != b => {
            return Err(format!("exact original value differs at {path}: {a} vs {b}").into())
        }
        _ => {}
    }
    Ok(())
}
fn wire_counts(old: &J, native: &J, packet: bool) -> Result<(u64, u64)> {
    if packet {
        return Ok((1, u64::from(bytes(old)? == bytes(native)?)));
    }
    let mut total = 0;
    let mut equal = 0;
    match old.view() {
        V::Array(rows) => {
            let V::Array(other) = native.view() else {
                return Err("wire roster type changed".into());
            };
            for (a, b) in rows.iter().zip(&other) {
                let (n, e) = wire_counts(a, b, false)?;
                total += n;
                equal += e;
            }
        }
        V::Object(_) => {
            for (name, value) in entries(old)? {
                let other = field(native, &name)?;
                if name == "messages" {
                    let V::Array(a) = value.view() else {
                        return Err("messages not array".into());
                    };
                    let V::Array(b) = other.view() else {
                        return Err("native messages not array".into());
                    };
                    for (a, b) in a.iter().zip(&b) {
                        let (n, e) = wire_counts(a, b, true)?;
                        total += n;
                        equal += e;
                    }
                } else {
                    let (n, e) = wire_counts(&value, &other, name == "CarParams")?;
                    total += n;
                    equal += e;
                }
            }
        }
        _ => {}
    }
    Ok((total, equal))
}

// Frame only the outer cases array. The existing parsers still validate every
// complete slice, including Python's nonfinite values on the two raw channels.
// Borrowing the retained raw lines avoids three whole-array parsed trees.
struct CasesCursor<'a> {
    source: &'a str,
    offset: usize,
    finished: bool,
}
impl<'a> CasesCursor<'a> {
    fn new(source: &'a str) -> Result<Self> {
        let mut cursor = Self {
            source,
            offset: 0,
            finished: false,
        };
        cursor.whitespace();
        if cursor.peek() != Some(b'[') {
            return Err("cases result must be an array".into());
        }
        cursor.offset += 1;
        cursor.whitespace();
        if cursor.peek() == Some(b']') {
            cursor.close()?;
        }
        Ok(cursor)
    }

    fn peek(&self) -> Option<u8> {
        self.source.as_bytes().get(self.offset).copied()
    }

    fn whitespace(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\r' | b'\n')) {
            self.offset += 1;
        }
    }

    fn close(&mut self) -> Result<()> {
        self.offset += 1;
        self.whitespace();
        if self.peek().is_some() {
            return Err("trailing cases array data".into());
        }
        self.finished = true;
        Ok(())
    }

    fn next_value(&mut self) -> Result<Option<&'a str>> {
        if self.finished {
            return Ok(None);
        }
        let start = self.offset;
        let mut stack = Vec::new();
        let mut quoted = false;
        let mut escaped = false;
        loop {
            let byte = self.peek().ok_or("incomplete cases array")?;
            if quoted {
                if escaped {
                    escaped = false;
                } else {
                    match byte {
                        b'\\' => escaped = true,
                        b'"' => quoted = false,
                        _ => {}
                    }
                }
            } else {
                match byte {
                    b'"' => quoted = true,
                    b'[' | b'{' => stack.push(byte),
                    b',' | b']' if stack.is_empty() => {
                        let value = &self.source[start..self.offset];
                        if value.is_empty() {
                            return Err("missing cases array element".into());
                        }
                        if byte == b']' {
                            self.close()?;
                        } else {
                            self.offset += 1;
                            self.whitespace();
                            if matches!(self.peek(), None | Some(b']')) {
                                return Err("missing case after comma".into());
                            }
                        }
                        return Ok(Some(value));
                    }
                    b']' | b'}' => {
                        let opening = if byte == b']' { b'[' } else { b'{' };
                        if stack.pop() != Some(opening) {
                            return Err("mismatched cases array brackets".into());
                        }
                    }
                    _ => {}
                }
            }
            self.offset += 1;
        }
    }
}

fn audit_cases(
    old: &str,
    expected: &str,
    native: &str,
    projections: &mut impl Write,
    record: usize,
) -> Result<(bool, u64, u64)> {
    let mut old = CasesCursor::new(old)?;
    let mut expected = CasesCursor::new(expected)?;
    let mut native = CasesCursor::new(native)?;
    let mut index = 0;
    let mut raw_equal = true;
    let mut packets = 0;
    let mut equal = 0;
    projections.write_all(b"[")?;
    loop {
        let (old, expected, native) = match (
            old.next_value()?,
            expected.next_value()?,
            native.next_value()?,
        ) {
            (None, None, None) => break,
            (Some(old), Some(expected), Some(native)) => (old, expected, native),
            _ => return Err(format!("cases list length differs at record {record}").into()),
        };
        // These trees and the normalized native case are released each turn.
        // Keep the full original-projection check before the native check.
        let old = J::parse(old)?;
        let expected: Value = serde_json::from_str(expected)?;
        let native = J::parse(native)?;
        compare(
            &expected,
            &normalize(&old)?,
            &format!("original-projection[{record}][{index}]"),
        )?;
        let normalized = normalize(&native)?;
        compare(
            &expected,
            &normalized,
            &format!("native[{record}][{index}]"),
        )?;
        if index != 0 {
            projections.write_all(b",")?;
        }
        // One write per serialized case also avoids per-field File writes.
        projections.write_all(serde_json::to_string(&normalized)?.as_bytes())?;
        let (n, e) = wire_counts(&old, &native, false)?;
        packets += n;
        equal += e;
        raw_equal &= old.to_json()? == native.to_json()?;
        index += 1;
    }
    projections.write_all(b"]\n")?;
    Ok((raw_equal, packets, equal))
}

pub fn audit_saved(proof: &Path) -> Result<()> {
    let manifest: Value = serde_json::from_slice(&fs::read(proof.join("original-manifest.json"))?)?;
    for name in [
        "requests.jsonl",
        "original.jsonl",
        "original-projection.jsonl",
    ] {
        if crate::seal::digest(&proof.join(name))?
            != manifest["files"][name]["sha256"]
                .as_str()
                .ok_or("missing original file seal")?
        {
            return Err(format!("original file changed: {name}").into());
        }
    }
    for (name, hash) in super::SOURCES {
        if manifest["sources"][name] != hash {
            return Err(format!("original source admission changed: {name}").into());
        }
    }
    let mut requests = BufReader::new(fs::File::open(proof.join("requests.jsonl"))?).lines();
    let mut original = BufReader::new(fs::File::open(proof.join("original.jsonl"))?).lines();
    let mut expected =
        BufReader::new(fs::File::open(proof.join("original-projection.jsonl"))?).lines();
    let mut actual = BufReader::new(fs::File::open(proof.join("native.jsonl"))?).lines();
    let mut projections = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(proof.join("native-projection.jsonl"))?;
    let mut count = 0;
    let mut raw_equal = 0;
    let mut packet_count = 0;
    let mut packet_bytes_equal = 0;
    for row in requests.by_ref() {
        let input = J::parse(&row?)?;
        if super::text(&field(&input, "op")?)? == "cases" {
            let old = original.next().ok_or("missing original result")??;
            let expected = expected.next().ok_or("missing original projection")??;
            let native = actual.next().ok_or("missing native result")??;
            let (same, packets, equal) =
                audit_cases(&old, &expected, &native, &mut projections, count)?;
            raw_equal += usize::from(same);
            packet_count += packets;
            packet_bytes_equal += equal;
            count += 1;
            continue;
        }
        let old = J::parse(&original.next().ok_or("missing original result")??)?;
        let expected: Value =
            serde_json::from_str(&expected.next().ok_or("missing original projection")??)?;
        let native = J::parse(&actual.next().ok_or("missing native result")??)?;
        // Check our full-schema projection against pycapnp on original wire
        // before using the same reader to inspect the native wire.
        compare(
            &expected,
            &result(&input, &old)?,
            &format!("original-projection[{count}]"),
        )?;
        let normalized = result(&input, &native)?;
        compare(&expected, &normalized, &format!("native[{count}]"))?;
        writeln!(projections, "{}", serde_json::to_string(&normalized)?)?;
        let direct = matches!(
            super::text(&field(&input, "op")?)?.as_str(),
            "event" | "parameters"
        );
        let (packets, equal) = wire_counts(&old, &native, direct)?;
        packet_count += packets;
        packet_bytes_equal += equal;
        raw_equal += usize::from(old.to_json()? == native.to_json()?);
        count += 1;
    }
    if original.next().is_some() || expected.next().is_some() || actual.next().is_some() {
        return Err("extra fixture output record".into());
    }
    if count
        != manifest["requests"]
            .as_u64()
            .ok_or("missing original request count")? as usize
    {
        return Err("original request roster changed".into());
    }
    projections.sync_all()?;
    let mut receipt = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(proof.join("comparison.json"))?;
    receipt.write_all(&serde_json::to_vec_pretty(&json!({"pass":true,"requests":count,"raw_result_records_equal":raw_equal,"packet_pairs":packet_count,"packet_pairs_byte_identical":packet_bytes_equal,"schema_and_float_bits":"exact; independently checked original pycapnp projection","expected_origin":"actual original fixture/scenario helpers","sources_retired":false,"runtime_acceptance":false}))?)?;
    receipt.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cases_cursor_splits_nested_escaped_and_nonfinite_values() {
        let values = [
            r#"{"nested":[{"text":"한글,]}\"\\","empty":[]},[NaN,Infinity,-Infinity,-0.0]],"surrogate":"\ud800"}"#,
            r#""quote: \" and backslash: \\ and delimiters , [ ] { }""#,
            "true",
            "null",
        ];
        let source = format!(" \t[\n{}\r\n]\t", values.join(" ,\n"));
        let mut cursor = CasesCursor::new(&source).unwrap();
        for expected in values {
            let value = cursor.next_value().unwrap().unwrap();
            assert_eq!(value.trim(), expected);
            J::parse(value).unwrap();
        }
        assert!(cursor.next_value().unwrap().is_none());
        assert!(cursor.next_value().unwrap().is_none());
        assert!(CasesCursor::new(" [ \n ] \t")
            .unwrap()
            .next_value()
            .unwrap()
            .is_none());
    }

    #[test]
    fn streamed_cases_preserve_projection_float_bits_and_packet_counts() {
        let packet =
            super::super::wire::event_packet("onroadEvents", 1., &J::parse("[]").unwrap()).unwrap();
        let packet = serde_json::to_string(&packet).unwrap();
        let first = format!(
            r#"{{"name":"first","frames":[{{"time":-0.0,"messages":[{packet}]}}],"more":[NaN,Infinity,-Infinity]}}"#
        );
        let second =
            format!(r#"{{"name":"second","frames":[{{"time":1.0,"messages":[{packet}]}}]}}"#);
        let original = format!("[{first},{second}]");
        // Only the first case changes dictionary order: a later equal case
        // must not reset the whole record's raw-equality diagnostic to true.
        let native = format!(
            r#"[{{"more":[NaN,Infinity,-Infinity],"frames":[{{"messages":[{packet}],"time":-0.0}}],"name":"first"}},{second}]"#
        );
        let whole_old = J::parse(&original).unwrap();
        let whole_native = J::parse(&native).unwrap();
        // Synthetic algorithm parity with the retained whole-array path;
        // this does not generate or replace any captured original expected data.
        let expected = serde_json::to_string(&normalize(&whole_old).unwrap()).unwrap();
        let mut output = Vec::new();
        let stats = audit_cases(&original, &expected, &native, &mut output, 108).unwrap();
        assert_eq!(stats, (false, 2, 2));
        assert_eq!(
            stats.0,
            whole_old.to_json().unwrap() == whole_native.to_json().unwrap()
        );
        assert_eq!(
            (stats.1, stats.2),
            wire_counts(&whole_old, &whole_native, false).unwrap()
        );
        assert_eq!(
            output,
            format!(
                "{}\n",
                serde_json::to_string(&normalize(&whole_native).unwrap()).unwrap()
            )
            .as_bytes()
        );
        let projected: Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(
            projected[0]["frames"][0]["time"]["$f64"],
            "8000000000000000"
        );
        assert_eq!(projected[0]["more"][0]["$f64"], "7ff8000000000000");
        assert_eq!(projected[0]["more"][1]["$f64"], "7ff0000000000000");
        assert_eq!(projected[0]["more"][2]["$f64"], "fff0000000000000");
        let mut output = Vec::new();
        assert_eq!(
            audit_cases(&original, &expected, &original, &mut output, 108).unwrap(),
            (true, 2, 2)
        );
        let mut empty = Vec::new();
        assert_eq!(
            audit_cases("[]", "[]", "[]", &mut empty, 0).unwrap(),
            (true, 0, 0)
        );
        assert_eq!(empty, b"[]\n");
    }

    #[test]
    fn streamed_cases_reject_incomplete_rosters_syntax_and_late_differences() {
        let original = r#"[{"n":-0.0,"list":[1,2]},{"n":NaN,"list":[]}]"#;
        let expected = r#"[{"n":{"$f64":"8000000000000000"},"list":[1,2]},{"n":{"$f64":"7ff8000000000000"},"list":[]}]"#;
        for native in [
            r#"[{"n":-0.0,"list":[1,2]}]"#,
            r#"[{"n":-0.0,"list":[1,2]},{"n":NaN,"list":[]},null]"#,
            r#"[{"n":-0.0,"list":[1,2]},{"n":NaN,"list":[1]}]"#,
            r#"[{"n":-0.0,"list":[1,2]},{"n":NaN,"list":[],"extra":true}]"#,
            r#"[{"n":-0.0,"list":[1,2]},{"n":NaN,"list":{}}]"#,
            r#"[{"n":0.0,"list":[1,2]},{"n":NaN,"list":[]}]"#,
            r#"[{"n":-0.0,"list":[1,2]},{"n":Infinity,"list":[]}]"#,
        ] {
            assert!(
                audit_cases(original, expected, native, &mut Vec::new(), 108).is_err(),
                "accepted differing native cases: {native}"
            );
        }
        // A bad original projection is rejected even if the native agrees
        // with it; each case must first agree with the original wire/scalars.
        assert!(audit_cases("[1,2]", "[1,3]", "[1,3]", &mut Vec::new(), 0).is_err());
        for malformed in [
            "",
            "null",
            "{}",
            "[",
            "[1",
            "[1,",
            "[1,]",
            "[,1]",
            "[1,,2]",
            "[1] null",
            "[1]]",
            "[[1,]]",
            "[{\"x\":1,}]",
            "[{\"x\":[1}]]",
            "[[1}]",
            "[\"unterminated]",
            "[\"escaped end\\\"]",
            "[\"invalid\\q\"]",
            "[true false]",
            "[01]",
            "[1 2]",
        ] {
            for channel in 0..3 {
                let mut rows = ["[1]"; 3];
                rows[channel] = malformed;
                assert!(
                    audit_cases(rows[0], rows[1], rows[2], &mut Vec::new(), 0).is_err(),
                    "accepted malformed channel {channel}: {malformed:?}"
                );
            }
        }
        for extra in ["[]", "[1,2]"] {
            for channel in 0..3 {
                let mut rows = ["[1]"; 3];
                rows[channel] = extra;
                assert!(audit_cases(rows[0], rows[1], rows[2], &mut Vec::new(), 0).is_err());
            }
        }
        assert!(audit_cases("[NaN]", "[NaN]", "[NaN]", &mut Vec::new(), 0).is_err());
    }
}
