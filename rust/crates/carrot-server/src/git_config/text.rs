pub(super) fn whitespace(character: char) -> bool {
    character.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&character)
}

pub(super) fn decode(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .trim_matches(whitespace)
        .to_owned()
}

pub(super) fn lines(text: &str) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    let split = |c| {
        matches!(
            c,
            '\n' | '\x0b' | '\x0c' | '\x1c' | '\x1d' | '\x1e' | '\u{85}' | '\u{2028}' | '\u{2029}'
        )
    };
    let mut lines: Vec<String> = text.split(split).map(str::to_owned).collect();
    if text.chars().last().is_some_and(split) {
        lines.pop();
    }
    lines
}
