//! Fixed schedule translated from the pinned Python checker. Expected UTF-8 is
//! encoded here independently of the candidate's JsonValue/atomic_value code.
//! CI validates every input and output byte against the unchanged original.
use std::fmt::Write;

pub const CASES: usize = 15;
pub const SCALARS: usize = 0x110000 - 0x800;
pub struct Case {
    pub input: String,
    pub expected: Vec<u8>,
    pub succeeded: bool,
}
fn string(value: impl Iterator<Item = char>, ascii: bool) -> String {
    let mut text = String::from("\"");
    for ch in value {
        match ch {
            '"' => text.push_str("\\\""),
            '\\' => text.push_str("\\\\"),
            '\u{8}' => text.push_str("\\b"),
            '\u{c}' => text.push_str("\\f"),
            '\n' => text.push_str("\\n"),
            '\r' => text.push_str("\\r"),
            '\t' => text.push_str("\\t"),
            ch if ch < ' ' || (ascii && ch > '~') => {
                let point = u32::from(ch);
                if point <= 0xffff {
                    write!(text, "\\u{point:04x}").expect("String write");
                } else {
                    let point = point - 0x10000;
                    write!(
                        text,
                        "\\u{:04x}\\u{:04x}",
                        0xd800 + (point >> 10),
                        0xdc00 + (point & 1023)
                    )
                    .expect("String write");
                }
            }
            ch => text.push(ch),
        }
    }
    text.push('"');
    text
}
pub fn cases() -> Vec<Case> {
    let mut cases = [
        "null",
        "true",
        "false",
        "0",
        "-0.0",
        "1e-07",
        "1e+20",
        "Infinity",
        "-Infinity",
        "NaN",
    ]
    .into_iter()
    .map(|text| Case {
        input: text.into(),
        expected: text.as_bytes().to_vec(),
        succeeded: true,
    })
    .collect::<Vec<_>>();
    let number = format!("1{}", "0".repeat(200));
    let nested = "한😀\0\u{7f}";
    cases.push(Case {
        input: format!(
            "{{\"z\": {}, \"a\": [true, null, {number}]}}",
            string(nested.chars(), true)
        ),
        expected: format!(
            "{{\"z\": {}, \"a\": [true, null, {number}]}}",
            string(nested.chars(), false)
        )
        .into_bytes(),
        succeeded: true,
    });
    for input in [r#""\ud800""#, r#""\udfff""#, r#"{"\ud800": "value"}"#] {
        cases.push(Case {
            input: input.into(),
            expected: b"initial".to_vec(),
            succeeded: false,
        });
    }
    cases.push(Case {
        input: string((0..0x110000).filter_map(char::from_u32), true),
        expected: string((0..0x110000).filter_map(char::from_u32), false).into_bytes(),
        succeeded: true,
    });
    cases
}
pub fn payload(cases: &[Case]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for case in cases {
        bytes.extend_from_slice(case.input.as_bytes());
        bytes.push(b'\n');
    }
    bytes
}
