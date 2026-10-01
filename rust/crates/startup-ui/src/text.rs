use crate::geometry::Point;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Font {
    Normal,
    NormalRaw,
    Medium,
    Pretendard,
    Display,
    Bold,
    SemiBold,
    Unifont,
    Regular,
}
pub trait Measure {
    fn measure(&self, font: Font, text: &str, size: f32, spacing: f32) -> Point;
}
pub fn whitespace(character: char) -> bool {
    character.is_whitespace() || matches!(character, '\x1c'..='\x1f')
}
pub fn trim(text: &str) -> &str {
    text.trim_matches(whitespace)
}

/// Source text.py wrapping: retain indentation, whitespace delimiters and hyphens.
pub fn wrap(text: &str, size: f32, width: f32, measure: &impl Measure) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        if trim(paragraph).is_empty() {
            if !lines.is_empty() {
                lines.push(String::new());
            }
            continue;
        }
        let indent_len = paragraph
            .char_indices()
            .find(|(_, c)| !whitespace(*c))
            .map_or(paragraph.len(), |(i, _)| i);
        let mut current = paragraph[..indent_len].to_owned();
        let words = split_words(&paragraph[indent_len..]);
        let mut words = words.iter();
        while let Some(word) = words.next() {
            let candidate = format!("{current}{word}{}", words.next().map_or("", String::as_str));
            if measure.measure(Font::Normal, &candidate, size, 0.0).x <= width {
                current = candidate;
            } else {
                lines.push(current);
                current = format!("{word} ");
            }
        }
        let current = current.trim_end_matches(whitespace);
        if !current.is_empty() {
            lines.push(current.into());
        }
    }
    lines
}
fn split_words(text: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((index, character)) = chars.next() {
        if character == '-' || whitespace(character) {
            parts.push(text[start..index].into());
            let mut end = index + character.len_utf8();
            if character != '-' {
                while let Some(&(index, character)) = chars.peek() {
                    if !whitespace(character) {
                        break;
                    }
                    end = index + character.len_utf8();
                    chars.next();
                }
            }
            parts.push(text[index..end].into());
            start = end;
        }
    }
    parts.push(text[start..].into());
    parts
}
pub fn fit_single_line(text: &str, size: f32, width: f32, measure: &impl Measure) -> String {
    let text = text
        .split(whitespace)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    if text.is_empty() || measure.measure(Font::Pretendard, &text, size, 0.0).x <= width {
        return text;
    }
    let chars: Vec<_> = text.chars().collect();
    let (mut low, mut high) = (0, chars.len());
    while low < high {
        let mid = (low + high).div_ceil(2);
        let prefix: String = chars[..mid].iter().collect();
        let candidate = format!("{}...", prefix.trim_end_matches(whitespace));
        if measure.measure(Font::Pretendard, &candidate, size, 0.0).x <= width {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    let prefix: String = chars[..low].iter().collect();
    format!("{}...", prefix.trim_end_matches(whitespace))
}

/// Source emoji pattern treats adjacent matching code points as one measured cluster.
pub fn strip_emoji(text: &str) -> (String, u32) {
    let mut plain = String::new();
    let mut count = 0;
    let mut prior = false;
    for c in text.chars() {
        let emoji = matches!(c, '\u{1f600}'..='\u{1f64f}' | '\u{1f300}'..='\u{1f5ff}' | '\u{1f680}'..='\u{1f6ff}' | '\u{1f1e0}'..='\u{1f1ff}' | '\u{1f900}'..='\u{1f9ff}' | '\u{2300}'..='\u{23ff}' | '\u{2600}'..='\u{2bff}' | '\u{1fa70}'..='\u{1faff}' | '\u{1f700}'..='\u{1f77f}' | '\u{200d}' | '\u{fe0f}' | '\u{3030}');
        if emoji {
            if !prior {
                count += 1;
            }
        } else {
            plain.push(c);
        }
        prior = emoji;
    }
    (plain, count)
}
