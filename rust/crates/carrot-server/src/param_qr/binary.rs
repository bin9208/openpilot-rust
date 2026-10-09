use super::{error, schema::Schema, text, values, Codec};
use crate::{Error, Value};
use num_bigint::BigInt;

pub(crate) fn build(codec: &Codec, input: &Value, version: u8) -> Result<Vec<u8>, Error> {
    let fallback_schema;
    let schema = match &codec.schema {
        Some(schema) => schema,
        None => {
            fallback_schema = Schema::for_values(input)?;
            &fallback_schema
        }
    };
    let codes = schema.key_codes(2);
    let mut fields: Vec<_> = crate::json_fields::fields(input)?.iter().collect();
    fields.sort_by(|left, right| left.0.cmp(&right.0));
    let mut pairs = Vec::new();
    let mut fallback = Vec::new();
    for (key, value) in fields {
        let name = key
            .iter()
            .copied()
            .map(char::from_u32)
            .collect::<Option<String>>();
        let encoded = values::encode(value, name.as_ref().and_then(|name| schema.kind(name)))?;
        let name = text::utf8(key)?;
        if let Some(code) = codes.get(&name) {
            pairs.push((code.clone(), encoded));
        } else {
            fallback.push((name.into_bytes(), encoded));
        }
    }
    let mut raw = vec![version, 2];
    raw.extend(schema.fingerprint());
    raw.extend(values::varint(BigInt::from(pairs.len()))?);
    for (code, encoded) in pairs {
        raw.extend(code);
        raw.extend(encoded);
    }
    raw.extend(values::varint(BigInt::from(fallback.len()))?);
    for (key, encoded) in fallback {
        raw.extend(values::varint(BigInt::from(key.len()))?);
        raw.extend(key);
        raw.extend(encoded);
    }
    Ok(raw)
}

pub(crate) fn assign(fields: &mut Vec<(Vec<u32>, Value)>, name: Vec<u32>, value: Value) {
    if let Some((_, current)) = fields.iter_mut().find(|(key, _)| *key == name) {
        *current = value;
    } else {
        fields.push((name, value));
    }
}

pub(crate) fn parse(codec: &Codec, raw: &[u8]) -> Result<Value, Error> {
    if raw.len() < 6 {
        return Err(error("bad QR backup format"));
    }
    if raw[0] > 4 {
        return Err(error("unsupported QR backup version"));
    }
    let code_size = usize::from(raw[1]);
    if !(1..=8).contains(&code_size) {
        return Err(error("bad QR backup code size"));
    }
    let codes = codec.required_schema()?.codes(code_size);
    let mut pos = 6;
    let mut output = Vec::new();
    let count = values::size(raw, &mut pos)?;
    for _ in 0..count {
        let end = pos
            .checked_add(code_size)
            .filter(|end| *end <= raw.len())
            .ok_or_else(|| error("bad QR backup code"))?;
        let code = &raw[pos..end];
        pos = end;
        let value = values::decode(raw, &mut pos)?;
        if let Some(name) = codes.get(code) {
            assign(&mut output, name.chars().map(u32::from).collect(), value);
        }
    }
    let count = values::size(raw, &mut pos)?;
    for _ in 0..count {
        let key_size = values::size(raw, &mut pos)?;
        let end = pos
            .checked_add(key_size)
            .filter(|end| *end <= raw.len())
            .ok_or_else(|| error("bad QR backup key"))?;
        let name = text::decode(&raw[pos..end])?;
        pos = end;
        let value = values::decode(raw, &mut pos)?;
        assign(&mut output, name.chars().map(u32::from).collect(), value);
    }
    if pos != raw.len() {
        return Err(error("bad QR backup trailing data"));
    }
    Ok(Value::Object(output))
}
