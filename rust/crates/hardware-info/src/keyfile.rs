//! The subset of ConfigParser(interpolation=None) consumed by NM keyfile policy:
//! strict sections/options, DEFAULT inheritance, multiline values and no inline comments.
use crate::{numeric, Error};
use num_bigint::BigInt;
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn ssid_bytes(ssid: &str) -> Result<Vec<u8>, Error> {
    let input = ssid.as_bytes();
    let mut output = Vec::new();
    let mut index = 0;
    while index < input.len() {
        let byte = input[index];
        index += 1;
        if byte != b'\\' {
            output.push(byte);
            continue;
        }
        let escape = *input
            .get(index)
            .ok_or(Error::Value("trailing SSID escape"))?;
        index += 1;
        let point = match escape {
            b'\\' | b'\'' | b'"' => u32::from(escape),
            b'a' => 7,
            b'b' => 8,
            b'f' => 12,
            b'n' => 10,
            b'r' => 13,
            b't' => 9,
            b'v' => 11,
            b'\n' => continue,
            b'0'..=b'7' => {
                let mut point = u32::from(escape - b'0');
                for _ in 0..2 {
                    if let Some(digit @ b'0'..=b'7') = input.get(index) {
                        point = point * 8 + u32::from(digit - b'0');
                        index += 1;
                    } else {
                        break;
                    }
                }
                point
            }
            b'x' | b'u' | b'U' => {
                let count = match escape {
                    b'x' => 2,
                    b'u' => 4,
                    _ => 8,
                };
                let text = input
                    .get(index..index + count)
                    .ok_or(Error::Value("truncated SSID escape"))?;
                let mut point = 0_u32;
                for &byte in text {
                    point = point
                        .checked_mul(16)
                        .and_then(|v| char::from(byte).to_digit(16).and_then(|d| v.checked_add(d)))
                        .ok_or(Error::Value("invalid SSID hex escape"))?;
                }
                index += count;
                point
            }
            b'N' => {
                if input.get(index) != Some(&b'{') {
                    return Err(Error::Value("invalid named SSID escape"));
                }
                index += 1;
                let rest = &input[index..];
                let end = rest
                    .iter()
                    .position(|&byte| byte == b'}')
                    .ok_or(Error::Value("truncated named SSID escape"))?;
                let name = std::str::from_utf8(&rest[..end])
                    .map_err(|_| Error::Value("invalid SSID character name"))?
                    .to_ascii_uppercase();
                index += end + 1;
                // Only Latin-1 names can survive the source's final latin-1 encode.
                include_str!("../data/latin1-names.tsv")
                    .lines()
                    .filter_map(|line| line.split_once('\t'))
                    .find(|(candidate, _)| *candidate == name)
                    .and_then(|(_, value)| value.parse::<u32>().ok())
                    .ok_or(Error::Value("SSID name is absent or outside Latin-1"))?
            }
            _ => {
                output.push(b'\\');
                u32::from(escape)
            }
        };
        output
            .push(u8::try_from(point).map_err(|_| Error::Value("SSID escape is outside Latin-1"))?);
    }
    Ok(output)
}

pub(crate) struct Keyfile {
    sections: BTreeMap<String, BTreeMap<String, String>>,
}
impl Keyfile {
    pub fn parse(raw: &str) -> Result<Self, Error> {
        let mut sections: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
        sections.insert("DEFAULT".into(), BTreeMap::new());
        let mut seen_sections = BTreeSet::new();
        let mut seen_options = BTreeSet::new();
        let mut current: Option<String> = None;
        let mut option: Option<String> = None;
        let mut indent = 0;
        for line in raw.split_terminator('\n') {
            let value = numeric::trim(line);
            if value.starts_with('#') || value.starts_with(';') {
                continue;
            }
            if value.is_empty() {
                if let (Some(section), Some(key)) = (&current, &option) {
                    sections
                        .get_mut(section)
                        .and_then(|s| s.get_mut(key))
                        .ok_or(Error::Value("missing keyfile continuation"))?
                        .push('\n');
                }
                continue;
            }
            let current_indent = line
                .chars()
                .take_while(|&character| numeric::whitespace(character))
                .count();
            if let (Some(section), Some(key)) = (&current, &option) {
                if current_indent > indent {
                    let previous = sections
                        .get_mut(section)
                        .and_then(|s| s.get_mut(key))
                        .ok_or(Error::Value("missing keyfile continuation"))?;
                    previous.push('\n');
                    previous.push_str(value);
                    continue;
                }
            }
            indent = current_indent;
            if value.starts_with('[') {
                if let Some(end) = value.rfind(']').filter(|&end| end > 1) {
                    let section = value[1..end].to_owned();
                    if section != "DEFAULT" && !seen_sections.insert(section.clone()) {
                        return Err(Error::Value("duplicate keyfile section"));
                    }
                    sections.entry(section.clone()).or_default();
                    current = Some(section);
                    option = None;
                    continue;
                }
            }
            let section = current
                .as_ref()
                .ok_or(Error::Value("missing keyfile section"))?;
            let position = value
                .find(['=', ':'])
                .ok_or(Error::Value("invalid keyfile option"))?;
            let key = numeric::trim(&value[..position]).to_lowercase();
            if key.is_empty() || !seen_options.insert((section.clone(), key.clone())) {
                return Err(Error::Value("empty or duplicate keyfile option"));
            }
            let item = numeric::trim(&value[position + 1..]).to_owned();
            sections
                .get_mut(section)
                .ok_or(Error::Value("missing keyfile section"))?
                .insert(key.clone(), item);
            option = Some(key);
        }
        for values in sections.values_mut() {
            for value in values.values_mut() {
                *value = value.trim_end_matches(numeric::whitespace).to_owned();
            }
        }
        Ok(Self { sections })
    }
    pub fn get<'a>(&'a self, section: &str, key: &str, fallback: &'a str) -> &'a str {
        match self.sections.get(section) {
            Some(options) => options
                .get(key)
                .or_else(|| self.sections.get("DEFAULT").and_then(|s| s.get(key)))
                .map_or(fallback, String::as_str),
            None => fallback,
        }
    }
    pub fn metered(&self) -> Result<BigInt, Error> {
        numeric::integer(self.get("connection", "metered", "0"), 10)
    }
}
