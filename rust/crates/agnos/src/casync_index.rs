use crate::{transport, Error};
use std::{collections::HashMap, path::Path};
const CA_FORMAT_INDEX: u64 = 0x96824d9c7b129ff9;
const CA_FORMAT_TABLE: u64 = 0xe75b9e112f17417d;
#[derive(Clone, Debug, serde::Serialize)]
pub struct Chunk {
    pub sha: [u8; 32],
    pub offset: u64,
    pub length: u64,
}
pub fn parse_bytes(bytes: &[u8]) -> Result<Vec<Chunk>, Error> {
    fn number(bytes: &[u8], offset: usize) -> Result<u64, Error> {
        let data = bytes
            .get(offset..offset + 8)
            .ok_or_else(|| Error::Contract("truncated caibx".into()))?;
        Ok(u64::from_le_bytes(
            data.try_into()
                .map_err(|_| Error::Contract("truncated caibx".into()))?,
        ))
    }
    if number(bytes, 0)? != 48
        || number(bytes, 8)? != CA_FORMAT_INDEX
        || number(bytes, 56)? != CA_FORMAT_TABLE
    {
        return Err(Error::Contract("invalid caibx header".into()));
    }
    let min = number(bytes, 24)?;
    let max = number(bytes, 40)?;
    let count = bytes.len().saturating_sub(104) / 40;
    let mut chunks = Vec::with_capacity(count);
    let mut offset = 0;
    for i in 0..count {
        let start = 64 + i * 40;
        let next = number(bytes, start)?;
        let length = next
            .checked_sub(offset)
            .ok_or_else(|| Error::Contract("decreasing caibx offset".into()))?;
        if length > max || (i + 1 < count && length < min) {
            return Err(Error::Contract("invalid caibx chunk size".into()));
        }
        let sha = bytes[start + 8..start + 40]
            .try_into()
            .map_err(|_| Error::Contract("truncated caibx hash".into()))?;
        chunks.push(Chunk {
            sha,
            offset,
            length,
        });
        offset = next;
    }
    Ok(chunks)
}
pub fn parse(path: &str) -> Result<Vec<Chunk>, Error> {
    let bytes = if Path::new(path).is_file() {
        std::fs::read(path)?
    } else {
        let (status, bytes) = transport::all(&transport::agent(120, 120), path)?;
        transport::check_status(status)?;
        bytes
    };
    parse_bytes(&bytes)
}
pub fn dictionary(chunks: &[Chunk]) -> HashMap<[u8; 32], Chunk> {
    let mut result = HashMap::new();
    for chunk in chunks {
        result.entry(chunk.sha).or_insert_with(|| chunk.clone());
    }
    result
}
