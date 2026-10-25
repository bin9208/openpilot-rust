//! Read-only rich strace inspection. No process, path, FD or DSO admission.
use super::{require, Result};
use crate::argparse_scan::{scan_os_required_first, OptionSpec};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    ffi::OsString,
    fs::{self, File},
    io::{BufRead, BufReader, Read, Write},
    os::{
        fd::AsRawFd,
        unix::fs::{FileExt, MetadataExt},
    },
    path::{Component, Path, PathBuf},
};

const MAX_FILE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_LINE_BYTES: usize = 512 * 1024;
const MAX_MODEL_BYTES: usize = 64 * 1024 * 1024;
const MAX_FILES: usize = 8192;
const MAX_LINES: usize = 2_000_000;
const MAX_EVENTS: usize = 1_000_000;
const MAX_STRING: usize = 65_536;
const MAX_DEPTH: usize = 128;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct Identity {
    dev: u64,
    ino: u64,
    mode: u32,
    uid: u32,
    gid: u32,
    links: u64,
    size: u64,
    mtime: i64,
    mtime_nsec: i64,
    ctime: i64,
    ctime_nsec: i64,
}
fn identity(m: &fs::Metadata) -> Identity {
    Identity {
        dev: m.dev(),
        ino: m.ino(),
        mode: m.mode(),
        uid: m.uid(),
        gid: m.gid(),
        links: m.nlink(),
        size: m.len(),
        mtime: m.mtime(),
        mtime_nsec: m.mtime_nsec(),
        ctime: m.ctime(),
        ctime_nsec: m.ctime_nsec(),
    }
}
type Anchor = (u64, u64, u32, u32, u32);
fn absolute(path: &Path) -> Result<()> {
    require(
        path.is_absolute()
            && path
                .components()
                .all(|c| matches!(c, Component::RootDir | Component::Normal(_))),
        "trace path must be normal absolute",
    )
}
fn directory(path: &Path) -> Result<(File, Vec<Anchor>)> {
    absolute(path)?;
    let mut held = File::open("/")?;
    let mut anchors = Vec::new();
    let mut capture = |file: &File| -> Result<()> {
        let m = file.metadata()?;
        anchors.push((m.dev(), m.ino(), m.mode(), m.uid(), m.gid()));
        Ok(())
    };
    capture(&held)?;
    for component in path.components() {
        if let Component::Normal(name) = component {
            held = File::from(rustix::fs::openat(
                &held,
                Path::new(name),
                rustix::fs::OFlags::RDONLY
                    | rustix::fs::OFlags::DIRECTORY
                    | rustix::fs::OFlags::NOFOLLOW
                    | rustix::fs::OFlags::NONBLOCK
                    | rustix::fs::OFlags::CLOEXEC,
                rustix::fs::Mode::empty(),
            )?);
            capture(&held)?;
        }
    }
    let m = held.metadata()?;
    require(
        m.is_dir() && m.uid() == fs::metadata("/proc/self")?.uid() && m.mode() & 0o077 == 0,
        "trace directory must be owned private",
    )?;
    Ok((held, anchors))
}
fn open_input(parent: &File, name: &str) -> Result<File> {
    Ok(File::from(rustix::fs::openat(
        parent,
        name,
        rustix::fs::OFlags::RDONLY
            | rustix::fs::OFlags::NOFOLLOW
            | rustix::fs::OFlags::NONBLOCK
            | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )?))
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct Roster {
    name: String,
    identity: Identity,
}
fn roster(parent: &File) -> Result<Vec<Roster>> {
    roster_with_file_bound(parent, MAX_FILE_BYTES)
}
fn roster_with_file_bound(parent: &File, file_bound: u64) -> Result<Vec<Roster>> {
    require(file_bound <= MAX_FILE_BYTES, "trace file bound invalid")?;
    let mut result = Vec::new();
    let uid = fs::metadata("/proc/self")?.uid();
    for entry in fs::read_dir(format!("/proc/self/fd/{}", parent.as_raw_fd()))? {
        let entry = entry?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| "trace roster name invalid")?;
        require(
            !name.is_empty()
                && name.len() <= 255
                && name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b)),
            "trace roster name invalid",
        )?;
        let named = fs::symlink_metadata(entry.path())?;
        require(
            named.is_file()
                && !named.file_type().is_symlink()
                && named.nlink() == 1
                && named.uid() == uid
                && named.mode() & 0o022 == 0
                && named.len() <= file_bound,
            "trace roster entry unsafe",
        )?;
        let held = open_input(parent, &name)?;
        require(
            identity(&held.metadata()?) == identity(&named),
            "trace roster changed",
        )?;
        result.push(Roster {
            name,
            identity: identity(&named),
        });
        require(result.len() <= MAX_FILES, "trace file count exceeds bound")?;
    }
    result.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(result)
}
struct Held {
    pid: u32,
    name: String,
    file: File,
    before: Identity,
    sha256: String,
}
struct Inputs {
    path: PathBuf,
    directory: File,
    anchors: Vec<Anchor>,
    before: Identity,
    roster: Vec<Roster>,
    files: Vec<Held>,
    total: u64,
}
fn hash(file: &File, size: u64) -> Result<String> {
    let mut offset = 0;
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 65_536];
    loop {
        let n = file.read_at(&mut buffer, offset)?;
        if n == 0 {
            break;
        }
        offset += n as u64;
        require(
            offset <= size && offset <= MAX_FILE_BYTES,
            "trace input changed",
        )?;
        digest.update(&buffer[..n]);
    }
    require(offset == size, "trace input changed")?;
    Ok(format!("{:x}", digest.finalize()))
}
impl Inputs {
    fn open(path: &Path, prefix: &str) -> Result<Self> {
        require(
            !prefix.is_empty()
                && prefix.len() <= 128
                && prefix
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b)),
            "trace prefix invalid",
        )?;
        let (directory, anchors) = directory(path)?;
        let before = identity(&directory.metadata()?);
        let roster = roster(&directory)?;
        let mut files = Vec::new();
        let mut pids = BTreeSet::new();
        let mut total = 0u64;
        let stem = format!("{prefix}.");
        for row in &roster {
            let Some(suffix) = row.name.strip_prefix(&stem) else {
                continue;
            };
            require(
                !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit()),
                "trace PID filename invalid",
            )?;
            let pid = suffix
                .parse::<u32>()
                .map_err(|_| "trace PID filename invalid")?;
            require(pid > 0, "trace PID filename invalid")?;
            require(pids.insert(pid), "duplicate trace PID")?;
            total = total
                .checked_add(row.identity.size)
                .ok_or("trace byte count overflow")?;
            require(total <= MAX_TOTAL_BYTES, "trace total exceeds bound")?;
            let file = open_input(&directory, &row.name)?;
            require(
                identity(&file.metadata()?) == row.identity,
                "trace input changed",
            )?;
            files.push(Held {
                pid,
                name: row.name.clone(),
                file,
                before: row.identity.clone(),
                sha256: String::new(),
            });
        }
        require(!files.is_empty(), "no selected trace PID files")?;
        Ok(Self {
            path: path.into(),
            directory,
            anchors,
            before,
            roster,
            files,
            total,
        })
    }
    fn events(&mut self) -> Result<Vec<Event>> {
        let mut events = Vec::new();
        let mut budget = 0usize;
        let mut lines = 0usize;
        for file in &mut self.files {
            parse_file(file, &mut events, &mut budget, &mut lines)?;
        }
        Ok(events)
    }
    fn verify(&self) -> Result<()> {
        let (now, anchors) = directory(&self.path)?;
        require(
            anchors == self.anchors
                && identity(&now.metadata()?) == self.before
                && identity(&self.directory.metadata()?) == self.before,
            "trace directory changed",
        )?;
        require(
            roster(&self.directory)? == self.roster,
            "trace roster changed",
        )?;
        for file in &self.files {
            let named = open_input(&self.directory, &file.name)?;
            require(
                identity(&named.metadata()?) == file.before
                    && identity(&file.file.metadata()?) == file.before
                    && hash(&file.file, file.before.size)? == file.sha256,
                "trace input changed",
            )?;
            let after = open_input(&self.directory, &file.name)?;
            require(
                identity(&after.metadata()?) == file.before
                    && identity(&file.file.metadata()?) == file.before,
                "trace input changed",
            )?;
        }
        let (now, anchors) = directory(&self.path)?;
        require(
            anchors == self.anchors
                && identity(&now.metadata()?) == self.before
                && roster(&self.directory)? == self.roster,
            "trace roster changed",
        )?;
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize)]
struct Range {
    start: usize,
    end: usize,
}
fn trimmed(bytes: &[u8], start: usize, end: usize) -> Range {
    let mut a = start;
    let mut b = end;
    while a < b && bytes[a].is_ascii_whitespace() {
        a += 1;
    }
    while b > a && bytes[b - 1].is_ascii_whitespace() {
        b -= 1;
    }
    Range { start: a, end: b }
}
#[derive(Clone, Debug, Serialize)]
struct Fragment {
    line: usize,
    line_offset: u64,
    line_bytes: usize,
    column: usize,
    length: usize,
    logical_offset: usize,
    timestamp: String,
}
#[derive(Debug, Serialize)]
struct BytesAt {
    bytes: Vec<u8>,
    source: Range,
}
#[derive(Debug, Serialize)]
struct Environment {
    representation: String,
    entries: Option<Vec<BytesAt>>,
    source: Range,
    complete: bool,
}
#[derive(Debug, Serialize)]
struct Exec {
    program: BytesAt,
    argv: Option<Vec<BytesAt>>,
    environment: Environment,
    dirfd: Option<Range>,
    flags: Option<Range>,
}
#[derive(Debug, Serialize)]
struct Event {
    pid: u32,
    kind: String,
    operation: Option<String>,
    source: Vec<Fragment>,
    arguments: Option<Range>,
    result: Option<Vec<u8>>,
    result_source: Option<Range>,
    outcome: Option<String>,
    exec: Option<Exec>,
    opaque_payload: bool,
    numeric_read_arguments: Option<Vec<BytesAt>>,
    numeric_read_result: Option<BytesAt>,
    aborted_by: Option<String>,
}
fn event(pid: u32, kind: &str, source: Vec<Fragment>) -> Event {
    Event {
        pid,
        kind: kind.into(),
        operation: None,
        source,
        arguments: None,
        result: None,
        result_source: None,
        outcome: None,
        exec: None,
        opaque_payload: false,
        numeric_read_arguments: None,
        numeric_read_result: None,
        aborted_by: None,
    }
}
fn push(events: &mut Vec<Event>, budget: &mut usize, row: Event) -> Result<()> {
    *budget = budget
        .checked_add(serde_json::to_vec(&row)?.len())
        .ok_or("trace model exceeds bound")?;
    require(
        *budget <= MAX_MODEL_BYTES && events.len() < MAX_EVENTS,
        "trace model exceeds bound",
    )?;
    events.push(row);
    Ok(())
}
#[derive(Default)]
struct EventSummary {
    events: Vec<Event>,
    budget: usize,
    event_count: usize,
    counts: std::collections::BTreeMap<String, usize>,
}
impl EventSummary {
    fn observe(&mut self, row: Event) -> Result<()> {
        self.event_count = self
            .event_count
            .checked_add(1)
            .ok_or("trace event count exceeds bound")?;
        require(
            self.event_count <= MAX_EVENTS,
            "trace event count exceeds bound",
        )?;
        let key = format!(
            "{}:{}:{}",
            row.kind,
            row.operation.as_deref().unwrap_or("-"),
            row.outcome.as_deref().unwrap_or("-")
        );
        require(key.len() <= 256, "trace summary key exceeds bound")?;
        require(
            self.counts.contains_key(&key) || self.counts.len() < 1024,
            "trace summary key count exceeds bound",
        )?;
        *self.counts.entry(key).or_default() += 1;
        // 모든 줄은 같은 parser를 거친다. syscall 세부 Event의 영속 출력 범위만 명시적으로 줄인다.
        if row.exec.is_some() || row.kind != "syscall" {
            push(&mut self.events, &mut self.budget, row)?;
        }
        Ok(())
    }
}
fn quoted(bytes: &[u8], at: &mut usize) -> Result<BytesAt> {
    let start = *at;
    require(bytes.get(*at) == Some(&b'"'), "trace C-string invalid")?;
    *at += 1;
    let mut decoded = Vec::new();
    loop {
        let byte = *bytes.get(*at).ok_or("trace C-string invalid")?;
        *at += 1;
        if byte == b'"' {
            return Ok(BytesAt {
                bytes: decoded,
                source: Range { start, end: *at },
            });
        }
        let value = if byte == b'\\' {
            let escaped = *bytes.get(*at).ok_or("trace C-string invalid")?;
            *at += 1;
            match escaped {
                b'"' | b'\\' => escaped,
                b'a' => 7,
                b'b' => 8,
                b't' => 9,
                b'n' => 10,
                b'v' => 11,
                b'f' => 12,
                b'r' => 13,
                b'x' => {
                    let pair = bytes.get(*at..*at + 2).ok_or("trace C-string invalid")?;
                    require(
                        pair.iter().all(u8::is_ascii_hexdigit),
                        "trace C-string invalid",
                    )?;
                    *at += 2;
                    u8::from_str_radix(std::str::from_utf8(pair)?, 16)?
                }
                b'0'..=b'7' => {
                    let mut number = u16::from(escaped - b'0');
                    let mut n = 1;
                    while n < 3 && bytes.get(*at).is_some_and(|b| (b'0'..=b'7').contains(b)) {
                        number = number * 8 + u16::from(bytes[*at] - b'0');
                        *at += 1;
                        n += 1;
                    }
                    require(number <= 255, "trace C-string invalid")?;
                    number as u8
                }
                _ => return Err("trace C-string invalid".into()),
            }
        } else {
            require(byte >= 0x20 && byte != 0x7f, "trace C-string invalid")?;
            byte
        };
        require(decoded.len() < MAX_STRING, "trace C-string exceeds bound")?;
        decoded.push(value);
    }
}
fn annotation(bytes: &[u8], at: &mut usize) -> Result<bool> {
    let start = *at;
    let mut depth = 0usize;
    loop {
        let byte = *bytes
            .get(*at)
            .ok_or("trace descriptor annotation invalid")?;
        require(
            byte >= 0x20 && byte != 0x7f,
            "trace descriptor annotation invalid",
        )?;
        if byte == b'<' {
            depth += 1;
            require(depth <= MAX_DEPTH, "trace nesting exceeds bound")?;
        }
        if byte == b'>' && (*at == start || bytes[*at - 1] != b'-') {
            depth -= 1;
            *at += 1;
            if depth == 0 {
                break;
            }
            continue;
        }
        *at += 1;
    }
    Ok(bytes[start..*at].windows(3).any(|w| w == b"..."))
}
fn shift_word(bytes: &[u8]) -> bool {
    if bytes
        .first()
        .is_some_and(|b| b.is_ascii_alphabetic() || *b == b'_')
    {
        return bytes
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || *b == b'_');
    }
    if let Some(hex) = bytes
        .strip_prefix(b"0x")
        .or_else(|| bytes.strip_prefix(b"0X"))
    {
        return !hex.is_empty() && hex.iter().all(u8::is_ascii_hexdigit);
    }
    !bytes.is_empty() && bytes.iter().all(u8::is_ascii_digit)
}
// A binary token pair, not an evaluated mask or a general C expression parser.
fn left_shift(bytes: &[u8], at: &mut usize, body: usize) -> Result<()> {
    let mut left_end = *at;
    while left_end > body && matches!(bytes[left_end - 1], b' ' | b'\t') {
        left_end -= 1;
    }
    let mut left_start = left_end;
    while left_start > body
        && (bytes[left_start - 1].is_ascii_alphanumeric() || bytes[left_start - 1] == b'_')
    {
        left_start -= 1;
    }
    let mut before = left_start;
    while before > body && matches!(bytes[before - 1], b' ' | b'\t') {
        before -= 1;
    }
    require(
        shift_word(&bytes[left_start..left_end])
            && (before == body || b"([{,=|&".contains(&bytes[before - 1])),
        "trace shift expression invalid",
    )?;
    let mut right_start = *at + 2;
    while bytes
        .get(right_start)
        .is_some_and(|b| matches!(*b, b' ' | b'\t'))
    {
        right_start += 1;
    }
    let mut right_end = right_start;
    while bytes
        .get(right_end)
        .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
    {
        right_end += 1;
    }
    let mut after = right_end;
    while bytes.get(after).is_some_and(|b| matches!(*b, b' ' | b'\t')) {
        after += 1;
    }
    require(
        shift_word(&bytes[right_start..right_end])
            && (after == bytes.len() || b",)]}|&".contains(&bytes[after])),
        "trace shift expression invalid",
    )?;
    *at = right_end;
    Ok(())
}
struct Lexed {
    end: usize,
    args: Vec<Range>,
    opaque: bool,
    complete: bool,
}
// Lexical structure only: no interpretation of flags, sockaddr, FD or memory.
fn balanced(bytes: &[u8], start: usize, partial: bool) -> Result<Lexed> {
    let close = match bytes.get(start) {
        Some(b'(') => b')',
        Some(b'[') => b']',
        Some(b'{') => b'}',
        _ => return Err("trace opening delimiter invalid".into()),
    };
    let mut stack = vec![close];
    let mut at = start + 1;
    let mut arg_start = at;
    let mut args = Vec::new();
    let mut opaque = false;
    while at < bytes.len() {
        match bytes[at] {
            b'"' => {
                quoted(bytes, &mut at)?;
                continue;
            }
            b'<' if bytes.get(at + 1) == Some(&b'<') => {
                left_shift(bytes, &mut at, start + 1)?;
                continue;
            }
            b'<' => {
                opaque |= annotation(bytes, &mut at)?;
                continue;
            }
            b'/' if bytes.get(at + 1) == Some(&b'*') => {
                let end = bytes[at + 2..]
                    .windows(2)
                    .position(|w| w == b"*/")
                    .ok_or("trace comment invalid")?
                    + at
                    + 4;
                require(
                    bytes[at..end].iter().all(|b| *b >= 0x20 && *b != 0x7f),
                    "trace comment invalid",
                )?;
                opaque |= bytes[at..end].windows(3).any(|w| w == b"...");
                at = end;
                continue;
            }
            b'(' | b'[' | b'{' => {
                stack.push(match bytes[at] {
                    b'(' => b')',
                    b'[' => b']',
                    _ => b'}',
                });
                require(stack.len() <= MAX_DEPTH, "trace nesting exceeds bound")?;
            }
            b')' | b']' | b'}' => {
                require(stack.pop() == Some(bytes[at]), "trace delimiters invalid")?;
                if stack.is_empty() {
                    let last = trimmed(bytes, arg_start, at);
                    if last.start < last.end {
                        args.push(last);
                    } else {
                        require(args.is_empty(), "trace empty argument")?;
                    }
                    return Ok(Lexed {
                        end: at,
                        args,
                        opaque,
                        complete: true,
                    });
                }
            }
            b',' if stack.len() == 1 => {
                let part = trimmed(bytes, arg_start, at);
                require(part.start < part.end, "trace empty argument")?;
                args.push(part);
                require(args.len() <= 512, "trace argument count exceeds bound")?;
                arg_start = at + 1;
            }
            b'.' if bytes.get(at..at + 3) == Some(b"...") => {
                opaque = true;
                at += 3;
                continue;
            }
            b if b.is_ascii_alphanumeric() || b"_ \t,:;=!|&~+-*/.%?@>".contains(&b) => {}
            _ => return Err("trace lexical token invalid".into()),
        }
        at += 1;
    }
    require(partial, "trace delimiters invalid")?;
    Ok(Lexed {
        end: at,
        args,
        opaque,
        complete: false,
    })
}
fn name_end(bytes: &[u8]) -> Result<usize> {
    let end = bytes
        .iter()
        .position(|b| *b == b'(')
        .ok_or("trace syscall name invalid")?;
    require(
        end > 0
            && end <= 64
            && bytes[0].is_ascii_lowercase()
            && bytes[..end]
                .iter()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_'),
        "trace syscall name invalid",
    )?;
    Ok(end)
}
fn list(bytes: &[u8], range: &Range) -> Result<Option<Vec<BytesAt>>> {
    if &bytes[range.start..range.end] == b"NULL" {
        return Ok(None);
    }
    require(
        bytes.get(range.start) == Some(&b'['),
        "trace exec vector invalid",
    )?;
    let mut at = range.start + 1;
    let mut result = Vec::new();
    let mut need_value = false;
    loop {
        while bytes.get(at).is_some_and(|b| b.is_ascii_whitespace()) {
            at += 1;
        }
        if bytes.get(at) == Some(&b']') {
            require(
                !need_value && at + 1 == range.end,
                "trace exec vector invalid",
            )?;
            return Ok(Some(result));
        }
        require(
            at < range.end && result.len() < 4096,
            "trace exec vector invalid",
        )?;
        let value = quoted(bytes, &mut at)?;
        require(!value.bytes.contains(&0), "trace exec contains NUL")?;
        result.push(value);
        while bytes.get(at).is_some_and(|b| b.is_ascii_whitespace()) {
            at += 1;
        }
        match bytes.get(at) {
            Some(b',') => {
                at += 1;
                need_value = true;
            }
            Some(b']') => need_value = false,
            _ => return Err("trace exec vector invalid".into()),
        }
    }
}
fn exec(bytes: &[u8], operation: &str, args: &[Range], opaque: bool) -> Result<Exec> {
    require(!opaque, "trace exec abbreviation forbidden")?;
    let offset = usize::from(operation == "execveat");
    require(
        args.len() == if offset == 0 { 3 } else { 5 },
        "trace exec argument count invalid",
    )?;
    let mut at = args[offset].start;
    let program = quoted(bytes, &mut at)?;
    require(
        at == args[offset].end && !program.bytes.contains(&0),
        "trace exec program invalid",
    )?;
    let argv = list(bytes, &args[offset + 1])?;
    let source = args[offset + 2].clone();
    let raw = &bytes[source.start..source.end];
    let environment = if raw.starts_with(b"[") || raw == b"NULL" {
        Environment {
            representation: if raw == b"NULL" { "null" } else { "vector" }.into(),
            entries: list(bytes, &source)?,
            source,
            complete: true,
        }
    } else {
        let text = std::str::from_utf8(raw)?;
        let (pointer, comment) = text.split_once(" /* ").unwrap_or((text, ""));
        require(
            pointer
                .strip_prefix("0x")
                .is_some_and(|v| !v.is_empty() && v.bytes().all(|b| b.is_ascii_hexdigit())),
            "trace exec environment invalid",
        )?;
        if !comment.is_empty() {
            let count = comment
                .strip_suffix(" vars */")
                .or_else(|| comment.strip_suffix(" var */"))
                .ok_or("trace exec environment invalid")?;
            require(
                !count.is_empty() && count.bytes().all(|b| b.is_ascii_digit()),
                "trace exec environment invalid",
            )?;
        }
        Environment {
            representation: "pointer".into(),
            entries: None,
            source,
            complete: false,
        }
    };
    Ok(Exec {
        program,
        argv,
        environment,
        dirfd: (offset == 1).then(|| args[0].clone()),
        flags: (offset == 1).then(|| args[4].clone()),
    })
}
fn errno(text: &str) -> Result<()> {
    let (code, description) = text.split_once(" (").ok_or("trace result invalid")?;
    require(
        code.starts_with('E')
            && code.len() > 1
            && code.len() <= 64
            && code
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
            && description.ends_with(')')
            && description.len() > 1
            && description.bytes().all(|b| (0x20..=0x7e).contains(&b)),
        "trace result invalid",
    )
}
fn result(raw: &[u8]) -> Result<String> {
    let text = std::str::from_utf8(raw)?;
    if text == "?" {
        return Ok("no_return".into());
    }
    if let Some(rest) = text.strip_prefix("? ") {
        errno(rest)?;
        require(rest.starts_with("ERESTART"), "trace result invalid")?;
        return Ok("interrupted".into());
    }
    if let Some(rest) = text.strip_prefix("-1 ") {
        errno(rest)?;
        return Ok("failed".into());
    }
    let mut at = usize::from(raw.first() == Some(&b'-'));
    let begin = at;
    if raw.get(at..at + 2) == Some(b"0x") {
        at += 2;
        let begin = at;
        while raw.get(at).is_some_and(u8::is_ascii_hexdigit) {
            at += 1;
        }
        require(at > begin, "trace result invalid")?;
    } else {
        while raw.get(at).is_some_and(u8::is_ascii_digit) {
            at += 1;
        }
        require(at > begin, "trace result invalid")?;
    }
    if raw.get(at) == Some(&b'<') {
        annotation(raw, &mut at)?;
    }
    let tail = trimmed(raw, at, raw.len());
    if tail.start < tail.end {
        let parsed = balanced(raw, tail.start, false)?;
        require(parsed.end + 1 == tail.end, "trace result invalid")?;
    }
    Ok("returned".into())
}
// Recognise the observed raw=read,pread64 spelling, not an FD binding or buffer.
fn raw_number(raw: &[u8]) -> bool {
    raw == b"0"
        || raw.strip_prefix(b"0x").is_some_and(|digits| {
            !digits.is_empty() && digits.len() <= 16 && digits.iter().all(u8::is_ascii_hexdigit)
        })
}
fn call(pid: u32, bytes: &[u8], source: Vec<Fragment>) -> Result<Event> {
    let end = name_end(bytes)?;
    let operation = std::str::from_utf8(&bytes[..end])?.to_owned();
    let parsed = balanced(bytes, end, false)?;
    let rest = trimmed(bytes, parsed.end + 1, bytes.len());
    require(
        bytes.get(rest.start..rest.start + 2) == Some(b"= "),
        "trace result separator invalid",
    )?;
    let raw = &bytes[rest.start + 2..rest.end];
    let outcome = result(raw)?;
    let executable = operation == "execve" || operation == "execveat";
    let model = if executable {
        Some(exec(bytes, &operation, &parsed.args, parsed.opaque)?)
    } else {
        None
    };
    if executable {
        require(
            raw == b"0" || outcome == "failed" || outcome == "no_return",
            "trace exec result invalid",
        )?;
    }
    let mut row = event(pid, "syscall", source);
    row.operation = Some(operation);
    row.arguments = Some(Range {
        start: end + 1,
        end: parsed.end,
    });
    row.result = Some(raw.to_vec());
    row.result_source = Some(Range {
        start: rest.start + 2,
        end: rest.end,
    });
    row.outcome = Some(if executable && raw == b"0" {
        "exec_succeeded".into()
    } else {
        outcome
    });
    row.exec = model;
    if matches!(row.operation.as_deref(), Some("read" | "pread64"))
        && parsed.args.len()
            == if row.operation.as_deref() == Some("read") {
                3
            } else {
                4
            }
        && parsed
            .args
            .iter()
            .all(|r| raw_number(&bytes[r.start..r.end]))
    {
        row.numeric_read_arguments = Some(
            parsed
                .args
                .iter()
                .map(|r| BytesAt {
                    bytes: bytes[r.start..r.end].to_vec(),
                    source: r.clone(),
                })
                .collect(),
        );
        if raw_number(raw) {
            row.numeric_read_result = Some(BytesAt {
                bytes: raw.to_vec(),
                source: Range {
                    start: rest.start + 2,
                    end: rest.end,
                },
            });
        }
    }
    // A printable I/O prefix is not a decoded complete payload contract.
    row.opaque_payload = parsed.opaque
        || matches!(
            row.operation.as_deref(),
            Some(
                "read"
                    | "readv"
                    | "pread64"
                    | "preadv"
                    | "preadv2"
                    | "write"
                    | "writev"
                    | "pwrite64"
                    | "pwritev"
                    | "pwritev2"
                    | "sendto"
                    | "recvfrom"
                    | "sendmsg"
                    | "recvmsg"
                    | "sendmmsg"
                    | "recvmmsg"
            )
        );
    Ok(row)
}

struct ReadAt<'a> {
    file: &'a File,
    offset: u64,
}
impl Read for ReadAt<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let n = self.file.read_at(buffer, self.offset)?;
        self.offset += n as u64;
        Ok(n)
    }
}
fn bounded_line(reader: &mut impl BufRead, line: &mut Vec<u8>) -> Result<bool> {
    line.clear();
    loop {
        let bytes = reader.fill_buf()?;
        if bytes.is_empty() {
            require(line.is_empty(), "trace final newline missing")?;
            return Ok(false);
        }
        let n = bytes
            .iter()
            .position(|b| *b == b'\n')
            .map_or(bytes.len(), |i| i + 1);
        require(line.len() + n <= MAX_LINE_BYTES, "trace line exceeds bound")?;
        line.extend_from_slice(&bytes[..n]);
        reader.consume(n);
        if line.last() == Some(&b'\n') {
            return Ok(true);
        }
    }
}
fn timestamp(line: &[u8], previous: &mut u64) -> Result<(usize, String)> {
    let end = line
        .iter()
        .position(|b| *b == b' ')
        .ok_or("trace timestamp invalid")?;
    let raw = std::str::from_utf8(&line[..end])?;
    let (whole, fraction) = raw.split_once('.').ok_or("trace timestamp invalid")?;
    require(
        !whole.is_empty()
            && whole.bytes().all(|b| b.is_ascii_digit())
            && fraction.len() == 6
            && fraction.bytes().all(|b| b.is_ascii_digit()),
        "trace timestamp invalid",
    )?;
    let value = whole
        .parse::<u64>()
        .ok()
        .and_then(|v| v.checked_mul(1_000_000))
        .and_then(|v| v.checked_add(fraction.parse::<u64>().ok()?))
        .ok_or("trace timestamp invalid")?;
    require(value >= *previous, "trace timestamp invalid")?;
    *previous = value;
    let mut start = end;
    while line.get(start) == Some(&b' ') {
        start += 1;
    }
    require(start < line.len(), "trace body missing")?;
    Ok((start, raw.into()))
}
struct Pending {
    operation: String,
    bytes: Vec<u8>,
    source: Vec<Fragment>,
}
fn partial(bytes: &[u8]) -> Result<String> {
    let end = name_end(bytes)?;
    let parsed = balanced(bytes, end, true)?;
    if matches!(&bytes[..end], b"execve" | b"execveat") {
        require(!parsed.opaque, "trace exec abbreviation forbidden")?;
    }
    require(
        !parsed.complete || bytes[parsed.end + 1..].iter().all(u8::is_ascii_whitespace),
        "trace unfinished call invalid",
    )?;
    Ok(std::str::from_utf8(&bytes[..end])?.into())
}
fn signal_name(text: &str) -> bool {
    text.strip_prefix("SIG").is_some_and(|tail| {
        !tail.is_empty()
            && tail
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b"_+-".contains(&b))
    })
}
fn marker(body: &[u8]) -> Result<Option<(String, Option<String>)>> {
    if let Some(name) = body
        .strip_prefix(b"--- stopped by ")
        .and_then(|v| v.strip_suffix(b" ---"))
    {
        require(
            signal_name(std::str::from_utf8(name)?),
            "trace signal invalid",
        )?;
        return Ok(Some(("signal_stop".into(), None)));
    }
    if let Some(rest) = body.strip_prefix(b"--- ") {
        let text = std::str::from_utf8(rest)?;
        let (name, payload) = text.split_once(' ').ok_or("trace signal invalid")?;
        require(signal_name(name), "trace signal invalid")?;
        let payload = payload.strip_suffix(" ---").ok_or("trace signal invalid")?;
        require(
            payload.starts_with(&format!("{{si_signo={name},")),
            "trace signal invalid",
        )?;
        let parsed = balanced(payload.as_bytes(), 0, false)?;
        require(
            parsed.end + 1 == payload.len() && !parsed.opaque,
            "trace signal invalid",
        )?;
        return Ok(Some(("signal".into(), None)));
    }
    if let Some(code) = body
        .strip_prefix(b"+++ exited with ")
        .and_then(|v| v.strip_suffix(b" +++"))
    {
        require(
            !code.is_empty()
                && code.iter().all(u8::is_ascii_digit)
                && std::str::from_utf8(code)?
                    .parse::<u16>()
                    .is_ok_and(|n| n <= 255),
            "trace terminal invalid",
        )?;
        return Ok(Some(("terminal_exit".into(), None)));
    }
    if let Some(raw) = body
        .strip_prefix(b"+++ killed by ")
        .and_then(|v| v.strip_suffix(b" +++"))
    {
        let raw = std::str::from_utf8(raw)?;
        let name = raw.strip_suffix(" (core dumped)").unwrap_or(raw);
        require(signal_name(name), "trace terminal invalid")?;
        return Ok(Some(("terminal_signal".into(), Some(name.into()))));
    }
    Ok(None)
}
fn parse_file(
    file: &mut Held,
    events: &mut Vec<Event>,
    budget: &mut usize,
    lines: &mut usize,
) -> Result<()> {
    parse_file_emit(file, lines, &mut |row| push(events, budget, row))
}
fn parse_file_emit(
    file: &mut Held,
    lines: &mut usize,
    emit: &mut dyn FnMut(Event) -> Result<()>,
) -> Result<()> {
    let mut reader = BufReader::new(ReadAt {
        file: &file.file,
        offset: 0,
    });
    let mut bytes = Vec::new();
    let mut digest = Sha256::new();
    let mut offset = 0u64;
    let mut previous = 0u64;
    let mut number = 0usize;
    let mut pending: Option<Pending> = None;
    let mut terminal = false;
    while bounded_line(&mut reader, &mut bytes)? {
        *lines += 1;
        number += 1;
        require(*lines <= MAX_LINES, "trace line count exceeds bound")?;
        require(!terminal, "trace record after terminal")?;
        require(
            bytes[..bytes.len() - 1]
                .iter()
                .all(|b| (*b >= 0x20 && *b != 0x7f) || *b == b'\t'),
            "trace control byte invalid",
        )?;
        digest.update(&bytes);
        let (start, time) = timestamp(&bytes[..bytes.len() - 1], &mut previous)?;
        let body = &bytes[start..bytes.len() - 1];
        let line_offset = offset;
        let fragment = |column: usize, length: usize, logical_offset: usize| Fragment {
            line: number,
            line_offset,
            line_bytes: bytes.len(),
            column,
            length,
            logical_offset,
            timestamp: time.clone(),
        };
        offset = offset
            .checked_add(bytes.len() as u64)
            .ok_or("trace byte count overflow")?;
        require(
            offset <= file.before.size && offset <= MAX_FILE_BYTES,
            "trace input changed",
        )?;
        if let Some((kind, killed)) = marker(body)? {
            let source = vec![fragment(start, body.len(), 0)];
            if kind.starts_with("terminal_") {
                if let Some(call) = pending.take() {
                    let name = killed
                        .clone()
                        .ok_or("trace unresolved pending at terminal")?;
                    let mut row = event(file.pid, "aborted", call.source);
                    row.operation = Some(call.operation);
                    row.outcome = Some("aborted".into());
                    row.opaque_payload = true;
                    row.aborted_by = Some(name);
                    emit(row)?;
                }
                terminal = true;
            }
            let mut row = event(file.pid, &kind, source);
            row.result = Some(body.to_vec());
            emit(row)?;
            continue;
        }
        let unfinished = body.ends_with(b"<unfinished ...>");
        let end = body.len()
            - if unfinished {
                b"<unfinished ...>".len()
            } else {
                0
            };
        let mut call = if let Some(resumed) = body.strip_prefix(b"<... ") {
            let boundary = resumed
                .windows(b" resumed>".len())
                .position(|w| w == b" resumed>")
                .ok_or("trace resume invalid")?;
            let name = std::str::from_utf8(&resumed[..boundary])?;
            let mut call = pending.take().ok_or("trace orphan resumed syscall")?;
            require(name == call.operation, "trace resumed syscall differs")?;
            let skip = b"<... ".len() + boundary + b" resumed>".len();
            require(skip <= end, "trace resume invalid")?;
            call.source
                .push(fragment(start + skip, end - skip, call.bytes.len()));
            call.bytes.extend_from_slice(&body[skip..end]);
            call
        } else {
            require(pending.is_none(), "trace nested pending syscall")?;
            Pending {
                operation: String::new(),
                bytes: body[..end].to_vec(),
                source: vec![fragment(start, end, 0)],
            }
        };
        require(
            call.bytes.len() <= MAX_LINE_BYTES && call.source.len() <= 16,
            "trace joined call exceeds bound",
        )?;
        if unfinished {
            call.operation = partial(&call.bytes)?;
            pending = Some(call);
        } else {
            emit(self::call(file.pid, &call.bytes, call.source)?)?;
        }
    }
    require(pending.is_none(), "trace unresolved pending at EOF")?;
    require(terminal && number > 0, "trace terminal missing")?;
    require(
        offset == file.before.size && identity(&file.file.metadata()?) == file.before,
        "trace input changed",
    )?;
    file.sha256 = format!("{:x}", digest.finalize());
    Ok(())
}

#[derive(Serialize)]
struct FileReport<'a> {
    pid: u32,
    name: &'a str,
    identity_before: &'a Identity,
    identity_after: Identity,
    sha256: &'a str,
}
#[derive(Serialize)]
struct Report<'a> {
    schema: u32,
    scope: &'static str,
    directory: &'a Path,
    prefix: &'a str,
    directory_identity: &'a Identity,
    directory_anchors: &'a [Anchor],
    raw_files: Vec<FileReport<'a>>,
    directory_roster: &'a [Roster],
    selected_raw_bytes: u64,
    events: &'a [Event],
    raw_immutable: bool,
    requires_exit_status_zero: bool,
    closed_pid: bool,
    closed_cwd: bool,
    closed_fd: bool,
    closed_dso: bool,
    full_acceptance: bool,
    retirement_credit: bool,
    limits: [&'static str; 5],
}
#[derive(Serialize)]
struct SummaryReport<'a> {
    #[serde(flatten)]
    report: Report<'a>,
    full_event_index: bool,
    lexically_checked_all_events: bool,
    event_index_scope: &'static str,
    event_count: usize,
    physical_lines: usize,
    event_counts: &'a std::collections::BTreeMap<String, usize>,
}
#[derive(Default)]
struct ReportBytes(Vec<u8>);
impl Write for ReportBytes {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self
            .0
            .len()
            .checked_add(bytes.len())
            .is_none_or(|size| size > MAX_MODEL_BYTES)
        {
            return Err(std::io::Error::other("trace report exceeds proof bound"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn verify_output_seal(
    file: &File,
    parent: &File,
    leaf: &str,
    expected: &Identity,
    expected_sha256: &str,
) -> Result<()> {
    let named = open_input(parent, leaf)?;
    require(
        expected.links == 1
            && identity(&file.metadata()?) == *expected
            && identity(&named.metadata()?) == *expected,
        "trace output changed",
    )?;
    let actual = hash(file, expected.size).map_err(|_| "trace output changed")?;
    let after = open_input(parent, leaf)?;
    require(
        actual == expected_sha256
            && identity(&file.metadata()?) == *expected
            && identity(&after.metadata()?) == *expected,
        "trace output changed",
    )
}
fn inspect(path: &Path, prefix: &str, output: &Path) -> Result<()> {
    inspect_mode(path, prefix, output, false)
}
fn inspect_mode(path: &Path, prefix: &str, output: &Path, summarized: bool) -> Result<()> {
    absolute(output)?;
    require(
        !output.starts_with(path),
        "trace output must be outside input directory",
    )?;
    let parent = output.parent().ok_or("trace output parent missing")?;
    let (parent_fd, parent_anchors) = directory(parent)?;
    let leaf = output
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("trace output leaf invalid")?;
    match rustix::fs::statat(
        &parent_fd,
        Path::new(leaf),
        rustix::fs::AtFlags::SYMLINK_NOFOLLOW,
    ) {
        Err(rustix::io::Errno::NOENT) => {}
        _ => return Err("trace output already exists or unsafe".into()),
    }
    let mut input = Inputs::open(path, prefix)?;
    let mut summary = None;
    let mut physical_lines = 0;
    let events = if summarized {
        let mut collected = EventSummary::default();
        for file in &mut input.files {
            parse_file_emit(file, &mut physical_lines, &mut |row| collected.observe(row))?;
        }
        summary = Some(collected);
        Vec::new()
    } else {
        input.events()?
    };
    input.verify()?;
    let files = input
        .files
        .iter()
        .map(|f| {
            Ok(FileReport {
                pid: f.pid,
                name: &f.name,
                identity_before: &f.before,
                identity_after: identity(&f.file.metadata()?),
                sha256: &f.sha256,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let report = Report {
        schema: if summarized { 2 } else { 1 },
        scope: if summarized {
            "all raw lines checked by shared lexer; syscall details summarized; no execution policy or family admission"
        } else {
            "read-only rich strace lexical inspection; no execution policy or family admission"
        },
        directory: path,
        prefix,
        directory_identity: &input.before,
        directory_anchors: &input.anchors,
        raw_files: files,
        directory_roster: &input.roster,
        selected_raw_bytes: input.total,
        events: summary.as_ref().map_or(events.as_slice(), |s| s.events.as_slice()),
        raw_immutable: true,
        requires_exit_status_zero: true,
        closed_pid: false,
        closed_cwd: false,
        closed_fd: false,
        closed_dso: false,
        full_acceptance: false,
        retirement_credit: false,
        limits: [
            "per-PID terminal syntax is not closed-family or wait/reap proof",
            "general I/O payloads and aborted calls remain opaque with raw source spans",
            "raw trace bytes outside the selected prefix are not content-hashed",
            "same-UID visible checks; no hostile same-UID or privileged exclusion",
            "write/final-check failure may leave diagnostic output; require successful command status",
        ],
    };
    let mut writer = ReportBytes::default();
    if let Some(summary) = &summary {
        serde_json::to_writer(
            &mut writer,
            &SummaryReport {
                report,
                full_event_index: false,
                lexically_checked_all_events: true,
                event_index_scope: "all exec attempts, signals, terminals and aborted calls; other syscall details remain in immutable raw files",
                event_count: summary.event_count,
                physical_lines,
                event_counts: &summary.counts,
            },
        )?;
    } else {
        serde_json::to_writer(&mut writer, &report)?;
    }
    writer.write_all(b"\n")?;
    let bytes = writer.0;
    require(
        bytes.len() <= MAX_MODEL_BYTES && input.total + bytes.len() as u64 <= MAX_TOTAL_BYTES,
        "trace report exceeds proof bound",
    )?;
    let (now, anchors) = directory(parent)?;
    require(
        anchors == parent_anchors
            && (now.metadata()?.dev(), now.metadata()?.ino())
                == (parent_fd.metadata()?.dev(), parent_fd.metadata()?.ino()),
        "trace output parent changed",
    )?;
    let mut file = File::from(rustix::fs::openat(
        &parent_fd,
        Path::new(leaf),
        rustix::fs::OFlags::RDWR
            | rustix::fs::OFlags::CREATE
            | rustix::fs::OFlags::EXCL
            | rustix::fs::OFlags::NOFOLLOW
            | rustix::fs::OFlags::NONBLOCK
            | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
    )?);
    file.write_all(&bytes)?;
    file.flush()?;
    // Keep the original post-write identity and expected bytes across all input rehashes.
    let output_identity = identity(&file.metadata()?);
    let output_sha256 = format!("{:x}", Sha256::digest(&bytes));
    require(
        output_identity.size == bytes.len() as u64,
        "trace output changed",
    )?;
    verify_output_seal(&file, &parent_fd, leaf, &output_identity, &output_sha256)?;
    input.verify()?;
    let (_, anchors) = directory(parent)?;
    require(anchors == parent_anchors, "trace output parent changed")?;
    verify_output_seal(&file, &parent_fd, leaf, &output_identity, &output_sha256)?;
    Ok(())
}
pub(super) fn selected(args: &[OsString]) -> bool {
    args.first()
        .is_some_and(|a| a == "inspect-trace" || a == "inspect-trace-summary")
}
pub(super) fn cli(args: &[OsString]) -> Result<()> {
    cli_mode(args, false)
}
pub(super) fn cli_selected(args: &[OsString]) -> Result<()> {
    require(selected(args), "trace subcommand required")?;
    if args[0] == "inspect-trace-summary" {
        cli_mode(&args[1..], true)
    } else {
        cli(&args[1..])
    }
}
fn cli_mode(args: &[OsString], summarized: bool) -> Result<()> {
    let parsed = scan_os_required_first(
        args,
        &[
            OptionSpec::value("--directory", true),
            OptionSpec::value("--prefix", true),
            OptionSpec::value("--output", true),
        ],
    )?;
    if parsed.help {
        let command = if summarized {
            "inspect-trace-summary"
        } else {
            "inspect-trace"
        };
        println!("usage: check-native-logging {command} --directory DIR --prefix PREFIX --output ABSENT_JSON");
        return Ok(());
    }
    let directory = Path::new(&parsed.values["--directory"]);
    let prefix = parsed.values["--prefix"]
        .to_str()
        .ok_or("trace prefix not UTF8")?;
    let output = Path::new(&parsed.values["--output"]);
    if summarized {
        inspect_mode(directory, prefix, output, true)
    } else {
        inspect(directory, prefix, output)
    }
}

#[cfg(test)]
#[path = "trace_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "trace_summary_tests.rs"]
mod summary_tests;
