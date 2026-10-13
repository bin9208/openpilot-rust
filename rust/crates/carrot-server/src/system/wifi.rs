use crate::Value;
use num_bigint::BigInt;
use num_traits::Zero;

pub(super) fn strip(text: &str) -> &str {
    text.trim_matches(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c))
}

pub(super) fn lines(text: &str) -> Vec<&str> {
    let split = |c| {
        matches!(
            c,
            '\n' | '\r'
                | '\x0b'
                | '\x0c'
                | '\x1c'
                | '\x1d'
                | '\x1e'
                | '\u{85}'
                | '\u{2028}'
                | '\u{2029}'
        )
    };
    let mut lines: Vec<_> = text.split(split).collect();
    if text.chars().last().is_some_and(split) {
        lines.pop();
    }
    lines
}

pub(super) fn parts(line: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut buffer = String::new();
    let mut escaped = false;
    for character in line.chars() {
        if escaped {
            buffer.push(character);
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == ':' {
            parts.push(std::mem::take(&mut buffer));
        } else {
            buffer.push(character);
        }
    }
    parts.push(buffer);
    parts
}

struct Entry {
    ssid: String,
    connected: bool,
    security: String,
    signal: Option<BigInt>,
}

impl Entry {
    fn score(&self, fallback: i32) -> BigInt {
        self.signal
            .as_ref()
            .filter(|value| !value.is_zero())
            .cloned()
            .unwrap_or_else(|| fallback.into())
    }
    fn value(self) -> Value {
        let secure = !self.security.is_empty() && self.security != "--";
        Value::object([
            ("ssid", Value::text(&self.ssid)),
            ("connected", Value::Bool(self.connected)),
            ("security", Value::text(&self.security)),
            ("signal", self.signal.map_or(Value::Null, Value::Integer)),
            ("secure", Value::Bool(secure)),
        ])
    }
}

pub(super) fn networks(text: &str) -> Value {
    let mut seen: Vec<Entry> = Vec::new();
    for line in lines(text) {
        let fields = parts(line);
        let [active, ssid, security, signal, ..] = fields.as_slice() else {
            continue;
        };
        let ssid = strip(ssid);
        if ssid.is_empty() {
            continue;
        }
        let entry = Entry {
            ssid: ssid.into(),
            connected: strip(active).to_lowercase() == "yes",
            security: strip(security).into(),
            signal: Value::text(signal).int().ok(),
        };
        if let Some(previous) = seen.iter_mut().find(|previous| previous.ssid == entry.ssid) {
            if (entry.connected && !previous.connected) || entry.score(0) > previous.score(0) {
                *previous = entry;
            }
        } else {
            seen.push(entry);
        }
    }
    seen.sort_by(|a, b| {
        b.connected
            .cmp(&a.connected)
            .then_with(|| b.score(-1).cmp(&a.score(-1)))
            .then_with(|| a.ssid.cmp(&b.ssid))
    });
    Value::Array(seen.into_iter().map(Entry::value).collect())
}

pub(super) fn address(text: &str) -> String {
    for line in lines(text) {
        if let Some((_, value)) = line.split_once(':') {
            let value = strip(value);
            if !value.is_empty() {
                return value.split('/').next().unwrap_or("").into();
            }
        }
    }
    String::new()
}
