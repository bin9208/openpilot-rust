//! Params TIME conversion follows Python 3.12 datetime.fromisoformat.
use chrono::{Datelike, NaiveDate, NaiveDateTime, Weekday};
#[derive(Clone, Copy, Debug)]
pub struct DateTime {
    pub local: NaiveDateTime,
    pub offset_micros: i64,
}
fn number(value: &[u8]) -> Option<u32> {
    if value.is_empty() || !value.iter().all(u8::is_ascii_digit) {
        return None;
    }
    value.iter().try_fold(0u32, |acc, c| {
        acc.checked_mul(10)?.checked_add(u32::from(c - b'0'))
    })
}
fn separator(value: &[char]) -> Option<usize> {
    if value.len() < 7 {
        return None;
    }
    if value.len() == 7 {
        return Some(7);
    }
    if value[4] == '-' {
        if value[5] != 'W' {
            return Some(10);
        }
        if value.get(8) != Some(&'-') {
            return Some(8);
        }
        if value.len() == 9 {
            return None;
        }
        return Some(if value.get(10).is_some_and(char::is_ascii_digit) {
            8
        } else {
            10
        });
    }
    if value[4] != 'W' {
        return Some(8);
    }
    let mut index = 7;
    while value.get(index).is_some_and(char::is_ascii_digit) {
        index += 1;
    }
    Some(if index < 9 {
        index
    } else if index % 2 == 0 {
        7
    } else {
        8
    })
}
fn date(value: &[u8]) -> Option<NaiveDate> {
    if ![7, 8, 10].contains(&value.len()) {
        return None;
    }
    let year = i32::try_from(number(value.get(..4)?)?).ok()?;
    if !(1..=9999).contains(&year) {
        return None;
    }
    let dash = value[4] == b'-';
    let mut index = 4 + usize::from(dash);
    if value.get(index) == Some(&b'W') {
        index += 1;
        let week = number(value.get(index..index + 2)?)?;
        index += 2;
        let day = if index == value.len() {
            1
        } else {
            if (value.get(index) == Some(&b'-')) != dash {
                return None;
            }
            index += usize::from(dash);
            if index + 1 != value.len() {
                return None;
            }
            number(value.get(index..index + 1)?)?
        };
        let weekday = match day {
            1 => Weekday::Mon,
            2 => Weekday::Tue,
            3 => Weekday::Wed,
            4 => Weekday::Thu,
            5 => Weekday::Fri,
            6 => Weekday::Sat,
            7 => Weekday::Sun,
            _ => return None,
        };
        return NaiveDate::from_isoywd_opt(year, week, weekday)
            .filter(|d| (1..=9999).contains(&d.year()));
    }
    let month = number(value.get(index..index + 2)?)?;
    index += 2;
    if (value.get(index) == Some(&b'-')) != dash {
        return None;
    }
    index += usize::from(dash);
    if index + 2 != value.len() {
        return None;
    }
    NaiveDate::from_ymd_opt(year, month, number(value.get(index..index + 2)?)?)
}
fn time(value: &[u8], end: usize) -> Option<([u32; 4], bool)> {
    let mut result = [0; 4];
    let mut index = 0;
    let mut colon = false;
    for (component, output) in result.iter_mut().take(3).enumerate() {
        *output = number(value.get(index..index + 2)?)?;
        index += 2;
        let next = value.get(index).copied().unwrap_or(0);
        index += 1;
        if component == 0 {
            colon = next == b':';
        }
        if index >= end {
            return Some((result, next != 0));
        }
        if colon && next == b':' {
            continue;
        }
        if matches!(next, b'.' | b',') {
            break;
        }
        if colon {
            return None;
        }
        index -= 1;
    }
    let count = end.checked_sub(index)?.min(6);
    result[3] =
        number(value.get(index..index + count)?)? * 10u32.pow(u32::try_from(6 - count).ok()?);
    index += count;
    while value.get(index).is_some_and(u8::is_ascii_digit) {
        index += 1;
    }
    Some((result, value.get(index).copied().unwrap_or(0) != 0))
}

pub fn parse(value: &str) -> Option<DateTime> {
    let chars: Vec<_> = value.chars().collect();
    let split = separator(&chars)?;
    let prefix: String = chars.get(..split)?.iter().collect();
    let date = date(prefix.as_bytes())?;
    if split == chars.len() {
        return Some(DateTime {
            local: date.and_hms_opt(0, 0, 0)?,
            offset_micros: 0,
        });
    }
    let suffix: String = chars.get(split + 1..)?.iter().collect();
    let bytes = suffix.as_bytes();
    let zone = bytes.iter().position(|c| matches!(c, b'+' | b'-' | b'Z'));
    let (components, trailing) = time(bytes, zone.unwrap_or(bytes.len()))?;
    if zone.is_none() && trailing {
        return None;
    }
    let local =
        date.and_hms_micro_opt(components[0], components[1], components[2], components[3])?;
    let offset_micros = if let Some(index) = zone {
        if bytes[index] == b'Z' {
            if bytes.get(index + 1).copied().unwrap_or(0) != 0 {
                return None;
            }
            0
        } else {
            let zone_bytes = &bytes[index + 1..];
            let (c, trailing) = time(zone_bytes, zone_bytes.len())?;
            if trailing {
                return None;
            }
            // CPython treats any zero whole-second timezone as UTC, discarding its fractional part.
            let seconds = i64::from(c[0]) * 3600 + i64::from(c[1]) * 60 + i64::from(c[2]);
            let micros = if seconds == 0 {
                0
            } else {
                seconds * 1_000_000 + i64::from(c[3])
            };
            if micros >= 86_400_000_000 {
                return None;
            }
            if bytes[index] == b'-' {
                -micros
            } else {
                micros
            }
        }
    } else {
        0
    };
    Some(DateTime {
        local,
        offset_micros,
    })
}
