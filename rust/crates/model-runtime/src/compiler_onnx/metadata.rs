//! Inert decoding of model output-slice metadata. REDUCE constructs only a
//! slice value; globals are never imported and code is never executed.
use super::*;
#[derive(Clone, Debug)]
enum Item {
    None,
    Integer(i64),
    String(String),
    SliceGlobal,
    Tuple([Option<i64>; 3]),
    Slice([Option<i64>; 3]),
    Dict,
    Mark,
}
fn take<'a>(bytes: &'a [u8], index: &mut usize, count: usize) -> Result<&'a [u8], String> {
    let end = index.checked_add(count).ok_or("pickle size overflow")?;
    let v = bytes.get(*index..end).ok_or("truncated slice pickle")?;
    *index = end;
    Ok(v)
}
fn number(item: Item) -> Result<Option<i64>, String> {
    match item {
        Item::None => Ok(None),
        Item::Integer(v) => Ok(Some(v)),
        _ => Err("slice endpoint is not an integer or None".into()),
    }
}
fn pop(stack: &mut Vec<Item>) -> Result<Item, String> {
    stack.pop().ok_or("slice pickle stack underflow".into())
}
pub fn decode_pickle_slices(bytes: &[u8]) -> Result<BTreeMap<String, [Option<i64>; 3]>, String> {
    if bytes.len() > 1024 * 1024 {
        return Err("slice pickle exceeds 1 MiB".into());
    }
    let (mut index, mut stack, mut memo, mut output, mut dictionary) =
        (0, Vec::new(), BTreeMap::new(), BTreeMap::new(), false);
    while index < bytes.len() {
        let opcode = take(bytes, &mut index, 1)?[0];
        match opcode {
            0x80 => {
                if !matches!(take(bytes, &mut index, 1)?[0], 2..=5) {
                    return Err("unsupported slice pickle protocol".into());
                }
            }
            0x95 => {
                let n = u64::from_le_bytes(take(bytes, &mut index, 8)?.try_into().unwrap());
                if n > (bytes.len() - index) as u64 {
                    return Err("truncated pickle frame".into());
                }
            }
            b'}' => {
                if dictionary || !stack.is_empty() {
                    return Err("nested slice dictionary unsupported".into());
                }
                dictionary = true;
                stack.push(Item::Dict);
            }
            b'(' => stack.push(Item::Mark),
            b'N' => stack.push(Item::None),
            b'K' => stack.push(Item::Integer(take(bytes, &mut index, 1)?[0] as i64)),
            b'M' => stack.push(Item::Integer(u16::from_le_bytes(
                take(bytes, &mut index, 2)?.try_into().unwrap(),
            ) as i64)),
            b'J' => stack.push(Item::Integer(i32::from_le_bytes(
                take(bytes, &mut index, 4)?.try_into().unwrap(),
            ) as i64)),
            0x8a => {
                let n = take(bytes, &mut index, 1)?[0] as usize;
                if n == 0 {
                    stack.push(Item::Integer(0));
                } else {
                    if n > 8 {
                        return Err("slice integer exceeds i64".into());
                    }
                    let raw = take(bytes, &mut index, n)?;
                    let mut v = if raw[n - 1] & 0x80 != 0 {
                        [0xff; 8]
                    } else {
                        [0; 8]
                    };
                    v[..n].copy_from_slice(raw);
                    stack.push(Item::Integer(i64::from_le_bytes(v)));
                }
            }
            0x8c | b'X' => {
                let n = if opcode == 0x8c {
                    take(bytes, &mut index, 1)?[0] as usize
                } else {
                    u32::from_le_bytes(take(bytes, &mut index, 4)?.try_into().unwrap()) as usize
                };
                if n > 4096 {
                    return Err("slice name exceeds limit".into());
                }
                let s = std::str::from_utf8(take(bytes, &mut index, n)?)
                    .map_err(|_| "invalid slice UTF8")?;
                stack.push(Item::String(s.into()));
            }
            0x93 => {
                let name = pop(&mut stack)?;
                let module = pop(&mut stack)?;
                match (module, name) {
                    (Item::String(module), Item::String(name))
                        if module == "builtins" && name == "slice" =>
                    {
                        stack.push(Item::SliceGlobal)
                    }
                    _ => return Err("forbidden slice pickle global".into()),
                }
            }
            b'c' => {
                let start = index;
                let end = bytes[index..]
                    .iter()
                    .position(|&b| b == b'\n')
                    .ok_or("truncated pickle global")?
                    + index;
                index = end + 1;
                let module = &bytes[start..end];
                let start = index;
                let end = bytes[index..]
                    .iter()
                    .position(|&b| b == b'\n')
                    .ok_or("truncated pickle global")?
                    + index;
                index = end + 1;
                if !matches!(module, b"builtins" | b"__builtin__") || &bytes[start..end] != b"slice"
                {
                    return Err("forbidden slice pickle global".into());
                }
                stack.push(Item::SliceGlobal);
            }
            0x87 => {
                let step = number(pop(&mut stack)?)?;
                let stop = number(pop(&mut stack)?)?;
                let start = number(pop(&mut stack)?)?;
                stack.push(Item::Tuple([start, stop, step]));
            }
            b'R' => {
                let arguments = pop(&mut stack)?;
                let target = pop(&mut stack)?;
                match (target, arguments) {
                    (Item::SliceGlobal, Item::Tuple(v)) => stack.push(Item::Slice(v)),
                    _ => return Err("forbidden slice pickle reduction".into()),
                }
            }
            0x94 | b'q' | b'r' => {
                let key = match opcode {
                    0x94 => memo.len() as u32,
                    b'q' => take(bytes, &mut index, 1)?[0] as u32,
                    _ => u32::from_le_bytes(take(bytes, &mut index, 4)?.try_into().unwrap()),
                };
                memo.insert(key, stack.last().ok_or("empty pickle memo")?.clone());
            }
            b'h' | b'j' => {
                let key = if opcode == b'h' {
                    take(bytes, &mut index, 1)?[0] as u32
                } else {
                    u32::from_le_bytes(take(bytes, &mut index, 4)?.try_into().unwrap())
                };
                let value = memo.get(&key).ok_or("unknown pickle memo")?.clone();
                if matches!(value, Item::Dict | Item::Mark) {
                    return Err("invalid pickle memo value".into());
                }
                stack.push(value);
            }
            b'u' => {
                let mark = stack
                    .iter()
                    .rposition(|i| matches!(i, Item::Mark))
                    .ok_or("slice dictionary mark missing")?;
                if mark != 1 || !matches!(stack[0], Item::Dict) || (stack.len() - mark - 1) % 2 != 0
                {
                    return Err("invalid slice dictionary items".into());
                }
                let items = stack.drain(mark + 1..).collect::<Vec<_>>();
                stack.pop();
                for pair in items.chunks_exact(2) {
                    match (&pair[0], &pair[1]) {
                        (Item::String(name), Item::Slice(value)) => {
                            output.insert(name.clone(), *value);
                        }
                        _ => return Err("slice dictionary requires named slices".into()),
                    }
                }
            }
            b'.' => {
                if index != bytes.len()
                    || stack.len() != 1
                    || !matches!(stack[0], Item::Dict)
                    || output.is_empty()
                {
                    return Err("invalid slice pickle result".into());
                }
                return Ok(output);
            }
            _ => return Err(format!("unsupported slice pickle opcode {opcode:#x}")),
        }
        if stack.len() > 4096 || memo.len() > 4096 || output.len() > 1024 {
            return Err("slice pickle exceeds structural limits".into());
        }
    }
    Err("slice pickle missing STOP".into())
}
fn base64(text: &str) -> Result<Vec<u8>, String> {
    if text.len() > 2 * 1024 * 1024 {
        return Err("slice base64 exceeds limit".into());
    }
    let mut output = Vec::new();
    let (mut value, mut bits, mut padded) = (0u32, 0u32, false);
    for byte in text.bytes().filter(|b| !b.is_ascii_whitespace()) {
        if byte == b'=' {
            padded = true;
            continue;
        }
        if padded {
            return Err("data after base64 padding".into());
        }
        let n = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return Err("invalid slice base64".into()),
        };
        value = (value << 6) | n as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            output.push((value >> bits) as u8);
        }
    }
    if bits >= 6 {
        return Err("truncated slice base64".into());
    }
    Ok(output)
}
pub(super) fn pipeline(model: &Model) -> Result<Metadata, String> {
    let checkpoint = model
        .metadata_properties
        .get("model_checkpoint")
        .filter(|s| !s.is_empty() && s.len() <= 4096)
        .ok_or("model checkpoint missing or invalid")?
        .clone();
    if model.graph.outputs.len() != 1 {
        return Err("model requires one flat output".into());
    }
    let mut total = 1i64;
    for dim in &model.graph.outputs[0].shape {
        match dim {
            ShapeDimension::Fixed(size) if *size > 0 => {
                total = total
                    .checked_mul(*size)
                    .filter(|v| *v <= 1024 * 1024 * 1024)
                    .ok_or("model tensor too large")?;
            }
            _ => return Err("static positive model shapes required".into()),
        }
    }
    for value in model.graph.inputs.iter().chain(&model.graph.outputs) {
        if value.shape.is_empty() || value.shape.len() > 16 {
            return Err("invalid model shape".into());
        }
        let mut n = 1i64;
        for dim in &value.shape {
            match dim {
                ShapeDimension::Fixed(size) if *size > 0 => {
                    n = n
                        .checked_mul(*size)
                        .filter(|v| *v <= 1024 * 1024 * 1024)
                        .ok_or("model tensor too large")?;
                }
                _ => return Err("static positive model shapes required".into()),
            }
        }
    }
    let encoded = model
        .metadata_properties
        .get("output_slices")
        .ok_or("output_slices not found in metadata")?;
    let raw = decode_pickle_slices(&base64(encoded)?)?;
    let mut slices = BTreeMap::new();
    for (name, [start, stop, step]) in raw {
        if name.is_empty() || !matches!(step, None | Some(1)) {
            return Err("contiguous named model output slices required".into());
        }
        if [start, stop]
            .iter()
            .flatten()
            .any(|v| *v < -total || *v > total)
        {
            return Err("model output slice outside tensor".into());
        }
        let normalize = |v: i64| if v < 0 { total + v } else { v };
        let start = normalize(start.unwrap_or(0));
        let stop = normalize(stop.unwrap_or(total));
        if start >= stop {
            return Err("empty model output slice".into());
        }
        slices.insert(name, (start as usize, stop as usize));
    }
    Ok(Metadata { checkpoint, slices })
}
