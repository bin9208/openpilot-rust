use crate::{Error, Value};

pub(crate) fn unknown(name: &str) -> Error {
    let bytes = name.as_bytes();
    let quote = if bytes.contains(&b'\'') && !bytes.contains(&b'"') {
        b'"'
    } else {
        b'\''
    };
    let mut repr = format!("b{}", char::from(quote));
    for &byte in bytes {
        match byte {
            b'\t' => repr.push_str("\\t"),
            b'\n' => repr.push_str("\\n"),
            b'\r' => repr.push_str("\\r"),
            b'\\' => repr.push_str("\\\\"),
            value if value == quote => {
                repr.push('\\');
                repr.push(char::from(value));
            }
            32..=126 => repr.push(char::from(byte)),
            value => repr.push_str(&format!("\\x{value:02x}")),
        }
    }
    repr.push(char::from(quote));
    Error::Source(repr)
}

pub(crate) fn fatal(name: &str, error: &dyn std::fmt::Display) -> ! {
    eprintln!("carrot-server: fatal registered Params read {name}: {error}");
    std::process::abort()
}

pub(crate) fn mismatch(name: &str, expected: (&str, u8), value: &Value) -> Error {
    match value.repr() {
        Ok(repr) => Error::Source(format!(
            "Type mismatch while writing param {name}: proposed_type=<class '{}'> expected_type=<ParamKeyType.{}: {}> value={repr}",
            value.type_name(), expected.0, expected.1)),
        Err(error) => error.into(),
    }
}
