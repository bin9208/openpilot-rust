use crate::Value;
use chrono::Timelike;

#[path = "../../ui-application/src/params/datetime.rs"]
mod datetime;

pub(crate) fn read(text: &str) -> Option<Value> {
    let parsed = datetime::parse(text)?;
    let mut result = parsed.local.format("%Y-%m-%d %H:%M:%S").to_string();
    if parsed.local.nanosecond() != 0 {
        result.push_str(&format!(".{:06}", parsed.local.nanosecond() / 1000));
    }
    let chars: Vec<char> = text.chars().collect();
    let split = [7, 8, 10]
        .into_iter()
        .filter(|&end| {
            chars.get(..end).is_some_and(|prefix| {
                if !prefix
                    .iter()
                    .all(|c| c.is_ascii_digit() || matches!(c, 'W' | '-'))
                {
                    return false;
                }
                if end == 10 && prefix.get(4) != Some(&'-') {
                    return false;
                }
                let date: String = prefix.iter().collect();
                datetime::parse(&date).is_some_and(|date| date.local.date() == parsed.local.date())
            })
        })
        .max()?;
    let aware = chars
        .get(split + 1..)
        .is_some_and(|suffix| suffix.iter().any(|c| matches!(c, '+' | '-' | 'Z')));
    if aware {
        let micros = parsed.offset_micros.unsigned_abs();
        let seconds = micros / 1_000_000;
        result.push_str(&format!(
            "{}{:02}:{:02}",
            if parsed.offset_micros < 0 { '-' } else { '+' },
            seconds / 3600,
            seconds / 60 % 60
        ));
        if seconds % 60 != 0 || micros % 1_000_000 != 0 {
            result.push_str(&format!(":{:02}", seconds % 60));
            if micros % 1_000_000 != 0 {
                result.push_str(&format!(".{:06}", micros % 1_000_000));
            }
        }
    }
    Some(Value::text(&result))
}
