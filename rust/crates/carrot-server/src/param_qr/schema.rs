use super::{base, text};
use crate::{params::Backend, Error, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone)]
pub struct Schema {
    pub names: Vec<String>,
    pub types: Vec<(String, u8)>,
}

impl Schema {
    pub fn from_backend(backend: &Backend) -> Result<Self, Error> {
        let (names, types) = backend.backup_schema()?;
        Ok(Self { names, types })
    }

    pub(crate) fn for_values(values: &Value) -> Result<Self, Error> {
        Ok(Self {
            names: crate::json_fields::fields(values)?
                .iter()
                .map(|(key, _)| text::utf8(key))
                .collect::<Result<_, _>>()?,
            types: Vec::new(),
        })
    }

    fn sorted(&self) -> BTreeSet<&str> {
        self.names.iter().map(String::as_str).collect()
    }

    pub(crate) fn codes(&self, size: usize) -> BTreeMap<Vec<u8>, String> {
        let mut buckets: BTreeMap<Vec<u8>, Vec<&str>> = BTreeMap::new();
        for name in self.sorted() {
            buckets
                .entry(Sha256::digest(name.as_bytes())[..size].to_vec())
                .or_default()
                .push(name);
        }
        buckets
            .into_iter()
            .filter_map(|(code, keys)| (keys.len() == 1).then(|| (code, keys[0].into())))
            .collect()
    }

    pub(crate) fn key_codes(&self, size: usize) -> BTreeMap<String, Vec<u8>> {
        self.codes(size)
            .into_iter()
            .map(|(code, key)| (key, code))
            .collect()
    }

    pub(crate) fn string_codes(&self) -> BTreeMap<String, String> {
        self.codes(3)
            .into_iter()
            .map(|(code, key)| (base::b64_encode(&code), key))
            .collect()
    }

    pub(crate) fn fingerprint(&self) -> Vec<u8> {
        let mut bytes = vec![2];
        bytes.extend(
            self.sorted()
                .into_iter()
                .collect::<Vec<_>>()
                .join("\n")
                .bytes(),
        );
        Sha256::digest(bytes)[..4].to_vec()
    }

    pub(crate) fn kind(&self, name: &str) -> Option<u8> {
        self.types
            .iter()
            .find(|(key, _)| name == key)
            .map(|(_, kind)| *kind)
    }
}
