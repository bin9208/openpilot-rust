use crate::route_replay::Error;
use serde::{de::DeserializeOwned, Serialize};
use std::{
    io::{Read, Write},
    os::unix::net::UnixStream,
};

pub fn encode(value: &impl Serialize) -> Result<Vec<u8>, Error> {
    let body = serde_json::to_vec(value)?;
    let length = u64::try_from(body.len()).map_err(|error| Error::Invalid(error.to_string()))?;
    let mut data = length.to_le_bytes().to_vec();
    data.extend_from_slice(&body);
    Ok(data)
}
pub fn write(stream: &mut UnixStream, value: &impl Serialize) -> Result<(), Error> {
    stream.write_all(&encode(value)?)?;
    Ok(())
}
pub fn read<T: DeserializeOwned>(stream: &mut UnixStream) -> Result<T, Error> {
    let mut header = [0; 8];
    stream.read_exact(&mut header)?;
    let length = usize::try_from(u64::from_le_bytes(header))
        .map_err(|error| Error::Invalid(error.to_string()))?;
    let mut body = Vec::new();
    body.try_reserve_exact(length)
        .map_err(|error| Error::Invalid(error.to_string()))?;
    body.resize(length, 0);
    stream.read_exact(&mut body)?;
    Ok(serde_json::from_slice(&body)?)
}
pub fn decode<T: DeserializeOwned>(buffer: &mut Vec<u8>) -> Result<Option<T>, Error> {
    let Some(header) = buffer.get(..8) else {
        return Ok(None);
    };
    let header: [u8; 8] = header
        .try_into()
        .map_err(|_| Error::Invalid("invalid preload header".into()))?;
    let length = usize::try_from(u64::from_le_bytes(header))
        .map_err(|error| Error::Invalid(error.to_string()))?;
    let end = length
        .checked_add(8)
        .ok_or_else(|| Error::Invalid("preload response size overflow".into()))?;
    let Some(body) = buffer.get(8..end) else {
        return Ok(None);
    };
    let value = serde_json::from_slice(body)?;
    buffer.drain(..end);
    Ok(Some(value))
}
