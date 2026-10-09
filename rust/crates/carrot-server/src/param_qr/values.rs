use super::{digits, error, text};
use crate::{Error, Value};
use num_bigint::BigInt;
use num_traits::{ToPrimitive, Zero};

pub(crate) fn varint(mut number: BigInt) -> Result<Vec<u8>, Error> {
    if number < BigInt::zero() {
        return Err(error("negative varint"));
    }
    let mut output = Vec::new();
    loop {
        let byte = (&number & BigInt::from(127))
            .to_u8()
            .ok_or_else(|| error("bad varint"))?;
        number >>= 7;
        output.push(byte | if number.is_zero() { 0 } else { 128 });
        if number.is_zero() {
            return Ok(output);
        }
    }
}

pub(crate) fn read_varint(data: &[u8], pos: &mut usize) -> Result<BigInt, Error> {
    let mut shift = 0;
    let mut value = BigInt::zero();
    loop {
        if *pos >= data.len() || shift > 63 {
            return Err(error("bad varint"));
        }
        let byte = data[*pos];
        *pos += 1;
        value |= BigInt::from(byte & 127) << shift;
        if byte & 128 == 0 {
            return Ok(value);
        }
        shift += 7;
    }
}

pub(crate) fn size(data: &[u8], pos: &mut usize) -> Result<usize, Error> {
    Ok(read_varint(data, pos)?.to_usize().unwrap_or(usize::MAX))
}

fn integer_text(points: &[u32]) -> bool {
    if points == [48] {
        return true;
    }
    let body = points.strip_prefix(&[45]).unwrap_or(points);
    !body.is_empty()
        && body.iter().all(|point| digits::is_digit(*point))
        && !(body[0] == 48 && (body.len() > 1 || body.len() == points.len()))
}

fn int_value(value: &Value) -> Result<BigInt, Error> {
    let number = value.float()?;
    Ok(Value::Float(number.round_ties_even()).int()?)
}

pub(crate) fn encode(value: &Value, kind: Option<u8>) -> Result<Vec<u8>, Error> {
    if kind == Some(1) {
        let enabled = match value {
            Value::Text(_) => ["1", "true", "True", "on", "yes"]
                .iter()
                .any(|text| value.text_eq(text)),
            _ => value.truth(),
        };
        return Ok(vec![if enabled { 2 } else { 1 }]);
    }
    let Value::Text(points) = value.py_string()? else {
        return Err(error("expected text"));
    };
    match points.as_slice() {
        [] => return Ok(vec![0]),
        [48] => return Ok(vec![1]),
        [49] => return Ok(vec![2]),
        _ => {}
    }
    let integer = if kind == Some(2) {
        int_value(value).ok()
    } else if kind.is_none() && integer_text(&points) {
        Some(Value::Text(points.clone()).int()?)
    } else {
        None
    };
    if let Some(integer) = integer {
        let mut output = vec![3];
        match varint((&integer << 1) ^ (&integer >> 63)) {
            Ok(encoded) => {
                output.extend(encoded);
                return Ok(output);
            }
            Err(_) if kind == Some(2) => {}
            Err(failure) => return Err(failure),
        }
    }
    let raw = text::utf8(&points)?;
    let mut output = vec![4];
    output.extend(varint(BigInt::from(raw.len()))?);
    output.extend(raw.bytes());
    Ok(output)
}

pub(crate) fn decode(data: &[u8], pos: &mut usize) -> Result<Value, Error> {
    let tag = *data.get(*pos).ok_or_else(|| error("bad QR backup value"))?;
    *pos += 1;
    let result = match tag {
        0 => String::new(),
        1 => "0".into(),
        2 => "1".into(),
        3 => {
            let number = read_varint(data, pos)?;
            let decoded: BigInt = (&number >> 1) ^ -(&number & BigInt::from(1));
            decoded.to_string()
        }
        4 => {
            let size = size(data, pos)?;
            let end = pos
                .checked_add(size)
                .filter(|end| *end <= data.len())
                .ok_or_else(|| error("bad QR backup string"))?;
            let decoded = text::decode(&data[*pos..end])?;
            *pos = end;
            decoded
        }
        _ => return Err(error("unsupported QR backup value")),
    };
    Ok(Value::text(&result))
}
