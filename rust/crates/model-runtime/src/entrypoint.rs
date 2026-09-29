use crate::Error;
use serde::Deserialize;
use std::{collections::HashSet, ops::Range};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Entrypoint {
    pub name: String,
    pub start: usize,
    pub end: usize,
}

impl Entrypoint {
    pub fn range(&self) -> Range<usize> {
        self.start..self.end
    }
}

pub(crate) fn validate(version: u32, entries: &[Entrypoint], calls: usize) -> Result<(), Error> {
    if version == 1 && entries.is_empty() {
        return Ok(());
    }
    if version != 2 || entries.is_empty() || entries.len() > 65536 {
        return Err(Error::Contract("entrypoint version/count"));
    }
    let mut names = HashSet::new();
    let mut end = 0;
    for entry in entries {
        if entry.name.is_empty()
            || !names.insert(&entry.name)
            || entry.start != end
            || entry.end < entry.start
            || entry.end > calls
        {
            return Err(Error::Contract("entrypoint partition"));
        }
        end = entry.end;
    }
    if end != calls {
        return Err(Error::Contract("entrypoint coverage"));
    }
    Ok(())
}

pub(crate) fn find(entries: &[Entrypoint], name: &str) -> Result<usize, Error> {
    entries
        .iter()
        .position(|entry| entry.name == name)
        .ok_or_else(|| Error::Entrypoint(name.to_owned()))
}
