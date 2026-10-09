use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct Name(Vec<u8>);

impl Name {
    pub fn hostname(host: &str) -> Option<Self> {
        let label = host.strip_suffix(".local")?;
        if label.is_empty()
            || label.len() > 63
            || !label
                .bytes()
                .all(|value| value.is_ascii_alphanumeric() || value == b'-')
        {
            return None;
        }
        let mut name = vec![u8::try_from(label.len()).ok()?];
        name.extend(label.bytes().map(|value| value.to_ascii_lowercase()));
        name.extend_from_slice(b"\x05local\0");
        Some(Self(name))
    }
}

pub(super) fn query(host: &str) -> Result<Vec<u8>, crate::Error> {
    let label = host
        .strip_suffix(".local")
        .ok_or(crate::Error::Contract("invalid mDNS hostname"))?;
    let mut packet = vec![0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0];
    packet.push(u8::try_from(label.len())?);
    packet.extend_from_slice(label.as_bytes());
    packet.extend_from_slice(b"\x05local\0\0\x01\0\x01");
    Ok(packet)
}

fn name(raw: &[u8], cursor: &mut usize) -> Option<Name> {
    let mut position = *cursor;
    let mut end = None;
    let mut labels = Vec::new();
    let mut pointers = Vec::new();
    loop {
        let length = *raw.get(position)?;
        match length & 0xc0 {
            0 => {
                position = position.checked_add(1)?;
                labels.push(length);
                if length == 0 {
                    *cursor = end.unwrap_or(position);
                    return (labels.len() <= 255).then_some(Name(labels));
                }
                let next = position.checked_add(usize::from(length))?;
                labels.extend(raw.get(position..next)?.iter().map(u8::to_ascii_lowercase));
                if labels.len() >= 255 {
                    return None;
                }
                position = next;
            }
            0xc0 => {
                let low = *raw.get(position.checked_add(1)?)?;
                let target = (usize::from(length & 0x3f) << 8) | usize::from(low);
                if target >= position || pointers.contains(&target) || pointers.len() >= 128 {
                    return None;
                }
                pointers.push(target);
                end.get_or_insert(position.checked_add(2)?);
                position = target;
            }
            _ => return None,
        }
    }
}

fn word(raw: &[u8], position: usize) -> Option<u16> {
    Some(u16::from_be_bytes(
        raw.get(position..position.checked_add(2)?)?
            .try_into()
            .ok()?,
    ))
}

pub(super) fn answers(raw: &[u8]) -> Option<Vec<(Name, IpAddr)>> {
    if word(raw, 2)? & 0x7800 != 0 {
        return None;
    }
    let questions = word(raw, 4)?;
    let counts = [word(raw, 6)?, word(raw, 8)?, word(raw, 10)?];
    let mut cursor = 12_usize;
    for _ in 0..questions {
        name(raw, &mut cursor)?;
        cursor = cursor.checked_add(4)?;
        raw.get(..cursor)?;
    }
    let mut result = Vec::new();
    for (section, count) in counts.into_iter().enumerate() {
        for _ in 0..count {
            let name = name(raw, &mut cursor)?;
            let kind = word(raw, cursor)?;
            let class = word(raw, cursor.checked_add(2)?)?;
            let length = usize::from(word(raw, cursor.checked_add(8)?)?);
            cursor = cursor.checked_add(10)?;
            let end = cursor.checked_add(length)?;
            let data = raw.get(cursor..end)?;
            if section == 0 && class == 0x8001 {
                let address = match (kind, length) {
                    (1, 4) => Some(IpAddr::V4(Ipv4Addr::from(<[u8; 4]>::try_from(data).ok()?))),
                    (28, 16) => Some(IpAddr::V6(Ipv6Addr::from(<[u8; 16]>::try_from(data).ok()?))),
                    _ => None,
                };
                if let Some(address) = address {
                    result.push((name, address));
                }
            }
            cursor = end;
        }
    }
    (cursor == raw.len()).then_some(result)
}
