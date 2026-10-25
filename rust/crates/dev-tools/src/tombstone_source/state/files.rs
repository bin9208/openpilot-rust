//! Original apport filesystem, shell and report policy, independent of tombstoned.
use super::{os_path, path, reader::Lines, scalar, Worker};
use crate::{
    qa_process::{CaptureError, CommandSpec, NativeRunner, Runner},
    tombstone_source::{pure, value as v, Error, Result},
};
use openpilot_log_format::{JsonValue as J, JsonView};
use std::{
    ffi::OsStr,
    fs,
    os::unix::{ffi::OsStrExt, fs::MetadataExt},
    path::Path,
    time::Duration,
};

fn os_text(value: &OsStr) -> J {
    let mut bytes = value.as_bytes();
    let mut points = Vec::new();
    while !bytes.is_empty() {
        match std::str::from_utf8(bytes) {
            Ok(text) => {
                points.extend(text.chars().map(u32::from));
                break;
            }
            Err(error) => {
                let valid = error.valid_up_to();
                points.extend(
                    std::str::from_utf8(&bytes[..valid])
                        .expect("UTF8 prefix")
                        .chars()
                        .map(u32::from),
                );
                let count = error.error_len().unwrap_or(bytes.len() - valid);
                points.extend(
                    bytes[valid..valid + count]
                        .iter()
                        .map(|byte| 0xdc00 + u32::from(*byte)),
                );
                bytes = &bytes[valid + count..];
            }
        }
    }
    v::text(points)
}
pub(super) fn concat(parts: &[J]) -> J {
    v::text(parts.iter().flat_map(v::string).collect())
}
fn reject_nul(path: &Path) -> Result<()> {
    if path.as_os_str().as_bytes().contains(&0) {
        Err(Error::new("ValueError", "embedded null byte"))
    } else {
        Ok(())
    }
}
fn trim(text: &str) -> &str {
    text.trim_matches(|ch: char| ch.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&ch))
}
fn signal_name(text: &str) -> Option<&'static str> {
    let text = trim(text);
    let mut digits = String::new();
    let mut previous_digit = false;
    let chars = text.chars().collect::<Vec<_>>();
    for (index, ch) in chars.iter().copied().enumerate() {
        if index == 0 && matches!(ch, '+' | '-') {
            digits.push(ch);
            continue;
        }
        if ch == '_' {
            if !previous_digit
                || chars
                    .get(index + 1)
                    .is_none_or(|ch| crate::argparse_scan::decimal_value(*ch).is_none())
            {
                return None;
            }
            previous_digit = false;
        } else if let Some(value) = crate::argparse_scan::decimal_value(ch) {
            digits.push(char::from(b'0' + value));
            previous_digit = true;
        } else {
            return None;
        }
    }
    let number = digits.parse::<i64>().ok()?;
    // signal.Signals exposes canonical named enum members, including only the
    // two named RT endpoints; the intervening unnamed RT numbers raise ValueError.
    let names = [
        "SIGHUP",
        "SIGINT",
        "SIGQUIT",
        "SIGILL",
        "SIGTRAP",
        "SIGABRT",
        "SIGBUS",
        "SIGFPE",
        "SIGKILL",
        "SIGUSR1",
        "SIGSEGV",
        "SIGUSR2",
        "SIGPIPE",
        "SIGALRM",
        "SIGTERM",
        "SIGSTKFLT",
        "SIGCHLD",
        "SIGCONT",
        "SIGSTOP",
        "SIGTSTP",
        "SIGTTIN",
        "SIGTTOU",
        "SIGURG",
        "SIGXCPU",
        "SIGXFSZ",
        "SIGVTALRM",
        "SIGPROF",
        "SIGWINCH",
        "SIGIO",
        "SIGPWR",
        "SIGSYS",
    ];
    match number {
        1..=31 => Some(names[number as usize - 1]),
        34 => Some("SIGRTMIN"),
        64 => Some("SIGRTMAX"),
        _ => None,
    }
}
fn crash_function(trace: &str) -> String {
    let lines = trace.split('\n').collect::<Vec<_>>();
    if lines.len() <= 2 {
        return "No stacktrace".into();
    }
    let line = lines
        .iter()
        .copied()
        .find(|line| line.contains("at selfdrive/"))
        .unwrap_or(lines[1]);
    let input = line
        .split(' ')
        .skip(1)
        .filter(|part| !part.starts_with("0x"))
        .collect::<Vec<_>>()
        .join(" ");
    // re.sub(r'\(.*?\)', '', line): non-greedy, non-nesting, dot excludes LF.
    let mut output = String::new();
    let mut rest = input.as_str();
    while let Some(start) = rest.find('(') {
        let Some(end) = rest[start..].find(')') else {
            break;
        };
        output.push_str(&rest[..start]);
        rest = &rest[start + end + 1..];
    }
    output.push_str(rest);
    output
}
fn date(stamp: &J) -> Result<String> {
    let text = stamp.to_utf8().ok_or_else(|| {
        Error::new(
            if matches!(stamp.view(), JsonView::Text(_)) {
                "ValueError"
            } else {
                "TypeError"
            },
            "invalid strptime stamp",
        )
    })?;
    let parse = || -> Option<String> {
        let (day, time) = text.split_once("--")?;
        let date = day.split('-').collect::<Vec<_>>();
        let clock = time.split('-').collect::<Vec<_>>();
        if date.len() != 3 || clock.len() != 3 || date[0].chars().count() != 4 {
            return None;
        }
        let digits = |source: &str| -> Option<u32> {
            if source.is_empty() || !source.chars().all(|ch| ch.is_ascii_digit()) {
                return None;
            }
            source.parse().ok()
        };
        // strptime's %Y regex uses four Unicode decimal codepoints. The
        // month/day range regexes remain ASCII; do not generalize them too.
        let year = date[0].chars().try_fold(0_u32, |year, ch| {
            Some(year * 10 + u32::from(crate::argparse_scan::decimal_value(ch)?))
        })?;
        let (month, day) = (digits(date[1])?, digits(date[2])?);
        let (hour, minute, second) = (digits(clock[0])?, digits(clock[1])?, digits(clock[2])?);
        if !(1..=9999).contains(&year)
            || !(1..=12).contains(&month)
            || hour > 23
            || minute > 59
            || second > 59
        {
            return None;
        }
        let days = [
            31,
            if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) {
                29
            } else {
                28
            },
            31,
            30,
            31,
            30,
            31,
            31,
            30,
            31,
            30,
            31,
        ];
        if day == 0 || day > days[month as usize - 1] {
            return None;
        }
        Some(format!(
            "{year}-{month:02}-{day:02}--{hour:02}-{minute:02}-{second:02}"
        ))
    };
    parse().ok_or_else(|| Error::new("ValueError", "invalid strptime stamp"))
}

#[cfg(test)]
mod date_tests {
    use super::{date, J};
    use sha2::{Digest, Sha256};

    #[test]
    fn original_report_dates_preserve_unicode_years_low_year_format_and_ascii_month_day() {
        let bytes =
            include_bytes!("../../../tests/fixtures/tombstone-tools/date-format-original.json");
        assert_eq!(
            format!("{:x}", Sha256::digest(bytes)),
            "47c8b43c723b00ba2193ff271566ed30ced62bdab7b5e20dcabcf7beae9e581d"
        );
        let original: serde_json::Value = serde_json::from_slice(bytes).unwrap();
        assert_eq!(original["candidate_executed"], false);
        assert_eq!(original["cases"].as_array().unwrap().len(), 6);
        for case in original["cases"].as_array().unwrap() {
            let stamp = case["stamp"].as_str().unwrap();
            match date(&J::text(stamp)) {
                Ok(actual) => assert_eq!(actual, case["formatted"].as_str().unwrap(), "{stamp}"),
                Err(error) => assert_eq!(error.kind(), case["error"].as_str().unwrap(), "{stamp}"),
            }
        }
    }
}
impl Worker {
    pub(super) fn actual_path(&self, value: &J) -> Result<std::path::PathBuf> {
        Ok(self.cwd.join(os_path(value)?))
    }
    pub(super) fn scan(&self) -> Result<Vec<(J, i64)>> {
        let directory = match self.actual_path(&self.apport) {
            Ok(path) => path,
            Err(error) if error.kind() == "UnicodeEncodeError" => return Ok(Vec::new()),
            Err(error) => return Err(error),
        };
        if !directory.exists() {
            return Ok(Vec::new());
        }
        let mut files = Vec::new();
        for entry in fs::read_dir(&directory).map_err(Error::io)?.take(1000) {
            self.check_control()?;
            let entry = entry.map_err(Error::io)?;
            let name = entry.file_name();
            let name = name.as_bytes();
            if name.starts_with(b"tombstone") || name.ends_with(b".crash") {
                let metadata = fs::metadata(entry.path()).map_err(Error::io)?;
                if name.starts_with(b"tombstone") || metadata.mode() == 0o100640 {
                    // DirEntry.path retains the original source spelling,
                    // rather than turning a relative APPORT_DIR into cwd.
                    let name = os_text(entry.file_name().as_os_str());
                    files.push((concat(&[self.apport.clone(), name]), metadata.ctime()));
                }
            }
        }
        Ok(files)
    }
    pub(super) fn clear_apport(&self) {
        let Ok(directory) = self.actual_path(&self.apport) else {
            return;
        };
        let Ok(entries) = fs::read_dir(directory) else {
            return;
        };
        for entry in entries.flatten() {
            if !entry.file_name().as_bytes().starts_with(b".") {
                let _ = fs::remove_file(entry.path());
            }
        }
    }
    fn retrace(&self, filename: &J, request: &J) -> Result<String> {
        let shell = request.get("shell").unwrap_or_else(|| J::text("/bin/bash"));
        let shell = os_path(&shell)?;
        let timeout = request.get("timeout_ms").unwrap_or_else(|| scalar("30000"));
        let seconds = match timeout.view() {
            JsonView::Integer(text) => {
                // Python divides the arbitrary-precision integer before its
                // float conversion. Parsing it first can overflow prematurely.
                let seconds = format!("{text}e-3")
                    .parse::<f64>()
                    .map_err(|error| Error::new("OverflowError", error.to_string()))?;
                if seconds.is_infinite() {
                    return Err(Error::new("OverflowError", "integer division overflow"));
                }
                seconds
            }
            JsonView::Float(value) => value / 1000.0,
            JsonView::Bool(value) => f64::from(u8::from(value)) / 1000.0,
            _ => {
                return Err(Error::new(
                    "TypeError",
                    "timeout division requires a number",
                ))
            }
        };
        // subprocess.Popen runs before PollSelector converts the remaining
        // timeout to signed integer milliseconds. Preserve spawn-error priority
        // over that conversion error, while retaining owned bounded cleanup.
        let poll_error = if seconds.is_nan() {
            Some("ValueError")
        } else if seconds > f64::from(i32::MAX) / 1000.0 {
            Some("OverflowError")
        } else {
            None
        };
        let timeout = if poll_error.is_some() {
            Duration::from_nanos(1)
        } else {
            Duration::try_from_secs_f64(seconds.max(1e-9))
                .map_err(|error| Error::new("OverflowError", error.to_string()))?
        };
        let command = concat(&[
            J::text("apport-retrace -s <(cat <(echo \"Package: openpilot\") \""),
            filename.clone(),
            J::text("\")"),
        ]);
        let mut spec = CommandSpec::new_os(
            vec![
                shell.into_os_string(),
                "-c".into(),
                os_path(&command)?.into_os_string(),
            ],
            timeout,
        );
        spec.cwd = Some(self.cwd.clone());
        let environment = self.environment.clone();
        spec.env_remove = std::env::vars_os()
            .map(|(key, _)| key)
            .filter(|key| !environment.contains_key(key))
            .collect();
        spec.set_os_env(environment);
        spec.inherit_stdin = true;
        spec.inherit_stderr = true;
        spec.stdout_limit = usize::MAX;
        let control = self
            .control
            .as_ref()
            .map(|(deadline, cancelled)| (*deadline, cancelled.as_ref()));
        let captured = match (&self.stderr, control) {
            (Some(stderr), control) => NativeRunner.run_with_stderr(&spec, control, stderr),
            (None, Some((deadline, cancelled))) => {
                NativeRunner.run_controlled(&spec, deadline, cancelled)
            }
            (None, None) => NativeRunner.run(&spec),
        };
        // Preserve the shell's own30second timeout as a source return string,
        // but never swallow the enclosing controller's deadline/cancellation.
        self.check_control()?;
        if let Some(kind) = poll_error {
            if captured
                .as_ref()
                .err()
                .and_then(|error| error.downcast_ref::<std::io::Error>())
                .is_none()
            {
                return Err(Error::new(kind, "invalid subprocess poll timeout"));
            }
        }
        let captured = match captured {
            Ok(captured) => captured,
            Err(error)
                if matches!(
                    error.downcast_ref::<CaptureError>(),
                    Some(CaptureError::Timeout { .. })
                ) =>
            {
                return Ok("Timeout getting stacktrace".into())
            }
            Err(error) => {
                if let Some(io) = error.downcast_ref::<std::io::Error>() {
                    let error = io.raw_os_error().map_or_else(
                        || std::io::Error::new(io.kind(), io.to_string()),
                        std::io::Error::from_raw_os_error,
                    );
                    return Err(Error::io(error));
                }
                return Err(Error::new("OSError", error.to_string()));
            }
        };
        // communicate(text=True) translates stdout before check_returncode.
        let stdout = String::from_utf8(captured.stdout)
            .map_err(|error| Error::new("UnicodeDecodeError", error.to_string()))?;
        let stdout = stdout.replace("\r\n", "\n").replace('\r', "\n");
        if captured.code != 0 {
            Ok("Error getting stacktrace".into())
        } else {
            Ok(stdout)
        }
    }
    pub(super) fn report(&mut self, filename: &J, request: &J) -> Result<()> {
        let input = self.actual_path(filename)?;
        reject_nul(&input)?;
        let size = fs::metadata(&input).map_err(Error::io)?.len();
        if size > 100_000_000 {
            self.log(
                40,
                concat(&[
                    J::text("Tombstone "),
                    filename.clone(),
                    J::text(&format!(" too big, {size}. Skipping...")),
                ]),
                false,
            );
            return Ok(());
        }
        let mut lines = Lines::new(fs::File::open(&input).map_err(Error::io)?);
        let (mut message, mut contents, mut executable) =
            (String::new(), String::new(), String::new());
        let mut proc_maps = false;
        while let Some(line) = lines.next(|| self.check_control())? {
            if line.contains("CoreDump") {
                break;
            }
            if line.contains("ProcMaps") {
                proc_maps = true;
            } else if line.contains("ProcStatus") {
                proc_maps = false;
            }
            if !proc_maps {
                contents.push_str(&line);
            }
            if line.contains("ExecutablePath") {
                executable = trim(&line)
                    .rsplit(": ")
                    .next()
                    .unwrap_or_default()
                    .replace("/data/openpilot/", "");
                message.push_str(&executable);
            } else if line.contains("Signal") {
                message.push_str(" - ");
                message.push_str(trim(&line));
                if let Some(name) = signal_name(trim(&line).rsplit(": ").next().unwrap_or_default())
                {
                    message.push_str(&format!(" ({name})"));
                }
            }
        }
        let trace = self.retrace(filename, request)?;
        message.push_str(" - ");
        message.push_str(&crash_function(&trace));
        contents = format!("{trace}\n\n{contents}");
        self.log(40, v::object(&[("tombstone", J::text(&message))]), false);
        self.sdk_value("set_extra", J::text("tombstone_fn"), filename.clone())?;
        self.sdk_value("set_extra", J::text("tombstone"), J::text(&contents))?;
        self.sdk(
            "capture_message",
            v::object(&[("message", J::text(&message))]),
        )?;
        self.sdk("flush", scalar("null"))?;
        // Filename/build errors happen after all original SDK effects.
        let stamp = date(&self.stamp)?;
        let metadata = self.version("metadata", v::text(v::string(&self.base)))?;
        let commit = v::field(&v::field(&metadata, "openpilot")?, "git_commit")?;
        let name = pure::filename(&v::object(&[
            ("path", J::text(&executable)),
            ("stamp", J::text(&stamp)),
            ("commit", J::text(&commit.to_json().expect("source commit"))),
        ]))?;
        let root = self.actual_path(&self.root)?.join("crash");
        fs::create_dir_all(&root).map_err(Error::io)?;
        let mut target = root.join(os_path(&name)?);
        if target.is_dir() {
            target.push(
                input
                    .file_name()
                    .ok_or_else(|| Error::new("FileNotFoundError", "missing basename"))?,
            );
        }
        if fs::metadata(&target).is_ok_and(|target| {
            fs::metadata(&input)
                .is_ok_and(|source| source.dev() == target.dev() && source.ino() == target.ino())
        }) {
            return Err(Error::new(
                "SameFileError",
                "source and destination are the same file",
            ));
        }
        fs::copy(&input, &target).map_err(Error::io)?;
        self.check_control()?;
        match fs::remove_file(&input) {
            Ok(()) => {}
            Err(error) if matches!(error.raw_os_error(), Some(1 | 13)) => {}
            Err(error) => return Err(Error::io(error)),
        }
        Ok(())
    }
    pub(super) fn files(&mut self, request: &J, op: &J) -> Result<J> {
        if op.text_eq("scan") || op.text_eq("clear") {
            let mut points = v::points(&v::field(request, "path")?, "TypeError")?;
            points.push(47);
            self.apport = v::text(points);
            if op.text_eq("clear") {
                self.clear_apport();
                return Ok(scalar("null"));
            }
            let mut files = self.scan()?;
            files.sort_by_key(|(name, time)| (v::string(name), *time));
            return Ok(v::array(
                &files
                    .into_iter()
                    .map(|(name, time)| v::array(&[name, scalar(&time.to_string())]))
                    .collect::<Vec<_>>(),
            ));
        }
        if op.text_eq("retrace") {
            return self
                .retrace(&v::field(request, "path")?, request)
                .map(|value| J::text(&value));
        }
        let root = path(&v::field(request, "root")?)?;
        let stamp = v::field(request, "stamp")?;
        self.root = root;
        self.stamp = stamp;
        self.report(&v::field(request, "path")?, request)?;
        Ok(scalar("null"))
    }
}
