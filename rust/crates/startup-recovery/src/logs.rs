use std::{
    collections::VecDeque,
    fs::File,
    io::{self, BufRead, Read, Seek, SeekFrom, Write},
    path::Path,
    sync::OnceLock,
};
use unicode_general_category::{get_general_category, GeneralCategory};
const LIMIT: usize = 65536;
fn whitespace(c: char) -> bool {
    c.is_whitespace() || matches!(c, '\u{1c}'..='\u{1f}')
}
fn word(c: char) -> bool {
    c == '_'
        || matches!(
            get_general_category(c),
            GeneralCategory::UppercaseLetter
                | GeneralCategory::LowercaseLetter
                | GeneralCategory::TitlecaseLetter
                | GeneralCategory::ModifierLetter
                | GeneralCategory::OtherLetter
                | GeneralCategory::DecimalNumber
                | GeneralCategory::LetterNumber
                | GeneralCategory::OtherNumber
        )
}
fn diagnostic(line: &str) -> bool {
    let mut token = String::new();
    for c in line.chars() {
        if word(c) {
            token.push(if matches!(c, 'İ' | 'ı') {
                'i'
            } else {
                c.to_ascii_lowercase()
            });
        } else {
            if c == ':' && (token.ends_with("error") || token.ends_with("exception")) {
                return true;
            }
            token.clear();
        }
    }
    false
}
fn normalize(bytes: &[u8]) -> String {
    static ANSI: OnceLock<regex::Regex> = OnceLock::new();
    let ansi = ANSI
        .get_or_init(|| regex::Regex::new(r"\x1b\[[0-?]*[ -/]*[@-~]").expect("ANSI expression"));
    let raw = String::from_utf8_lossy(bytes);
    let text = ansi.replace_all(&raw, "");
    let mut lines: Vec<_> = text
        .split([
            '\n', '\r', '\u{b}', '\u{c}', '\u{1c}', '\u{1d}', '\u{1e}', '\u{85}', '\u{2028}',
            '\u{2029}',
        ])
        .map(|line| line.trim_end_matches(whitespace))
        .filter(|line| !line.trim_matches(whitespace).is_empty())
        .collect();
    if let Some(i) = lines.iter().position(|line| diagnostic(line)) {
        lines.rotate_left(i);
    }
    lines.join("\n")
}
pub fn read_error(path: Option<&Path>) -> String {
    let read = || -> io::Result<String> {
        let path = path.ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))?;
        let mut log = File::open(path)?;
        let size = std::fs::metadata(path)?.len();
        log.seek(SeekFrom::Start(size.saturating_sub(LIMIT as u64)))?;
        let mut bytes = Vec::new();
        log.take(LIMIT as u64).read_to_end(&mut bytes)?;
        Ok(normalize(&bytes))
    };
    read().unwrap_or_default()
}
/// Forward every byte, but retain complete readline chunks totalling at most
/// 64 KiB. The final write occurs even for an empty input, as in the source.
pub fn capture(
    source: &mut impl BufRead,
    destination: &mut impl Write,
    mut store: impl FnMut(&[u8]) -> io::Result<()>,
    mut now: impl FnMut() -> f64,
) -> io::Result<()> {
    let mut chunks: VecDeque<Vec<u8>> = VecDeque::new();
    let mut size = 0;
    let mut last_write = f64::NEG_INFINITY;
    loop {
        let mut data = Vec::new();
        (&mut *source).take(4096).read_until(b'\n', &mut data)?;
        if data.is_empty() {
            break;
        }
        destination.write_all(&data)?;
        destination.flush()?;
        size += data.len();
        chunks.push_back(data);
        while size > LIMIT {
            size -= chunks.pop_front().expect("capture chunk").len();
        }
        if now() - last_write >= 1.0 {
            store(&chunks.iter().flatten().copied().collect::<Vec<_>>())?;
            last_write = now();
        }
    }
    store(&chunks.iter().flatten().copied().collect::<Vec<_>>())
}
#[cfg(test)]
mod tests;
