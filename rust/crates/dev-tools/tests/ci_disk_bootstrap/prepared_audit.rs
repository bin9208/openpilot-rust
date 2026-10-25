//! Fast prepared SDK의 exec trace 데이터 감사. 자식 프로세스나 SDK 코드는 실행하지 않는다.
//! strace -f -qq -s 65535 -e trace=execve,execveat 형식만 받는다.
//! C string은 ASCII, \\, \" 및 표준 단문 escape와 정확히 세 자리 octal만 지원한다.
//! 비ASCII·NUL·알 수 없는 escape·abbreviation은 거부한다. 환경은 NULL/문자열 배열/주소+vars 주석이다.
//! 합성 parser 통과는 실제 SDK qualification이 아니다. 실제 증거 소비는 마지막 ignored test뿐이다.
//! 실제 입력은 CI_PREPARED_RUST_ROOT/MANIFEST/PROOF, CI_JOB_TEMP와 GitHub job/source 환경이다.
//! PROOF는 해당 job의 proof 디렉터리다. 결과는 prepared-cold-bootstrap.json에 원자적으로 기록한다.
//! owned job/proof 입장 직후 이전 receipt를 무효화하고 manifest/trace identity·hash를 기록 전 재검증한다.
//! 이 감사가 전체 SDK roster/source-index의 전후 불변성을 대체하지는 않는다. 그 범위는 workflow가 유지한다.
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Component, Path, PathBuf},
};

// 기존 합성 trace의 고정 경로다. 실제 hosted admission은 이 root를 거부한다.
const SDK: &str = "/home/actions/.rustup/toolchains/1.94.0-x86_64-unknown-linux-gnu";
const CARGO_SHA: &str = "77f14b761b02b47e6747473f556b3bc9f98f7e4525b7c3b8d74898ff816e4636";
const RUSTC_SHA: &str = "103b60e1b1339968c1d74202ea1d45686037e82c4ea3e0569de24b18a1e6836a";
type Result<T> = std::result::Result<T, String>;

struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
}
impl<'a> Cursor<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            bytes: text.as_bytes(),
            at: 0,
        }
    }
    fn ws(&mut self) {
        while self
            .bytes
            .get(self.at)
            .is_some_and(|b| matches!(*b, b' ' | b'\t'))
        {
            self.at += 1;
        }
    }
    fn eat(&mut self, token: &[u8]) -> bool {
        self.ws();
        if self.bytes[self.at..].starts_with(token) {
            self.at += token.len();
            true
        } else {
            false
        }
    }
    fn need(&mut self, token: &[u8]) -> Result<()> {
        if self.eat(token) {
            Ok(())
        } else {
            Err(format!(
                "malformed exec at byte {}: expected {:?}",
                self.at, token
            ))
        }
    }
    fn string(&mut self) -> Result<String> {
        self.need(b"\"")?;
        let mut output = Vec::new();
        loop {
            let byte = *self.bytes.get(self.at).ok_or("truncated C string")?;
            self.at += 1;
            let decoded = match byte {
                b'"' => return String::from_utf8(output).map_err(|e| e.to_string()),
                b'\\' => {
                    let escape = *self.bytes.get(self.at).ok_or("truncated C escape")?;
                    self.at += 1;
                    match escape {
                        b'\\' | b'"' => escape,
                        b'a' => 7,
                        b'b' => 8,
                        b'f' => 12,
                        b'n' => 10,
                        b'r' => 13,
                        b't' => 9,
                        b'v' => 11,
                        b'0'..=b'7' => {
                            let rest = self
                                .bytes
                                .get(self.at..self.at + 2)
                                .ok_or("truncated octal escape")?;
                            if !rest.iter().all(|b| matches!(*b, b'0'..=b'7')) {
                                return Err("unknown/short octal escape".into());
                            }
                            self.at += 2;
                            let value = u16::from(escape - b'0') * 64
                                + u16::from(rest[0] - b'0') * 8
                                + u16::from(rest[1] - b'0');
                            if value == 0 || value > 127 {
                                return Err("C string outside non-NUL ASCII scope".into());
                            }
                            value as u8
                        }
                        _ => return Err("unknown C escape".into()),
                    }
                }
                32..=126 => byte,
                _ => return Err("literal byte outside printable ASCII scope".into()),
            };
            output.push(decoded);
            if output.len() > 65535 {
                return Err("C string exceeds the admitted strace -s limit".into());
            }
        }
    }
    fn array(&mut self) -> Result<()> {
        if self.eat(b"NULL") {
            return Ok(());
        }
        self.need(b"[")?;
        if self.eat(b"]") {
            return Ok(());
        }
        loop {
            self.string()?;
            if self.eat(b"]") {
                return Ok(());
            }
            self.need(b",")?;
        }
    }
    fn environment(&mut self) -> Result<()> {
        self.ws();
        if self.bytes.get(self.at) == Some(&b'[') || self.bytes[self.at..].starts_with(b"NULL") {
            return self.array();
        }
        self.need(b"0x")?;
        let start = self.at;
        while self.bytes.get(self.at).is_some_and(u8::is_ascii_hexdigit) {
            self.at += 1;
        }
        if self.at == start {
            return Err("malformed environment address".into());
        }
        if self.eat(b"/*") {
            let start = self.at;
            while self.bytes.get(self.at..self.at + 2) != Some(b"*/") {
                if self.at >= self.bytes.len() {
                    return Err("truncated environment comment".into());
                }
                self.at += 1;
            }
            let comment = std::str::from_utf8(&self.bytes[start..self.at])
                .map_err(|e| e.to_string())?
                .trim();
            let count = comment
                .strip_suffix(" vars")
                .or_else(|| comment.strip_suffix(" var"))
                .ok_or("unknown environment annotation")?;
            if count.is_empty() || !count.bytes().all(|b| b.is_ascii_digit()) {
                return Err("malformed environment count".into());
            }
            self.at += 2;
        }
        Ok(())
    }
    fn end(&mut self) -> Result<()> {
        self.ws();
        if self.at == self.bytes.len() {
            Ok(())
        } else {
            Err("truncated/unknown trailing exec text".into())
        }
    }
}

#[derive(Debug)]
struct Exec {
    program: String,
    success: bool,
}
fn exec(text: &str) -> Result<Exec> {
    let mut cursor = Cursor::new(text);
    cursor.need(b"execve(")?;
    let program = cursor.string()?;
    if program.is_empty() {
        return Err("empty executable path".into());
    }
    cursor.need(b",")?;
    cursor.array()?;
    cursor.need(b",")?;
    cursor.environment()?;
    cursor.need(b")")?;
    cursor.need(b"=")?;
    let success = if cursor.eat(b"0") {
        true
    } else {
        cursor.need(b"-1")?;
        cursor.ws();
        let start = cursor.at;
        while cursor
            .bytes
            .get(cursor.at)
            .is_some_and(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || *b == b'_')
        {
            cursor.at += 1;
        }
        if cursor.at == start || !cursor.bytes[start].is_ascii_uppercase() {
            return Err("missing failure errno".into());
        }
        cursor.need(b"(")?;
        let start = cursor.at;
        while cursor.bytes.get(cursor.at) != Some(&b')') {
            let byte = *cursor
                .bytes
                .get(cursor.at)
                .ok_or("truncated failure description")?;
            if !(32..=126).contains(&byte) || matches!(byte, b'(' | b'"' | b'\\') {
                return Err("malformed failure description".into());
            }
            cursor.at += 1;
        }
        if cursor.at == start {
            return Err("empty failure description".into());
        }
        cursor.need(b")")?;
        false
    };
    cursor.end()?;
    Ok(Exec { program, success })
}
fn pid_line(line: &str) -> Result<(u32, &str)> {
    let (number, rest) = if let Some(rest) = line.strip_prefix("[pid ") {
        let (pid, body) = rest.split_once(']').ok_or("malformed bracket PID")?;
        (pid.trim(), body.trim_start())
    } else {
        let (pid, body) = line
            .split_once(char::is_whitespace)
            .ok_or("missing trace PID")?;
        (pid, body.trim_start())
    };
    if number.is_empty() || !number.bytes().all(|b| b.is_ascii_digit()) {
        return Err("malformed trace PID".into());
    }
    let pid = number.parse::<u32>().map_err(|e| e.to_string())?;
    if pid == 0 || rest.is_empty() {
        return Err("invalid trace PID/body".into());
    }
    Ok((pid, rest))
}
fn metadata(body: &str) -> bool {
    // 실제 거부 trace에서 관측한 SIGCHLD 주석의 소수점/time comment도 허용한다.
    if let Some(signal) = body
        .strip_prefix("--- ")
        .and_then(|v| v.strip_suffix(" ---"))
    {
        let Some((name, info)) = signal.split_once(' ') else {
            return false;
        };
        return name.starts_with("SIG")
            && name
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
            && info.starts_with(&format!("{{si_signo={name},"))
            && info.ends_with('}')
            && info
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_{}=, ./*+-".contains(&b))
            && balanced_siginfo(info);
    }
    if let Some(code) = body
        .strip_prefix("+++ exited with ")
        .and_then(|v| v.strip_suffix(" +++"))
    {
        return !code.is_empty()
            && code.bytes().all(|b| b.is_ascii_digit())
            && code.parse::<u8>().is_ok();
    }
    false
}
fn balanced_siginfo(info: &str) -> bool {
    let bytes = info.as_bytes();
    let (mut at, mut depth) = (0, 0usize);
    while at < bytes.len() {
        if bytes[at..].starts_with(b"/*") {
            let Some(end) = info[at + 2..].find("*/") else {
                return false;
            };
            if info[at + 2..at + 2 + end].contains("/*") {
                return false;
            }
            at += end + 4;
            continue;
        }
        match bytes[at] {
            b'{' => depth += 1,
            b'}' => {
                let Some(next) = depth.checked_sub(1) else {
                    return false;
                };
                depth = next;
                if depth == 0 && at + 1 != bytes.len() {
                    return false;
                }
            }
            b'/' | b'*' => return false,
            _ => (),
        }
        at += 1;
    }
    depth == 0
}
#[derive(Debug)]
struct Audit {
    calls: Vec<Exec>,
    cargo_successes: usize,
    rustc_successes: usize,
}
fn audit_trace(text: &str) -> Result<Audit> {
    audit_trace_for_sdk(text, Path::new(SDK))
}
fn audit_trace_for_sdk(text: &str, sdk: &Path) -> Result<Audit> {
    if text.is_empty() || !text.is_ascii() || text.contains(['\r', '\0']) || !text.ends_with('\n') {
        return Err("empty, non-ASCII or truncated trace".into());
    }
    let mut pending = BTreeMap::new();
    let mut exited = BTreeSet::new();
    let mut calls = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let (pid, body) = pid_line(line).map_err(|e| format!("line {}: {e}", index + 1))?;
        if exited.contains(&pid) {
            return Err("trace reuses an exited PID".into());
        }
        if body.starts_with("execveat(") || body.starts_with("<... execveat resumed>") {
            return Err("execveat is not admitted".into());
        }
        let complete = if let Some(tail) = body.strip_prefix("<... execve resumed>") {
            if tail.trim().is_empty() {
                return Err("truncated execve resume".into());
            }
            let head: String = pending
                .remove(&pid)
                .ok_or("orphan/mismatched PID execve resume")?;
            Some(format!("{head}{tail}"))
        } else if body.starts_with("execve(") {
            if pending.contains_key(&pid) {
                return Err("second exec before same PID resumed".into());
            }
            if let Some(head) = body.strip_suffix("<unfinished ...>") {
                pending.insert(pid, head.to_owned());
                None
            } else {
                Some(body.to_owned())
            }
        } else if metadata(body) {
            if body.starts_with("+++") {
                if pending.contains_key(&pid) {
                    return Err("PID exited with an unresolved execve".into());
                }
                exited.insert(pid);
            }
            None
        } else {
            return Err(format!("unrecognized trace line {}", index + 1));
        };
        if let Some(complete) = complete {
            let call = exec(&complete)?;
            let basename = Path::new(&call.program)
                .file_name()
                .and_then(|v| v.to_str())
                .ok_or("missing executable basename")?
                .to_ascii_lowercase();
            if basename.contains("rustup")
                || basename.contains("python")
                || basename.ends_with(".py")
            {
                return Err(format!(
                    "forbidden exec attempt, including failures: {}",
                    call.program
                ));
            }
            calls.push(call);
        }
    }
    if !pending.is_empty() {
        return Err("unresolved unfinished execve PID(s)".into());
    }
    let successes = |name: &str| {
        calls
            .iter()
            .filter(|c| c.success && c.program == format!("{}/bin/{name}", sdk.display()))
            .count()
    };
    let cargo_successes = successes("cargo");
    let rustc_successes = successes("rustc");
    if cargo_successes == 0 || rustc_successes == 0 {
        return Err("missing successful absolute SDK cargo/rustc exec".into());
    }
    Ok(Audit {
        calls,
        cargo_successes,
        rustc_successes,
    })
}

fn env(name: &str) -> Result<String> {
    std::env::var(name).map_err(|_| format!("mandatory actual-proof environment: {name}"))
}
fn owned(path: &Path, directory: bool) -> Result<fs::Metadata> {
    let metadata = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !path.is_absolute()
        || fs::canonicalize(path)
            .map_err(|e| e.to_string())?
            .as_os_str()
            != path.as_os_str()
        || metadata.file_type().is_symlink()
        || metadata.is_dir() != directory
        || (!directory && !metadata.is_file())
        || metadata.uid() != rustix::process::geteuid().as_raw()
        || metadata.mode() & 0o022 != 0
    {
        return Err(format!(
            "noncanonical, unowned or group/world-writable proof/SDK path: {}",
            path.display()
        ));
    }
    Ok(metadata)
}
fn open_regular(path: &Path) -> Result<File> {
    let metadata = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !metadata.is_file()
        || fs::canonicalize(path)
            .map_err(|e| e.to_string())?
            .as_os_str()
            != path.as_os_str()
    {
        return Err("noncanonical/nonregular manifest entry".into());
    }
    let file = OpenOptions::new()
        .read(true)
        .custom_flags((rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32)
        .open(path)
        .map_err(|e| e.to_string())?;
    let opened = file.metadata().map_err(|e| e.to_string())?;
    if !opened.is_file() || (opened.dev(), opened.ino()) != (metadata.dev(), metadata.ino()) {
        return Err("input identity changed while opening".into());
    }
    Ok(file)
}
#[derive(Debug, PartialEq, Eq)]
struct FileStamp {
    device: u64,
    inode: u64,
    uid: u32,
    gid: u32,
    mode: u32,
    size: u64,
    mtime: (i64, i64),
    ctime: (i64, i64),
}
impl FileStamp {
    fn of(metadata: &fs::Metadata) -> Self {
        Self {
            device: metadata.dev(),
            inode: metadata.ino(),
            uid: metadata.uid(),
            gid: metadata.gid(),
            mode: metadata.mode(),
            size: metadata.len(),
            mtime: (metadata.mtime(), metadata.mtime_nsec()),
            ctime: (metadata.ctime(), metadata.ctime_nsec()),
        }
    }
}
struct ProofInput {
    path: PathBuf,
    stamp: FileStamp,
    text: String,
    sha256: String,
}
impl ProofInput {
    fn read(path: &Path) -> Result<Self> {
        let stamp = FileStamp::of(&owned(path, false)?);
        if stamp.size > 64 * 1024 * 1024 {
            return Err("oversized audit metadata".into());
        }
        let mut file = open_regular(path)?;
        if FileStamp::of(&file.metadata().map_err(|e| e.to_string())?) != stamp {
            return Err("proof input changed before read".into());
        }
        let mut bytes = Vec::new();
        Read::by_ref(&mut file)
            .take(64 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() as u64 != stamp.size
            || FileStamp::of(&file.metadata().map_err(|e| e.to_string())?) != stamp
            || FileStamp::of(&owned(path, false)?) != stamp
        {
            return Err("proof input changed during read".into());
        }
        Ok(Self {
            path: path.to_owned(),
            stamp,
            sha256: sha(&bytes),
            text: String::from_utf8(bytes).map_err(|e| e.to_string())?,
        })
    }
    fn verify_unchanged(&self) -> Result<()> {
        let current = Self::read(&self.path)?;
        if current.stamp != self.stamp || current.sha256 != self.sha256 {
            return Err(format!("proof input changed: {}", self.path.display()));
        }
        Ok(())
    }
}
fn read_owned(path: &Path) -> Result<String> {
    Ok(ProofInput::read(path)?.text)
}
fn invalidate_owned_receipt(output: &Path) -> Result<()> {
    match fs::symlink_metadata(output) {
        Ok(_) => {
            // #380: job/proof/owner 입장 뒤에만 해당 owned regular receipt를 무효화한다.
            // symlink·타 UID·디렉터리·공유 writable 출력은 삭제하지 않고 거부한다.
            let stamp = FileStamp::of(&owned(output, false)?);
            if FileStamp::of(&fs::symlink_metadata(output).map_err(|e| e.to_string())?) != stamp {
                return Err("receipt identity changed before invalidation".into());
            }
            fs::remove_file(output).map_err(|e| e.to_string())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn check_manifest(manifest: &str, sdk: &Path) -> Result<usize> {
    // find/sha256sum이 만든 ASCII 절대 경로 형식만 받는다. GNU escaped filename은 별도 지원하지 않는다.
    if !manifest.is_ascii() || manifest.contains(['\r', '\0']) {
        return Err("unsupported SDK manifest path encoding".into());
    }
    let mut seen = BTreeSet::new();
    for line in manifest.lines() {
        let bytes = line.as_bytes();
        if bytes.len() < 67
            || !bytes[..64].iter().all(u8::is_ascii_hexdigit)
            || bytes[64] != b' '
            || !matches!(bytes[65], b' ' | b'*')
        {
            return Err("unsupported/malformed GNU SHA256 manifest line".into());
        }
        let path = Path::new(&line[66..]);
        if !path.is_absolute()
            || !path.starts_with(sdk)
            || path
                .components()
                .any(|v| matches!(v, Component::CurDir | Component::ParentDir))
            || !seen.insert(path.to_owned())
        {
            return Err("manifest entry outside exact SDK or duplicated".into());
        }
        let mut input = open_regular(path)?;
        let mut hash = Sha256::new();
        let mut buffer = [0; 65536];
        loop {
            let count = input.read(&mut buffer).map_err(|e| e.to_string())?;
            if count == 0 {
                break;
            }
            hash.update(&buffer[..count]);
        }
        let actual = format!("{:x}", hash.finalize());
        if !actual.eq_ignore_ascii_case(&line[..64]) {
            return Err(format!(
                "SDK manifest checksum mismatch: {}",
                path.display()
            ));
        }
        for (name, pinned) in [("cargo", CARGO_SHA), ("rustc", RUSTC_SHA)] {
            if path == sdk.join("bin").join(name) && actual != pinned {
                return Err(format!("SDK program pin mismatch: {name}"));
            }
        }
    }
    if !["cargo", "rustc"]
        .iter()
        .all(|name| seen.contains(&sdk.join("bin").join(name)))
    {
        return Err("manifest omits absolute SDK cargo/rustc".into());
    }
    Ok(seen.len())
}
fn private_hosted_sdk(job: &Path, sdk: &str, rustup: &str, cargo: &str) -> Result<PathBuf> {
    let expected = job.join("rustup-home/toolchains/1.94.0-x86_64-unknown-linux-gnu");
    if sdk != expected.to_str().ok_or("non-UTF8 private SDK")?
        || rustup
            != job
                .join("rustup-home")
                .to_str()
                .ok_or("non-UTF8 Rustup home")?
        || cargo
            != job
                .join("cargo-home")
                .to_str()
                .ok_or("non-UTF8 Cargo home")?
    {
        return Err("unapproved actual-proof private SDK/tool homes".into());
    }
    Ok(expected)
}
fn actual_sdk(
    job: &Path,
    environment: &str,
    runner: &str,
    sdk: &str,
    rustup: &str,
    cargo: &str,
) -> Result<PathBuf> {
    match environment {
        "github-hosted" => private_hosted_sdk(job, sdk, rustup, cargo),
        "self-hosted" if runner == "bin-actions" => {
            let expected =
                PathBuf::from("/home/actions/.rustup/toolchains/1.94.0-x86_64-unknown-linux-gnu");
            if sdk != expected.to_str().ok_or("non-UTF8 registered SDK")?
                || rustup
                    != job
                        .join("rustup-home")
                        .to_str()
                        .ok_or("non-UTF8 Rustup home")?
                || cargo
                    != job
                        .join("cargo-home")
                        .to_str()
                        .ok_or("non-UTF8 Cargo home")?
            {
                return Err("unapproved registered SDK/private tool homes".into());
            }
            Ok(expected)
        }
        _ => Err("unapproved actual-proof runner environment/name".into()),
    }
}

// Fast native에서는 환경 flag로 stock provider로 되돌아갈 수 없다. 기존 hosted
// diagnostic의 18/marker10/protocol3와 v1 marker body는 이 입구를 사용하지 않는다.
fn requires_private_provider(environment: &str, job: &str) -> bool {
    environment == "self-hosted" && job == "native"
}
struct ProviderProgram {
    path: PathBuf,
    stamp: FileStamp,
    sha256: String,
}
impl ProviderProgram {
    fn read(path: &Path) -> Result<Self> {
        let metadata = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
        if !metadata.is_file()
            || metadata.nlink() != 1
            || metadata.mode() & 0o022 != 0
            || metadata.mode() & 0o111 == 0
            || metadata.len() > 512 * 1024 * 1024
        {
            return Err("provider ELF shape/permissions/bound".into());
        }
        let stamp = FileStamp::of(&metadata);
        let mut file = open_regular(path)?;
        if FileStamp::of(&file.metadata().map_err(|e| e.to_string())?) != stamp {
            return Err("provider changed before hashing".into());
        }
        let mut header = [0; 64];
        file.read_exact(&mut header).map_err(|e| e.to_string())?;
        if header[..7] != [0x7f, b'E', b'L', b'F', 2, 1, 1]
            || header[18..20] != [62, 0]
            || !matches!(u16::from_le_bytes([header[16], header[17]]), 2 | 3)
            || header[20..24] != [1, 0, 0, 0]
            || header[52..54] != [64, 0]
            || header[54..56] != [56, 0]
            || u16::from_le_bytes([header[56], header[57]]) == 0
        {
            return Err("provider is not an actual x86_64 ELF".into());
        }
        let phoff = u64::from_le_bytes(header[32..40].try_into().map_err(|_| "ELF phoff")?);
        let phbytes = u64::from(u16::from_le_bytes([header[56], header[57]])) * 56;
        if phoff < 64
            || phoff
                .checked_add(phbytes)
                .is_none_or(|end| end > stamp.size)
        {
            return Err("provider ELF program header bound".into());
        }
        let mut hash = Sha256::new();
        hash.update(header);
        let mut count = 64u64;
        let mut buffer = [0; 65536];
        loop {
            let size = file.read(&mut buffer).map_err(|e| e.to_string())?;
            if size == 0 {
                break;
            }
            count += size as u64;
            if count > stamp.size {
                return Err("provider grew during hashing".into());
            }
            hash.update(&buffer[..size]);
        }
        if count != stamp.size
            || FileStamp::of(&file.metadata().map_err(|e| e.to_string())?) != stamp
            || FileStamp::of(&fs::symlink_metadata(path).map_err(|e| e.to_string())?) != stamp
        {
            return Err("provider changed during hashing".into());
        }
        Ok(Self {
            path: path.into(),
            stamp,
            sha256: format!("{:x}", hash.finalize()),
        })
    }
    fn verify(&self) -> Result<()> {
        let after = Self::read(&self.path)?;
        if after.stamp != self.stamp || after.sha256 != self.sha256 {
            return Err("provider changed before receipt publication".into());
        }
        Ok(())
    }
}
fn provider_input(inputs: &mut Vec<ProofInput>, path: PathBuf) -> Result<String> {
    if fs::symlink_metadata(&path)
        .map_err(|e| e.to_string())?
        .nlink()
        != 1
    {
        return Err("private provider evidence must be single-link".into());
    }
    let input = ProofInput::read(&path)?;
    let text = input.text.clone();
    inputs.push(input);
    Ok(text)
}
fn nul_arguments(text: &str) -> Result<Vec<&str>> {
    if !text.is_ascii() || !text.ends_with('\0') || text.len() > 65536 {
        return Err("launch argv encoding/bound/EOF".into());
    }
    let args: Vec<_> = text[..text.len() - 1].split('\0').collect();
    if args.iter().any(|arg| arg.is_empty()) {
        return Err("empty launch argument".into());
    }
    Ok(args)
}
fn held_roster(text: &str) -> Result<BTreeMap<&str, (&str, &str)>> {
    if !text.is_ascii() || !text.ends_with('\n') {
        return Err("held roster encoding/EOF".into());
    }
    let mut records = BTreeMap::new();
    for line in text.lines() {
        let fields: Vec<_> = line.split('\t').collect();
        if fields.len() != 3
            || !fields[0].starts_with('/')
            || fields[1].is_empty()
            || fields[2].len() != 64
            || !fields[2]
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || records.insert(fields[0], (fields[1], fields[2])).is_some()
        {
            return Err("held roster shape/duplicate/hash".into());
        }
    }
    Ok(records)
}
fn check_provider_roster(text: &str, program: &ProviderProgram) -> Result<()> {
    let stamp = &program.stamp;
    let path = program.path.to_str().ok_or("provider roster UTF8 path")?;
    let mut found = 0;
    for row in text
        .strip_suffix('\0')
        .ok_or("provider roster EOF")?
        .split('\0')
    {
        let fields: Vec<_> = row.split('\t').collect();
        if fields.len() != 8 {
            return Err("provider roster shape".into());
        }
        if fields[6] != path {
            continue;
        }
        found += 1;
        if fields[0] != format!("{}:{}", stamp.device, stamp.inode)
            || fields[1] != format!("{:o}:{}:{}:1", stamp.mode & 0o7777, stamp.uid, stamp.gid)
            || fields[2] != stamp.size.to_string()
            || fields[3] != format!("{}.{:09}0", stamp.mtime.0, stamp.mtime.1)
            || fields[4] != format!("{}.{:09}0", stamp.ctime.0, stamp.ctime.1)
            || fields[5] != "f"
            || !fields[7].is_empty()
        {
            return Err("actual provider metadata changed since cold preparation".into());
        }
    }
    if found != 1 {
        return Err("provider omitted/duplicated in complete bin roster".into());
    }
    Ok(())
}
struct FastProvider {
    inputs: Vec<ProofInput>,
    programs: Vec<ProviderProgram>,
    receipt: Value,
}
fn check_fast_launch(files: &BTreeMap<&str, String>, context: &Value) -> Result<()> {
    let ready = &files["ready"];
    let fields: Vec<_> = ready.trim_end_matches('\n').split(' ').collect();
    if !ready.ends_with('\n')
        || ready[..ready.len() - 1].contains('\n')
        || fields.len() != 13
        || fields[..2] != ["COLD560_LAUNCH", "1"]
        || files["approve"] != *ready
    {
        return Err("private launch ready/approval schema or exact binding".into());
    }
    let number = |at: usize| -> Result<u64> {
        let value = fields[at].parse::<u64>().map_err(|_| "launch integer")?;
        if value == 0 || value.to_string() != fields[at] {
            return Err("noncanonical launch identity".into());
        }
        Ok(value)
    };
    let pid = number(2)?;
    let start = number(3)?;
    let parent = number(6)?;
    let pstart = number(7)?;
    if pid > u64::from(u32::MAX)
        || parent > u64::from(u32::MAX)
        || pstart > start
        || fields[10] != context["source"].as_str().ok_or("source context")?
        || fields[11]
            != context["launcher_sha256"]
                .as_str()
                .ok_or("launcher context")?
        || fields[12]
            != context["provider_sha256"]
                .as_str()
                .ok_or("provider context")?
        || Some(number(4)?) != context["provider_device"].as_u64()
        || Some(number(5)?) != context["provider_inode"].as_u64()
        || Some(number(8)?) != context["timeout_device"].as_u64()
        || Some(number(9)?) != context["timeout_inode"].as_u64()
    {
        return Err("private launch actual source/provider/timeout identity".into());
    }
    let binding: Value = serde_json::from_str(&files["binding.json"]).map_err(|e| e.to_string())?;
    let leader = binding["owned_leader"].as_str().ok_or("launch leader")?;
    let leader_start = binding["owned_leader_start"]
        .as_str()
        .ok_or("launch leader start")?;
    let leader_number = leader.parse::<u32>().map_err(|_| "leader PID")?;
    let leader_ticks = leader_start.parse::<u64>().map_err(|_| "leader ticks")?;
    if leader_number==0 || leader_number.to_string()!=leader || leader_ticks==0
        || leader_ticks.to_string()!=leader_start || leader_ticks>pstart
        || binding != json!({"schema":1,"source":fields[10],"helper_sha256":fields[11],"provider_sha256":fields[12],
            "request":ready.trim_end_matches('\n'),"owned_leader":leader,"owned_leader_start":leader_start,
            "approval_fault":"none","actual_timeout_parent_and_exact_argv_verified":true,"parent_link_observed":false})
        || files["approval.stdout"]!=format!("COLD560_APPROVE tracer={pid} parent={parent} leader={leader} start={start} fault=none\n")
        || !files["approval.stderr"].is_empty()
    { return Err("private launch closed approval receipt".into()); }
    let inherited: Vec<_> = files["inherit.log"].lines().collect();
    if inherited.len() != 5
        || !files["inherit.log"].ends_with('\n')
        || inherited[0] != format!("COLD560_INHERIT start schema=1 tracer={pid} parent={parent}")
    {
        return Err("private inherited receipt start/EOF".into());
    }
    let root = inherited[1]
        .strip_prefix("COLD560_INHERIT root pid=")
        .ok_or("inherited root")?;
    let root_pid = root.parse::<u32>().map_err(|_| "root PID")?;
    let finish: Vec<_> = inherited[4].split(' ').collect();
    if root_pid == 0
        || root_pid.to_string() != root
        || inherited[2] != format!("COLD560_INHERIT root_seccomp pid={root}")
        || inherited[3] != format!("COLD560_INHERIT root_exit pid={root}")
        || finish.len() != 8
        || finish[..4]
            != [
                "COLD560_INHERIT",
                "finish",
                &format!("root={root}"),
                "root_seccomp=1",
            ]
        || finish[6..] != ["additional_ptrace=0", "parent_link_observed=0"]
    {
        return Err("private inherited root/strict completion".into());
    }
    let fresh = finish[4]
        .strip_prefix("fresh=")
        .ok_or("fresh count")?
        .parse::<u64>()
        .map_err(|_| "fresh integer")?;
    if finish[5] != format!("inherited={fresh}") || finish[4] != format!("fresh={fresh}") {
        return Err("private inherited filter count".into());
    }
    let actual = nul_arguments(&files["tracer.argv.nul"])?;
    let expected: Vec<&str> = context["argv"]
        .as_array()
        .ok_or("argv context")?
        .iter()
        .map(|arg| arg.as_str().ok_or_else(|| "argv string".to_owned()))
        .collect::<Result<_>>()?;
    if actual != expected {
        return Err("private tracer exact actual argv".into());
    }
    let timeout = nul_arguments(&files["timeout.argv.nul"])?;
    if timeout.len() < actual.len() + 8
        || timeout[..4]
            != [
                "/usr/bin/timeout",
                "--foreground",
                "--signal=TERM",
                "--kill-after=15s",
            ]
        || timeout[5..7] != ["/usr/bin/env", "-i"]
        || timeout[timeout.len() - actual.len()..] != actual
    {
        return Err("private timeout exact argv chain".into());
    }
    let seconds = timeout[4]
        .strip_suffix('s')
        .ok_or("timeout seconds")?
        .parse::<u32>()
        .map_err(|_| "timeout integer")?;
    if seconds == 0 || seconds > 420 || timeout[4] != format!("{seconds}s") {
        return Err("extended private launch deadline".into());
    }
    let mut environment = BTreeMap::new();
    for arg in &timeout[7..timeout.len() - actual.len()] {
        let (key, value) = arg.split_once('=').ok_or("launch environment assignment")?;
        if environment.insert(key, value).is_some() {
            return Err("duplicate launch environment".into());
        }
    }
    let expected_env = context["environment"]
        .as_object()
        .ok_or("environment context")?;
    if environment.len() != expected_env.len()
        || expected_env
            .iter()
            .any(|(key, value)| environment.get(key.as_str()).copied() != value.as_str())
    {
        return Err("launch does not use actual local root environments".into());
    }
    Ok(())
}
fn fast_provider(job: &Path, source: &Path, sdk: &Path, source_sha: &str) -> Result<FastProvider> {
    let proof = job.join("proof");
    let out = proof.join("fast-cold-560");
    let root = job.join("cold-strace-560");
    let preparation = proof.join("cold-strace-560");
    for path in [
        &out,
        &root,
        &preparation,
        &out.join("inheritance"),
        &out.join("inheritance/cold"),
        &root.join("bin"),
    ] {
        if owned(path, true)?.mode() & 0o777 != 0o700 {
            return Err("provider root is not private".into());
        }
    }
    let mut inputs = Vec::new();
    let mut read = |path| provider_input(&mut inputs, path);
    if out
        .join("first-failure")
        .try_exists()
        .map_err(|e| e.to_string())?
        || fs::symlink_metadata(out.join("first-failure")).is_ok()
        || read(out.join("cold.status"))?
            != "original_exit=0\nfinalizer_exit=0\nowned_groups_nonlive=true\n"
        || read(out.join("cold-complete"))? != format!("{source_sha}\n")
        || read(preparation.join("build-complete"))? != format!("{source_sha}\n")
        || read(preparation.join("verify-after-cold-complete"))? != format!("{source_sha}\n")
        || read(root.join("owner"))?
            != format!(
                "{}:{}:{}:{source_sha}\n",
                env("GITHUB_RUN_ID")?,
                env("GITHUB_RUN_ATTEMPT")?,
                env("GITHUB_JOB")?
            )
    {
        return Err("Fast provider failure/incomplete or foreign ownership".into());
    }
    for (name, bytes) in [
        (
            "fast_native_cold_560.sh",
            include_bytes!("../../../../../tools/ci/fast_native_cold_560.sh").as_slice(),
        ),
        (
            "cold_tracer_launch_560.sh",
            include_bytes!("../../../../../tools/ci/cold_tracer_launch_560.sh").as_slice(),
        ),
        (
            "prepare-cold-strace-560",
            include_bytes!("../../../../../tools/ci/prepare-cold-strace-560").as_slice(),
        ),
    ] {
        if sha(read(source.join("tools/ci").join(name))?.as_bytes()) != sha(bytes) {
            return Err("Fast implementation differs from compiled auditor source".into());
        }
    }
    for (before, after) in [
        (
            proof.join("index-before.bin"),
            out.join("index.after-cold.bin"),
        ),
        (
            out.join("stage.before.bin"),
            out.join("stage.after-cold.bin"),
        ),
        (
            proof.join("source-before.sha256"),
            out.join("source.after-cold.sha256"),
        ),
        (
            proof.join("prepared-rust-files.before.sha256"),
            out.join("sdk.after-cold.sha256"),
        ),
        (
            proof.join("prepared-rust-roster.before.bin"),
            out.join("sdk.after-cold.roster"),
        ),
    ] {
        if read(before)? != read(after)? {
            return Err("Fast source/index/SDK changed across cold".into());
        }
    }
    for name in ["sdk-original", "inputs", "source", "build", "bin"] {
        for kind in ["sha256", "roster"] {
            if read(preparation.join(format!("{name}.before.{kind}")))?
                != read(preparation.join(format!("{name}.after-cold.{kind}")))?
            {
                return Err("Fast full original SDK/provider seal differs".into());
            }
        }
    }
    let path = root.join("bin/strace");
    owned(&path, false)?;
    let program = ProviderProgram::read(&path)?;
    if read(out.join("provider.sha256"))? != format!("{}\n", program.sha256)
        || !read(preparation.join("bin.before.sha256"))?
            .lines()
            .any(|line| line == format!("{}  {}", program.sha256, path.display()))
    {
        return Err("Fast actual private provider SHA differs".into());
    }
    let timeout = ProviderProgram::read(Path::new("/usr/bin/timeout"))?;
    owned(&root.join("bin/approve-launch"), false)?;
    let fixture = ProviderProgram::read(&root.join("bin/approve-launch"))?;
    let bin_roster = read(preparation.join("bin.before.roster"))?;
    check_provider_roster(&bin_roster, &program)?;
    check_provider_roster(&bin_roster, &fixture)?;
    let before = read(out.join("cold-held.tsv"))?;
    let after = read(out.join("held-after.tsv"))?;
    let held = held_roster(&before)?;
    if held != held_roster(&after)? {
        return Err("Fast held before/after roster differs".into());
    }
    for program in [&program, &timeout, &fixture] {
        let (metadata, digest) = held
            .get(program.path.to_str().ok_or("provider UTF8 path")?)
            .ok_or("actual program missing from held roster")?;
        let stamp = &program.stamp;
        let prefix = format!(
            "{}:{}:{:x}:{}:{}:1:{}:",
            stamp.device, stamp.inode, stamp.mode, stamp.uid, stamp.gid, stamp.size
        );
        if *digest != program.sha256 || !metadata.starts_with(&prefix) {
            return Err("actual program differs from held launch provider".into());
        }
    }
    let mut files = BTreeMap::new();
    for name in [
        "ready",
        "approve",
        "binding.json",
        "inherit.log",
        "approval.stdout",
        "approval.stderr",
        "tracer.argv.nul",
        "timeout.argv.nul",
    ] {
        let path = out.join("inheritance/cold").join(name);
        let text = read(path.clone())?;
        if name != "approval.stderr"
            && held
                .get(path.to_str().ok_or("launch UTF8 path")?)
                .map(|entry| entry.1)
                != Some(sha(text.as_bytes()).as_str())
        {
            return Err("launch evidence differs from held cold snapshot".into());
        }
        files.insert(name, text);
    }
    let mut argv = vec![
        path.display().to_string(),
        "--debug".into(),
        "--seccomp-bpf".into(),
        "-f".into(),
        "-qq".into(),
        "-s".into(),
        "65535".into(),
        "-e".into(),
        "trace=execve,execveat".into(),
    ];
    for name in [
        "COLD560_LOG",
        "COLD560_FILTER_INHERIT",
        "COLD560_LAUNCH_DIR",
        "COLD560_SOURCE_SHA",
        "COLD560_HELPER_SHA",
        "COLD560_PROVIDER_SHA",
    ] {
        argv.extend(["-E".into(), name.into()]);
    }
    argv.extend([
        "-o".into(),
        proof
            .join("prepared-cold-bootstrap.exec")
            .display()
            .to_string(),
        source
            .join("rust/tools/check-disk-space")
            .display()
            .to_string(),
        "--path".into(),
        source.display().to_string(),
        "--growth-mib".into(),
        "2048".into(),
    ]);
    let launcher_sha = sha(include_bytes!(
        "../../../../../tools/ci/cold_tracer_launch_560.sh"
    ));
    let mut environment = BTreeMap::<String, String>::new();
    for (key, value) in [
        (
            "PATH",
            format!("{}:/usr/bin:/bin", sdk.join("bin").display()),
        ),
        ("HOME", job.join("home").display().to_string()),
        (
            "TMPDIR",
            job.join("fast-cold-560/tmp").display().to_string(),
        ),
        ("RUNNER_TEMP", job.display().to_string()),
        ("CARGO_HOME", job.join("cargo-home").display().to_string()),
        ("RUSTUP_HOME", job.join("rustup-home").display().to_string()),
        (
            "CARGO_TARGET_DIR",
            source.join("rust/target").display().to_string(),
        ),
        ("RUSTC", sdk.join("bin/rustc").display().to_string()),
        ("RUSTDOC", sdk.join("bin/rustdoc").display().to_string()),
        ("CI_JOB_TEMP", job.display().to_string()),
        ("GITHUB_WORKSPACE", source.display().to_string()),
        ("DIAG_SOURCE", source.display().to_string()),
        ("CI_PREPARED_RUST_ROOT", sdk.display().to_string()),
        (
            "CI_PREPARED_RUST_MANIFEST",
            proof
                .join("prepared-rust-files.before.sha256")
                .display()
                .to_string(),
        ),
        (
            "COLD560_LOG",
            out.join("observer.log").display().to_string(),
        ),
        (
            "COLD560_LAUNCH_DIR",
            out.join("inheritance/cold").display().to_string(),
        ),
        ("COLD560_SOURCE_SHA", source_sha.into()),
        ("COLD560_HELPER_SHA", launcher_sha.clone()),
        ("COLD560_PROVIDER_SHA", program.sha256.clone()),
        ("OPENPILOT_CI_PREPARED_RUST_ROOT", sdk.display().to_string()),
        (
            "OPENPILOT_CI_PREPARED_RUST_MANIFEST",
            proof
                .join("prepared-rust-files.before.sha256")
                .display()
                .to_string(),
        ),
    ] {
        environment.insert(key.into(), value);
    }
    for (key, value) in [
        ("LC_ALL", "C"),
        ("TZ", "UTC"),
        ("RUSTUP_TOOLCHAIN", "1.94.0"),
        ("CARGO_BUILD_JOBS", "1"),
        ("CARGO_INCREMENTAL", "0"),
        ("CARGO_PROFILE_DEV_DEBUG", "0"),
        ("CARGO_PROFILE_TEST_DEBUG", "0"),
        ("GITHUB_ACTIONS", "true"),
        ("GITHUB_REPOSITORY", "bin9208/openpilot-rust"),
        ("RUNNER_ENVIRONMENT", "self-hosted"),
        ("RUNNER_NAME", "bin-actions"),
        ("RUNNER_OS", "Linux"),
        ("RUNNER_ARCH", "X64"),
        ("SOURCE_ALLOWED", "true"),
        ("CARGO_TERM_COLOR", "never"),
        ("GIT_CONFIG_NOSYSTEM", "1"),
        ("GIT_CONFIG_GLOBAL", "/dev/null"),
        ("GIT_OPTIONAL_LOCKS", "0"),
        ("GIT_TERMINAL_PROMPT", "0"),
        ("GIT_NO_LAZY_FETCH", "1"),
        ("COLD560_FILTER_INHERIT", "1"),
    ] {
        environment.insert(key.into(), value.into());
    }
    for key in [
        "GITHUB_RUN_ID",
        "GITHUB_RUN_ATTEMPT",
        "GITHUB_JOB",
        "GITHUB_SHA",
        "GITHUB_EVENT_NAME",
        "DIAG_SOURCE_OWNER",
        "DIAG_SOURCE_IDENTITY",
        "DIAG_ACTIONS_WORKSPACE",
    ] {
        environment.insert(key.into(), env(key)?);
    }
    let context = json!({"source":source_sha,"provider_sha256":program.sha256,"launcher_sha256":launcher_sha,
        "provider_device":program.stamp.device,"provider_inode":program.stamp.inode,"timeout_device":timeout.stamp.device,"timeout_inode":timeout.stamp.inode,
        "argv":argv,"environment":environment});
    check_fast_launch(&files, &context)?;
    let observer = read(out.join("observer.log"))?;
    if held
        .get(
            out.join("observer.log")
                .to_str()
                .ok_or("observer UTF8 path")?,
        )
        .map(|entry| entry.1)
        != Some(sha(observer.as_bytes()).as_str())
    {
        return Err("observer differs from held cold snapshot".into());
    }
    Ok(FastProvider {
        inputs,
        programs: vec![program, timeout, fixture],
        receipt: json!({
        "mode":"private-patched-stopped-source-560-v1","provider":path,"provider_sha256":context["provider_sha256"],
        "launcher_sha256":launcher_sha,"launch_ready_sha256":sha(files["ready"].as_bytes()),"stock_binary_identity":false,
        "additional_ptrace_requests":0,"parent_link_observed":false}),
    })
}
fn actual_receipt() -> Result<Value> {
    for (name, expected) in [
        ("GITHUB_ACTIONS", "true"),
        ("GITHUB_REPOSITORY", "bin9208/openpilot-rust"),
        ("RUNNER_OS", "Linux"),
        ("RUNNER_ARCH", "X64"),
    ] {
        if env(name)? != expected {
            return Err(format!("unapproved actual-proof context: {name}"));
        }
    }
    let job = PathBuf::from(env("CI_JOB_TEMP")?);
    owned(&job, true)?;
    if job.parent() != Some(Path::new("/tmp"))
        || !job
            .file_name()
            .unwrap()
            .as_encoded_bytes()
            .starts_with(b"openpilot-ci.")
    {
        return Err("not an owned job-temp root".into());
    }
    let proof = PathBuf::from(env("CI_PREPARED_RUST_PROOF")?);
    if proof != job.join("proof") {
        return Err("proof is not this job's private proof directory".into());
    }
    owned(&proof, true)?;
    let owner = format!(
        "{}:{}:{}",
        env("GITHUB_RUN_ID")?,
        env("GITHUB_RUN_ATTEMPT")?,
        env("GITHUB_JOB")?
    );
    if read_owned(&job.join("owner"))? != owner {
        return Err("job owner token mismatch".into());
    }
    let output = proof.join("prepared-cold-bootstrap.json");
    invalidate_owned_receipt(&output)?;
    let sdk = actual_sdk(
        &job,
        &env("RUNNER_ENVIRONMENT")?,
        &env("RUNNER_NAME")?,
        &env("CI_PREPARED_RUST_ROOT")?,
        &env("RUSTUP_HOME")?,
        &env("CARGO_HOME")?,
    )?;
    let source_sha = env("GITHUB_SHA")?;
    if source_sha.len() != 40
        || !source_sha
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    {
        return Err("invalid source SHA".into());
    }
    if read_owned(&proof.join("SOURCE_COMMIT"))? != format!("{source_sha}\n") {
        return Err("SOURCE_COMMIT does not match GITHUB_SHA".into());
    }
    let ownership = read_owned(&proof.join("ownership.txt"))?;
    let mut fields = BTreeMap::new();
    for line in ownership.lines() {
        let (key, value) = line.split_once('=').ok_or("malformed ownership record")?;
        if fields.insert(key, value).is_some() {
            return Err("duplicate ownership record key".into());
        }
    }
    let source = super::root().canonicalize().map_err(|e| e.to_string())?;
    owned(&source, true)?;
    if fields.get("owner").copied() != Some(owner.as_str())
        || fields.get("job").copied() != Some(job.to_str().ok_or("non-UTF8 job")?)
        || fields.get("source").copied() != Some(source.to_str().ok_or("non-UTF8 source")?)
        || fields.get("workflow_sha").copied() != Some(source_sha.as_str())
    {
        return Err("actual source/job binding differs from ownership record".into());
    }
    let source_owner = PathBuf::from(*fields.get("source_owner").ok_or("missing source owner")?);
    let identity = owned(&source_owner, true)?;
    let source_identity = format!("{}:{}", identity.dev(), identity.ino());
    if source != source_owner.join("source")
        || read_owned(&source_owner.join(".diag296-owner"))? != format!("{owner}\n")
        || fields.get("source_identity").copied() != Some(source_identity.as_str())
    {
        return Err("private source owner identity mismatch".into());
    }
    owned(&sdk, true)?; // private SDK도 mode 775를 거부한다. permission 검사를 완화하지 않는다.
    for name in ["cargo", "rustc"] {
        let metadata = owned(&sdk.join("bin").join(name), false)?;
        if metadata.mode() & 0o111 == 0 {
            return Err("SDK program not executable".into());
        }
    }
    let manifest_path = PathBuf::from(env("CI_PREPARED_RUST_MANIFEST")?);
    if manifest_path != proof.join("prepared-rust-files.before.sha256") {
        return Err("unexpected SDK manifest location".into());
    }
    let manifest = ProofInput::read(&manifest_path)?;
    let manifest_files = check_manifest(&manifest.text, &sdk)?;
    let trace = ProofInput::read(&proof.join("prepared-cold-bootstrap.exec"))?;
    let audit = audit_trace_for_sdk(&trace.text, &sdk)?;
    let first = audit.calls.first().ok_or("empty exec audit")?;
    if !first.success || Path::new(&first.program) != source.join("rust/tools/check-disk-space") {
        return Err("trace does not begin with this source's successful bootstrap exec".into());
    }
    let provider = if requires_private_provider(&env("RUNNER_ENVIRONMENT")?, &env("GITHUB_JOB")?) {
        Some(fast_provider(&job, &source, &sdk, &source_sha)?)
    } else {
        None
    };
    let mut receipt = json!({
        "source_commit":source_sha, "absolute_cargo":sdk.join("bin/cargo"), "absolute_rustc":sdk.join("bin/rustc"),
        "runner_environment":env("RUNNER_ENVIRONMENT")?, "runner_name":env("RUNNER_NAME")?,
        "exec_attempts":audit.calls.len(), "observed_rustup_exec_attempts":0, "observed_python_exec_attempts":0,
        "successful_absolute_cargo_execs":audit.cargo_successes, "successful_absolute_rustc_execs":audit.rustc_successes,
        "failed_exec_attempts":audit.calls.iter().filter(|call| !call.success).count(),
        "trace_sha256":&trace.sha256, "manifest_sha256":&manifest.sha256, "manifest_regular_files_verified":manifest_files,
        "audit_schema":"openpilot.prepared-cold-bootstrap.native.v1"
    });
    if let Some(provider) = &provider {
        receipt["cold_provider"] = provider.receipt.clone();
    }
    let mut temporary = tempfile::NamedTempFile::new_in(&proof).map_err(|e| e.to_string())?;
    let mut bytes = serde_json::to_vec_pretty(&receipt).map_err(|e| e.to_string())?;
    bytes.push(b'\n');
    temporary.write_all(&bytes).map_err(|e| e.to_string())?;
    manifest.verify_unchanged()?;
    trace.verify_unchanged()?;
    if let Some(provider) = &provider {
        for input in &provider.inputs {
            input.verify_unchanged()?;
        }
        for program in &provider.programs {
            program.verify()?;
        }
    }
    temporary
        .persist_noclobber(&output)
        .map_err(|e| e.to_string())?;
    Ok(receipt)
}

fn synthetic_trace() -> String {
    format!("10 execve(\"{SDK}/bin/cargo\", [\"cargo\", \"build\"], 0x123 /* 3 vars */) = 0\n11 execve(\"/missing/cc\", [\"cc\"], NULL) = -1 ENOENT (No such file or directory)\n12 execve(\"{SDK}/bin/rustc\", [\"rustc\", \"--cfg=\\\"test\\\"\"], 0x456 /* 3 vars */ <unfinished ...>\n13 execve(\"/usr/bin/cc\", [\"cc\"], []) = 0\n12 <... execve resumed>) = 0\n10 --- SIGCHLD {{si_signo=SIGCHLD, si_code=CLD_EXITED, si_pid=12, si_uid=1000, si_status=0, si_utime=36 /* 0.36 s */, si_stime=0}} ---\n")
}
#[test]
fn prepared_audit_synthetic_counts_and_pid_resumption() {
    // parser 문법/집계만 검증한다. SDK file/manifest 또는 actual receipt를 만들지 않는다.
    let parsed = audit_trace(&synthetic_trace()).unwrap();
    assert_eq!(parsed.calls.len(), 4);
    assert_eq!((parsed.cargo_successes, parsed.rustc_successes), (1, 1));
    assert_eq!(parsed.calls.iter().filter(|c| !c.success).count(), 1);
    assert_eq!(
        audit_trace(&synthetic_trace().replace("13 execve(", "[pid 13] execve("))
            .unwrap()
            .calls
            .len(),
        4
    );
}
#[test]
fn prepared_audit_rejects_forbidden_attempts_including_failed_path_search() {
    for program in [
        "/missing/python3.12",
        "python",
        "/tmp/tool.PY",
        "/bin/rustup",
        r"/missing/\160ython3",
    ] {
        let trace = format!("{}20 execve(\"{program}\", [\"unused\"], NULL) = -1 ENOENT (No such file or directory)\n", synthetic_trace());
        assert!(
            audit_trace(&trace).unwrap_err().contains("forbidden"),
            "{program}"
        );
    }
}
#[test]
fn prepared_audit_requires_successful_absolute_compiler_and_cargo() {
    let trace = synthetic_trace();
    for changed in [
        trace.replace(&format!("{SDK}/bin/rustc"), "/usr/bin/rustc"),
        trace.replace(&format!("{SDK}/bin/rustc"), "rustc"),
        trace.replace(
            "12 <... execve resumed>) = 0",
            "12 <... execve resumed>) = -1 ENOENT (No such file or directory)",
        ),
        trace.replace(&format!("{SDK}/bin/cargo"), "/usr/bin/cargo"),
        trace.replacen(") = 0", ") = -1 EACCES (Permission denied)", 1),
    ] {
        assert!(audit_trace(&changed)
            .unwrap_err()
            .contains("missing successful absolute"));
    }
}
#[test]
fn prepared_audit_rejects_wrong_pid_or_unresolved_unfinished_calls() {
    let trace = synthetic_trace();
    for changed in [
        trace.replace("12 <... execve resumed>", "99 <... execve resumed>"),
        trace.replace("12 <... execve resumed>) = 0\n", ""),
        format!("{trace}99 <... execve resumed>) = 0\n"),
        trace.replace("13 execve(", "12 execve("),
        trace.replace(
            "12 <... execve resumed>) = 0",
            "12 +++ exited with 0 +++\n12 <... execve resumed>) = 0",
        ),
    ] {
        assert!(audit_trace(&changed).is_err(), "{changed}");
    }
}
#[test]
fn prepared_audit_rejects_fd_exec_malformed_and_truncated_data() {
    let trace = synthetic_trace();
    for suffix in [
        "20 execveat(AT_FDCWD, \"/usr/bin/true\", [], NULL, 0) = 0\n",
        "20 <... execveat resumed>) = 0\n",
        "20 execve(\"/usr/bin/true\", [\"true\", ...], NULL) = 0\n",
        "20 execve(\"/usr/bin/true, [], NULL) = 0\n",
        "20 execve(\"/usr/bin/true\", [], NULL) = ?\n",
        "20 not-a-strace-record\n",
        "pid execve(\"/usr/bin/true\", [], NULL) = 0\n",
        "20 --- SIGCHLD {si_signo=SIGCHLD, si_status=0 /* truncated} ---\n",
        "20 --- SIGCHLD {si_signo=SIGCHLD, si_status=0}} ---\n",
    ] {
        assert!(
            audit_trace(&format!("{trace}{suffix}")).is_err(),
            "{suffix}"
        );
    }
    assert!(audit_trace(trace.trim_end()).is_err());

    // 파일 admission 회귀도 합성 자료만 사용한다. 실제 SDK/receipt qualification은 아니다.
    let temporary = tempfile::tempdir().unwrap();
    let directory = temporary.path().canonicalize().unwrap();
    let mut input = tempfile::NamedTempFile::new_in(&directory).unwrap();
    input.write_all(b"before").unwrap();
    let snapshot = ProofInput::read(input.path()).unwrap();
    fs::write(input.path(), b"after!").unwrap();
    assert!(snapshot.verify_unchanged().is_err());
    let mut replacement = tempfile::NamedTempFile::new_in(&directory).unwrap();
    replacement.write_all(b"before").unwrap();
    replacement.persist(input.path()).unwrap();
    assert_eq!(
        ProofInput::read(input.path()).unwrap().sha256,
        snapshot.sha256
    );
    assert!(snapshot.verify_unchanged().is_err()); // 같은 bytes여도 inode 교체는 거부한다.

    let output = directory.join("prepared-cold-bootstrap.json");
    fs::copy(input.path(), &output).unwrap();
    invalidate_owned_receipt(&output).unwrap();
    assert!(!output.exists());
    invalidate_owned_receipt(&output).unwrap(); // 처음부터 receipt가 없어도 정상이다.
    std::os::unix::fs::symlink(input.path(), &output).unwrap();
    assert!(invalidate_owned_receipt(&output).is_err());
    assert!(fs::symlink_metadata(&output)
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(fs::read(input.path()).unwrap(), b"before"); // 외부 대상은 보존한다.
    let nonregular = directory.join("nonregular-receipt");
    fs::create_dir(&nonregular).unwrap();
    assert!(invalidate_owned_receipt(&nonregular).is_err());
    assert!(nonregular.is_dir());
}
#[test]
fn prepared_audit_c_strings_have_an_explicit_fail_closed_ascii_scope() {
    let trace = synthetic_trace();
    assert!(audit_trace(&trace.replace(SDK, &SDK.replacen('/', r"\057", 1))).is_ok());
    assert!(audit_trace(&trace.replace(
        "[\"cc\"], []",
        r#"["cc", "literal ... and <unfinished ...>", "a\\b\tc"], []"#
    ))
    .is_ok());
    for program in [
        r"/tmp/\x70ython",
        r"/tmp/\u0070ython",
        r"/tmp/\q",
        r"/tmp/\12",
        r"/tmp/\377",
        r"/tmp/\000",
        "/tmp/é",
    ] {
        assert!(
            audit_trace(&format!("{trace}20 execve(\"{program}\", [], NULL) = 0\n")).is_err(),
            "{program}"
        );
    }
}
#[test]
#[ignore = "실제 승인된 Actions cold strace와 소유 private source/SDK manifest 필요; 합성 자료 qualification 금지"]
fn prepared_audit_actual_cold_bootstrap_receipt() {
    let receipt = actual_receipt()
        .unwrap_or_else(|error| panic!("actual prepared SDK audit rejected: {error}"));
    println!("PREPARED_COLD_BOOTSTRAP_AUDIT_JSON {receipt}");
}

#[test]
fn prepared_audit_hosted_sdk_rejects_legacy_other_job_and_tool_homes() {
    let job = Path::new("/tmp/openpilot-ci.abcdef");
    let sdk = "/tmp/openpilot-ci.abcdef/rustup-home/toolchains/1.94.0-x86_64-unknown-linux-gnu";
    let rustup = "/tmp/openpilot-ci.abcdef/rustup-home";
    let cargo = "/tmp/openpilot-ci.abcdef/cargo-home";
    assert_eq!(
        private_hosted_sdk(job, sdk, rustup, cargo).unwrap(),
        Path::new(sdk)
    );
    for wrong in [
        SDK.to_owned(),
        sdk.replace("abcdef", "uvwxyz"),
        format!("{sdk}/"),
        sdk.replace("1.94.0", "1.93.0"),
    ] {
        assert!(
            private_hosted_sdk(job, &wrong, rustup, cargo).is_err(),
            "{wrong}"
        );
    }
    assert!(private_hosted_sdk(job, sdk, "/tmp/other/rustup-home", cargo).is_err());
    assert!(private_hosted_sdk(job, sdk, rustup, "/tmp/other/cargo-home").is_err());
    let trace = synthetic_trace().replace(SDK, sdk);
    assert!(audit_trace_for_sdk(&trace, Path::new(sdk)).is_ok());
    assert!(audit_trace_for_sdk(&synthetic_trace(), Path::new(sdk)).is_err());
    assert!(audit_trace_for_sdk(&trace.replace("abcdef", "uvwxyz"), Path::new(sdk)).is_err());
}

#[test]
fn prepared_audit_admits_only_registered_local_sdk_and_private_tool_homes() {
    let job = Path::new("/tmp/openpilot-ci.ABC123");
    let sdk = "/home/actions/.rustup/toolchains/1.94.0-x86_64-unknown-linux-gnu";
    let rustup = "/tmp/openpilot-ci.ABC123/rustup-home";
    let cargo = "/tmp/openpilot-ci.ABC123/cargo-home";
    assert_eq!(
        actual_sdk(job, "self-hosted", "bin-actions", sdk, rustup, cargo).unwrap(),
        Path::new(sdk)
    );
    for (environment, runner, sdk, rustup, cargo) in [
        ("self-hosted", "other", sdk, rustup, cargo),
        ("unknown", "bin-actions", sdk, rustup, cargo),
        ("github-hosted", "bin-actions", sdk, rustup, cargo),
        (
            "self-hosted",
            "bin-actions",
            "/tmp/openpilot-ci.ABC123/rustup-home/toolchains/1.94.0-x86_64-unknown-linux-gnu",
            rustup,
            cargo,
        ),
        (
            "self-hosted",
            "bin-actions",
            "/home/actions/.rustup/toolchains/stable-x86_64-unknown-linux-gnu",
            rustup,
            cargo,
        ),
        (
            "self-hosted",
            "bin-actions",
            sdk,
            "/home/actions/.rustup",
            cargo,
        ),
        (
            "self-hosted",
            "bin-actions",
            sdk,
            rustup,
            "/home/actions/.cargo",
        ),
    ] {
        assert!(actual_sdk(job, environment, runner, sdk, rustup, cargo).is_err());
    }
}

fn synthetic_private_launch() -> (BTreeMap<&'static str, String>, Value) {
    // 독립적인 문서형 protocol 자료다. 실제 provider/cold qualification이 아니다.
    let source = "a".repeat(40);
    let helper = "b".repeat(64);
    let provider = "c".repeat(64);
    let ready =
        format!("COLD560_LAUNCH 1 103 300 1 20 102 200 1 30 {source} {helper} {provider}\n");
    let binding = json!({"schema":1,"source":source,"helper_sha256":helper,"provider_sha256":provider,
        "request":ready.trim_end_matches('\n'),"owned_leader":"101","owned_leader_start":"100",
        "approval_fault":"none","actual_timeout_parent_and_exact_argv_verified":true,"parent_link_observed":false});
    let files=BTreeMap::from([
        ("ready",ready.clone()),("approve",ready),("binding.json",binding.to_string()),
        ("approval.stdout","COLD560_APPROVE tracer=103 parent=102 leader=101 start=300 fault=none\n".into()),
        ("approval.stderr",String::new()),
        ("inherit.log",concat!("COLD560_INHERIT start schema=1 tracer=103 parent=102\n",
            "COLD560_INHERIT root pid=104\n","COLD560_INHERIT root_seccomp pid=104\n",
            "COLD560_INHERIT root_exit pid=104\n",
            "COLD560_INHERIT finish root=104 root_seccomp=1 fresh=2 inherited=2 additional_ptrace=0 parent_link_observed=0\n").into()),
        ("tracer.argv.nul","/private/strace\0-o\0/private/cold.exec\0/source/rust/tools/check-disk-space\0".into()),
        ("timeout.argv.nul",concat!("/usr/bin/timeout\0--foreground\0--signal=TERM\0--kill-after=15s\090s\0/usr/bin/env\0-i\0",
            "RUNNER_ENVIRONMENT=self-hosted\0CARGO_BUILD_JOBS=1\0/private/strace\0-o\0/private/cold.exec\0/source/rust/tools/check-disk-space\0").into()),
    ]);
    let context = json!({"source":source,"launcher_sha256":helper,"provider_sha256":provider,
        "provider_device":1,"provider_inode":20,"timeout_device":1,"timeout_inode":30,
        "argv":["/private/strace","-o","/private/cold.exec","/source/rust/tools/check-disk-space"],
        "environment":{"RUNNER_ENVIRONMENT":"self-hosted","CARGO_BUILD_JOBS":"1"}});
    (files, context)
}
#[test]
fn prepared_audit_private_provider_requires_closed_launch_and_actual_context() {
    assert!(requires_private_provider("self-hosted", "native"));
    assert!(!requires_private_provider("github-hosted", "native"));
    assert!(!requires_private_provider("github-hosted", "cold-tracer"));
    let (files, context) = synthetic_private_launch();
    check_fast_launch(&files, &context).unwrap();
    for (name, from, to) in [
        ("approve", "103 300", "103 301"),
        ("ready", "103 300", "103 301"),
        ("approval.stdout", "parent=102", "parent=99"),
        ("approval.stderr", "", "unexpected stderr"),
        ("inherit.log", "fresh=2 inherited=2", "fresh=3 inherited=2"),
        ("inherit.log", "root_seccomp=1", "root_seccomp=0"),
        ("inherit.log", "additional_ptrace=0", "additional_ptrace=1"),
        (
            "inherit.log",
            "parent_link_observed=0",
            "parent_link_observed=1",
        ),
        ("tracer.argv.nul", "/private/strace", "/usr/bin/strace"),
        ("timeout.argv.nul", "90s", "421s"),
        ("timeout.argv.nul", "self-hosted", "github-hosted"),
        (
            "timeout.argv.nul",
            "CARGO_BUILD_JOBS=1",
            "CARGO_BUILD_JOBS=2",
        ),
        (
            "timeout.argv.nul",
            "CARGO_BUILD_JOBS=1\0",
            "CARGO_BUILD_JOBS=1\0CARGO_BUILD_JOBS=1\0",
        ),
    ] {
        let mut changed = files.clone();
        changed.insert(name, changed[name].replace(from, to));
        assert!(
            check_fast_launch(&changed, &context).is_err(),
            "{name}: {from} -> {to}"
        );
    }
    for field in [
        "source",
        "launcher_sha256",
        "provider_sha256",
        "provider_inode",
        "timeout_inode",
    ] {
        let mut changed = context.clone();
        changed[field] = json!("foreign");
        assert!(check_fast_launch(&files, &changed).is_err(), "{field}");
    }
    let mut changed = files.clone();
    changed
        .get_mut("inherit.log")
        .unwrap()
        .push_str("unknown\n");
    assert!(check_fast_launch(&changed, &context).is_err());
    for malformed in ["no-nul", "one\0\0", "é\0"] {
        assert!(nul_arguments(malformed).is_err());
    }
}
#[test]
fn prepared_audit_private_provider_held_roster_rejects_replacement_or_ambiguity() {
    let hash = "a".repeat(64);
    let roster = format!("/private/strace\t1:20:81c0:1000:1000:1:64:mtime:ctime\t{hash}\n");
    assert_eq!(held_roster(&roster).unwrap().len(), 1);
    for wrong in [
        format!("{roster}{roster}"),
        roster.replace(&hash, "bad"),
        roster.replace("/private/strace", "relative"),
        roster.trim_end().into(),
    ] {
        assert!(held_roster(&wrong).is_err());
    }
    assert_ne!(
        held_roster(&roster).unwrap(),
        held_roster(&roster.replace(":20:", ":21:")).unwrap()
    );
    // #560의 미해석 raw line은 여전히 parser에서 거부한다. provider admission으로 숨기지 않는다.
    assert!(audit_trace(&format!("{}19116 ???( <detached ...>\n", synthetic_trace())).is_err());
}

#[test]
fn prepared_audit_private_provider_seals_actual_elf_and_rejects_inode_replacement() {
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("provider");
    fs::copy(std::env::current_exe().unwrap(), &path).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
    let program = ProviderProgram::read(&path).unwrap();
    program.verify().unwrap();
    let stamp = &program.stamp;
    let roster = format!(
        "{}:{}\t700:{}:{}:1\t{}\t{}.{:09}0\t{}.{:09}0\tf\t{}\t\0",
        stamp.device,
        stamp.inode,
        stamp.uid,
        stamp.gid,
        stamp.size,
        stamp.mtime.0,
        stamp.mtime.1,
        stamp.ctime.0,
        stamp.ctime.1,
        path.display()
    );
    check_provider_roster(&roster, &program).unwrap();
    assert!(check_provider_roster(&roster.replace("\t700:", "\t755:"), &program).is_err());
    let replacement = directory.path().join("replacement");
    fs::copy(&path, &replacement).unwrap();
    fs::set_permissions(&replacement, fs::Permissions::from_mode(0o700)).unwrap();
    fs::rename(replacement, &path).unwrap();
    assert_eq!(ProviderProgram::read(&path).unwrap().sha256, program.sha256);
    assert!(program.verify().is_err());
    let link = directory.path().join("link");
    std::os::unix::fs::symlink(&path, &link).unwrap();
    assert!(ProviderProgram::read(&link).is_err());
    fs::write(&path, b"a shell script cannot impersonate the provider").unwrap();
    assert!(ProviderProgram::read(&path).is_err());
}
