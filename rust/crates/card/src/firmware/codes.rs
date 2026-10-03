use super::Brand;
use std::collections::BTreeSet;

pub(super) type Code = (Vec<u8>, Option<Vec<u8>>);

fn alpha_ford(byte: u8) -> bool {
    matches!(byte, b'A'..=b'H' | b'J'..=b'N' | b'P'..=b'V' | b'X'..=b'Z')
}
fn alpha_number(byte: u8) -> bool {
    byte.is_ascii_uppercase() || byte.is_ascii_digit()
}

fn ford(mut data: &[u8]) -> Option<Code> {
    if data.last() == Some(&b'\n') {
        data = &data[..data.len() - 1];
    }
    while data.last() == Some(&0) {
        data = &data[..data.len() - 1];
    }
    if !data.first().is_some_and(|byte| alpha_ford(*byte)) {
        return None;
    }
    let fields: Vec<_> = data.split(|byte| *byte == b'-').collect();
    if fields.len() != 3
        || fields[0].len() != 4
        || !(5..=6).contains(&fields[1].len())
        || fields[2].len() < 2
    {
        return None;
    }
    if !fields[0][1..]
        .iter()
        .chain(fields[1])
        .all(|byte| byte.is_ascii_digit() || alpha_ford(*byte))
        || !fields[2].iter().all(|byte| alpha_ford(*byte))
    {
        return None;
    }
    Some((fields[0][1..].to_vec(), Some(vec![data[0]])))
}

fn hyundai(data: &[u8]) -> Option<Code> {
    let start = data.windows(4).position(|bytes| {
        bytes[..2] == [0xf1, 0] && bytes[2..].iter().all(u8::is_ascii_uppercase)
    })? + 2;
    let count = data[start + 2..]
        .iter()
        .take(2)
        .take_while(|byte| byte.is_ascii_alphanumeric())
        .count();
    let mut code = data[start..start + 2 + count].to_vec();
    for start in 0..data.len().saturating_sub(5) {
        let prefix = &data[start..start + 5];
        if prefix[0].is_ascii_digit()
            && matches!(prefix[1], b'.' | b',')
            && prefix[2..4].iter().all(u8::is_ascii_digit)
            && prefix[4] == b' '
        {
            let part = &data[start + 5..];
            if part.len() < 10 || !part[..5].iter().all(u8::is_ascii_digit) {
                continue;
            }
            let offset = if matches!(part.get(5), Some(b'-' | b'/')) {
                6
            } else {
                5
            };
            let Some(suffix) = part.get(offset..offset + 5) else {
                continue;
            };
            if suffix[0].is_ascii_uppercase()
                && suffix[1..4].iter().all(|byte| alpha_number(*byte))
                && suffix[4].is_ascii_digit()
            {
                code.push(b'-');
                code.extend_from_slice(suffix);
                break;
            }
        }
    }
    let date_data = if data.last() == Some(&b'\n') {
        &data[..data.len() - 1]
    } else {
        data
    };
    let date = if date_data.len() >= 7
        && matches!(date_data[date_data.len() - 7], b' ' | b'-')
        && date_data[date_data.len() - 6..]
            .iter()
            .all(u8::is_ascii_digit)
    {
        Some(date_data[date_data.len() - 6..].to_vec())
    } else {
        None
    };
    Some((code, date))
}

fn toyota(mut data: &[u8]) -> Option<Code> {
    let count = match data.first() {
        Some(1..=3) => {
            let count = usize::from(data[0]);
            data = &data[1..];
            count
        }
        _ => 1,
    };
    if data.len() != count * 16 {
        return None;
    }
    data = &data[..16];
    while data.first().is_some_and(|byte| matches!(byte, 0 | b' ')) {
        data = &data[1..];
    }
    while data.last().is_some_and(|byte| matches!(byte, 0 | b' ')) {
        data = &data[..data.len() - 1];
    }
    if !data.iter().all(|byte| alpha_number(*byte)) {
        return None;
    }
    let code = match data.len() {
        8 => [data[1..3].to_vec(), data[3..5].to_vec()],
        10 => [data[..7].to_vec(), data[7..8].to_vec()],
        12 => [data[..7].to_vec(), data[7..9].to_vec()],
        _ => return None,
    };
    let mut result = Vec::new();
    if data.len() > 8 {
        result.extend_from_slice(&code[0][..5]);
        result.push(b'-');
        result.extend_from_slice(&code[0][5..]);
    } else {
        result.extend_from_slice(&code[0]);
    }
    result.push(b'-');
    result.extend_from_slice(&code[1]);
    Some((result, None))
}

pub(super) fn extract(
    brand: Brand,
    versions: impl Iterator<Item = impl AsRef<[u8]>>,
) -> BTreeSet<Code> {
    versions
        .filter_map(|version| match brand {
            Brand::Ford => ford(version.as_ref()),
            Brand::Hyundai => hyundai(version.as_ref()),
            Brand::Toyota => toyota(version.as_ref()),
            Brand::Body
            | Brand::Chrysler
            | Brand::Gm
            | Brand::Honda
            | Brand::Mazda
            | Brand::Mock
            | Brand::Nissan
            | Brand::Psa
            | Brand::Rivian
            | Brand::Subaru
            | Brand::Tesla
            | Brand::Volkswagen => None,
        })
        .collect()
}
