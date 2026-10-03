use crate::{checksum::Kind, signal::Signal, Error};
use regex::Regex;
use serde::Serialize;
use std::{collections::BTreeMap, path::Path};

#[derive(Clone, Debug, Serialize)]
pub struct Message {
    pub name: String,
    pub address: u32,
    pub size: usize,
    pub signals: Vec<Signal>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Definition {
    pub name: String,
    pub address: u32,
    pub values: String,
}

pub type Definitions = BTreeMap<u32, BTreeMap<String, BTreeMap<i64, String>>>;

#[derive(Debug, Serialize)]
pub struct Dbc {
    pub name: String,
    pub messages: BTreeMap<u32, Message>,
    pub names: BTreeMap<String, u32>,
    pub definitions: Vec<Definition>,
}

impl Dbc {
    pub fn load(path: &Path) -> Result<Self, Error> {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| Error::Dbc("file name".into()))?
            .replace(".dbc", "");
        Self::parse(&name, &std::fs::read_to_string(path)?)
    }

    pub fn parse(name: &str, text: &str) -> Result<Self, Error> {
        let bo = Regex::new(r"^BO_ (\w+) (\w+) *: (\w+) (\w+)")?;
        let sg = Regex::new(
            r#"^SG_ (\w+) (?:\w+ *|): (\d+)\|(\d+)@(\d)([+-]) \(([0-9.+\-eE]+),([0-9.+\-eE]+)\) \[[0-9.+\-eE]+\|[0-9.+\-eE]+\] ".*" .*"#,
        )?;
        let val = Regex::new(r"^VAL_ (\w+) (\w+) (.*);")?;
        let mut dbc = Self {
            name: name.into(),
            messages: BTreeMap::new(),
            names: BTreeMap::new(),
            definitions: Vec::new(),
        };
        let mut address = 0;
        for line in text.lines().map(str::trim) {
            if let Some(m) = bo.captures(line) {
                address = parse_integer(&m[1])?;
                let message = Message {
                    name: m[2].into(),
                    address,
                    size: usize::try_from(parse_integer(&m[3])?).map_err(|_| Error::Numeric)?,
                    signals: Vec::new(),
                };
                dbc.names.insert(message.name.clone(), address);
                dbc.messages.insert(address, message);
            } else if let Some(m) = sg.captures(line) {
                let start = m[2].parse::<usize>().map_err(|_| Error::Dbc(line.into()))?;
                let size = m[3].parse::<usize>().map_err(|_| Error::Dbc(line.into()))?;
                let little = &m[4] == "1";
                let (msb, lsb) = if little {
                    (start + size - 1, start)
                } else {
                    let index = (start / 8) * 8 + 7 - start % 8;
                    let end = index + size - 1;
                    if start >= 512 || end >= 512 {
                        return Err(Error::Dbc(line.into()));
                    }
                    (start, (end / 8) * 8 + 7 - end % 8)
                };
                let signal = Signal {
                    name: m[1].into(),
                    start_bit: start,
                    msb,
                    lsb,
                    size,
                    signed: &m[5] == "-",
                    factor: m[6].parse().map_err(|_| Error::Dbc(line.into()))?,
                    offset: m[7].parse().map_err(|_| Error::Dbc(line.into()))?,
                    little_endian: little,
                    kind: Kind::for_signal(name, &m[1]),
                };
                let message = dbc
                    .messages
                    .get_mut(&address)
                    .ok_or_else(|| Error::Dbc(line.into()))?;
                if let Some(previous) = message.signals.iter_mut().find(|s| s.name == signal.name) {
                    *previous = signal;
                } else {
                    message.signals.push(signal);
                }
            } else if let Some(m) = val.captures(line) {
                let values = m[3]
                    .split('"')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_uppercase().replace(' ', "_"))
                    .collect::<Vec<_>>()
                    .join(" ");
                dbc.definitions.push(Definition {
                    name: m[2].into(),
                    address: parse_integer(&m[1])?,
                    values,
                });
            }
        }
        Ok(dbc)
    }

    pub fn message(&self, name: &str) -> Result<&Message, Error> {
        let address = self
            .names
            .get(name)
            .ok_or_else(|| Error::Message(name.into()))?;
        self.messages
            .get(address)
            .ok_or_else(|| Error::Message(name.into()))
    }

    pub fn definitions(&self) -> Result<Definitions, Error> {
        let mut values = Definitions::new();
        for definition in &self.definitions {
            if !self.messages.contains_key(&definition.address) {
                return Err(Error::Message(definition.address.to_string()));
            }
            let parts: Vec<_> = definition.values.split_whitespace().collect();
            if parts.len() % 2 != 0 {
                return Err(Error::Dbc(definition.values.clone()));
            }
            let mut entry = BTreeMap::new();
            for pair in parts.chunks_exact(2) {
                entry.insert(
                    pair[0]
                        .parse()
                        .map_err(|_| Error::Dbc(definition.values.clone()))?,
                    pair[1].into(),
                );
            }
            values
                .entry(definition.address)
                .or_default()
                .insert(definition.name.clone(), entry);
        }
        Ok(values)
    }
}

fn parse_integer(raw: &str) -> Result<u32, Error> {
    let parsed = match raw.strip_prefix("0x").or_else(|| raw.strip_prefix("0X")) {
        Some(hex) => u32::from_str_radix(hex, 16),
        None => raw.parse(),
    };
    parsed.map_err(|_| Error::Dbc(raw.into()))
}
