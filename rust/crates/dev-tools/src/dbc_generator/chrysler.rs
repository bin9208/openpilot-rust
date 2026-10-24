// Port of opendbc's _stellantis_common_ram.py, MIT; see LICENSE.opendbc.
use super::{Error, Result};
use std::{
    collections::BTreeSet,
    fs::File,
    io::{Read, Write},
    path::Path,
};
const DT: &[(i64, i64)] = &[
    (258, 35),
    (264, 37),
    (268, 113),
    (280, 181),
    (284, 121),
    (288, 123),
    (320, 131),
    (344, 139),
    (368, 147),
    (464, 464),
    (500, 153),
    (501, 232),
    (544, 49),
    (571, 177),
    (559, 157),
    (625, 163),
    (669, 213),
    (678, 250),
    (720, 720),
    (792, 792),
    (820, 657),
];
const HD: &[(i64, i64)] = &[(571, 570), (678, 629)];
// Python int's decimal normalization, without narrowing unmapped DBC addresses
// to machine integers. Existing decimal Unicode mechanics are shared.
fn address(text: &str) -> Result<String> {
    let text = text.trim();
    let (negative, text) = if let Some(v) = text.strip_prefix('-') {
        (true, v)
    } else {
        (false, text.strip_prefix('+').unwrap_or(text))
    };
    let mut digits = String::new();
    let mut previous_digit = false;
    for ch in text.chars() {
        if ch == '_' && previous_digit {
            previous_digit = false;
            continue;
        }
        let Some(digit) = crate::argparse_scan::decimal_value(ch) else {
            return Err(Error::new(
                "ValueError",
                None,
                "invalid decimal DBC address",
            ));
        };
        digits.push(char::from(b'0' + digit));
        previous_digit = true;
    }
    if !previous_digit {
        return Err(Error::new(
            "ValueError",
            None,
            "invalid decimal DBC address",
        ));
    }
    // CPython limits decimal int conversion before stripping leading zeroes.
    // Each admitted Unicode decimal digit contributed one ASCII digit above;
    // signs and valid underscores do not contribute to this count.
    if digits.len() > 4300 {
        return Err(Error::new(
            "ValueError",
            None,
            "decimal DBC address exceeds the 4300 digit limit",
        ));
    }
    let digits = digits.trim_start_matches('0');
    Ok(if digits.is_empty() {
        "0".to_owned()
    } else if negative {
        format!("-{digits}")
    } else {
        digits.to_owned()
    })
}
pub(super) fn write(directory: &Path) -> Result<()> {
    let source = directory.join("_stellantis_common.dbc");
    for (filename, lookup) in [
        ("_stellantis_common_ram_dt_generated.dbc", DT),
        ("_stellantis_common_ram_hd_generated.dbc", HD),
    ] {
        let mut input = File::open(&source).map_err(|e| Error::io(e, &source))?;
        if input
            .metadata()
            .map_err(|e| Error::io(e, &source))?
            .is_dir()
        {
            return Err(
                Error::new("IsADirectoryError", Some(21), "common DBC is a directory")
                    .at_path(&source),
            );
        }
        let path = directory.join(filename);
        let mut output = File::create(&path).map_err(|e| Error::io(e, &path))?;
        output
            .write_all(b"CM_ \"Generated from _stellantis_common.dbc\"\n\n")
            .map_err(|e| Error::io(e, &path))?;
        let mut bytes = Vec::new();
        input
            .read_to_end(&mut bytes)
            .map_err(|e| Error::io(e, &source))?;
        let text = String::from_utf8(bytes)
            .map_err(|e| {
                Error::new("UnicodeDecodeError", None, &e.to_string())
                    .at_path(&source)
                    .with_decode(e.into_bytes())
            })?
            .replace("\r\n", "\n")
            .replace('\r', "\n");
        let mut wrote = BTreeSet::new();
        for line in text.split_inclusive('\n') {
            let text = if line.starts_with("BO_") || line.starts_with("VAL_") {
                let mut fields: Vec<_> = line.split(' ').map(str::to_owned).collect();
                let field = fields
                    .get_mut(1)
                    .ok_or_else(|| Error::new("IndexError", None, "DBC address field missing"))?;
                let value = address(field)?;
                wrote.insert(value.clone());
                *field = lookup
                    .iter()
                    .find(|(from, _)| from.to_string() == value)
                    .map(|(_, to)| to.to_string())
                    .unwrap_or(value);
                fields.join(" ")
            } else {
                line.to_owned()
            };
            output
                .write_all(text.as_bytes())
                .map_err(|e| Error::io(e, &path))?;
        }
        let missing: Vec<_> = lookup
            .iter()
            .filter(|(from, _)| !wrote.contains(&from.to_string()))
            .map(|(from, _)| *from)
            .collect();
        if !missing.is_empty() {
            return Err(Error::new(
                "AssertionError",
                None,
                &format!("Missing addrs from _stellantis_common.dbc: {missing:?}"),
            ));
        }
    }
    Ok(())
}
