//! Apport report text and stack-frame extraction from the original daemon.
use unicode_general_category::{get_general_category, GeneralCategory};

#[derive(Debug, PartialEq, Eq)]
pub struct Description {
    pub path: String,
    pub message: String,
    pub contents: String,
}
pub fn python_strip(value: &str) -> &str {
    value
        .trim_matches(|point: char| point.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&point))
}
pub fn safe_fn(value: &str) -> String {
    value
        .chars()
        .filter(|&point| {
            point == '_'
                || matches!(
                    get_general_category(point),
                    GeneralCategory::UppercaseLetter
                        | GeneralCategory::LowercaseLetter
                        | GeneralCategory::TitlecaseLetter
                        | GeneralCategory::ModifierLetter
                        | GeneralCategory::OtherLetter
                        | GeneralCategory::DecimalNumber
                        | GeneralCategory::LetterNumber
                        | GeneralCategory::OtherNumber
                )
        })
        .collect()
}
fn signal_name(text: &str) -> Option<&'static str> {
    let mut value = 0_u32;
    let text = python_strip(text)
        .strip_prefix('+')
        .unwrap_or(python_strip(text));
    let mut previous_digit = false;
    for point in text.chars() {
        if point == '_' && previous_digit {
            previous_digit = false;
            continue;
        }
        if get_general_category(point) != GeneralCategory::DecimalNumber {
            return None;
        }
        let mut start = u32::from(point);
        while let Some(previous) = start.checked_sub(1).and_then(char::from_u32) {
            if get_general_category(previous) != GeneralCategory::DecimalNumber {
                break;
            }
            start -= 1;
        }
        value = value
            .checked_mul(10)?
            .checked_add((u32::from(point) - start) % 10)?;
        previous_digit = true;
    }
    if !previous_digit {
        return None;
    }
    match value {
        1 => Some("SIGHUP"),
        2 => Some("SIGINT"),
        3 => Some("SIGQUIT"),
        4 => Some("SIGILL"),
        5 => Some("SIGTRAP"),
        6 => Some("SIGABRT"),
        7 => Some("SIGBUS"),
        8 => Some("SIGFPE"),
        9 => Some("SIGKILL"),
        10 => Some("SIGUSR1"),
        11 => Some("SIGSEGV"),
        12 => Some("SIGUSR2"),
        13 => Some("SIGPIPE"),
        14 => Some("SIGALRM"),
        15 => Some("SIGTERM"),
        16 => Some("SIGSTKFLT"),
        17 => Some("SIGCHLD"),
        18 => Some("SIGCONT"),
        19 => Some("SIGSTOP"),
        20 => Some("SIGTSTP"),
        21 => Some("SIGTTIN"),
        22 => Some("SIGTTOU"),
        23 => Some("SIGURG"),
        24 => Some("SIGXCPU"),
        25 => Some("SIGXFSZ"),
        26 => Some("SIGVTALRM"),
        27 => Some("SIGPROF"),
        28 => Some("SIGWINCH"),
        29 => Some("SIGIO"),
        30 => Some("SIGPWR"),
        31 => Some("SIGSYS"),
        34 => Some("SIGRTMIN"),
        64 => Some("SIGRTMAX"),
        _ => None,
    }
}
fn without_arguments(mut text: &str) -> String {
    let mut output = String::new();
    while let Some(open) = text.find('(') {
        let Some(close) = text[open..].find(')') else {
            break;
        };
        output.push_str(&text[..open]);
        text = &text[open + close + 1..];
    }
    output.push_str(text);
    output
}
#[derive(Default)]
pub(crate) struct Metadata {
    path: String,
    message: String,
    contents: String,
    proc_maps: bool,
}
impl Metadata {
    /// False stops reading before the core dump, just like the source's break.
    pub(crate) fn line(&mut self, line: &str) -> bool {
        if line.contains("CoreDump") {
            return false;
        }
        if line.contains("ProcMaps") {
            self.proc_maps = true;
        } else if line.contains("ProcStatus") {
            self.proc_maps = false;
        }
        if !self.proc_maps {
            self.contents.push_str(line);
        }
        if line.contains("ExecutablePath") {
            self.path = python_strip(line)
                .rsplit(": ")
                .next()
                .unwrap_or("")
                .replace("/data/openpilot/", "");
            self.message.push_str(&self.path);
        } else if line.contains("Signal") {
            self.message.push_str(" - ");
            self.message.push_str(python_strip(line));
            if let Some(name) = signal_name(python_strip(line).rsplit(": ").next().unwrap_or("")) {
                self.message.push_str(" (");
                self.message.push_str(name);
                self.message.push(')');
            }
        }
        true
    }
    pub(crate) fn finish(self, stacktrace: &str) -> Description {
        let lines: Vec<_> = stacktrace.split('\n').collect();
        let crash_function = if lines.len() > 2 {
            let line = lines
                .iter()
                .copied()
                .find(|line| line.contains("at selfdrive/"))
                .unwrap_or(lines[1]);
            without_arguments(
                &line
                    .split(' ')
                    .skip(1)
                    .filter(|part| !part.starts_with("0x"))
                    .collect::<Vec<_>>()
                    .join(" "),
            )
        } else {
            "No stacktrace".into()
        };
        Description {
            path: self.path,
            message: format!("{} - {crash_function}", self.message),
            contents: format!("{stacktrace}\n\n{}", self.contents),
        }
    }
}
pub fn description(text: &str, stacktrace: &str) -> Description {
    let mut metadata = Metadata::default();
    for line in text.split_inclusive('\n') {
        if !metadata.line(line) {
            break;
        }
    }
    metadata.finish(stacktrace)
}
