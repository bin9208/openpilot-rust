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
fn actual_receipt() -> Result<Value> {
    for (name, expected) in [
        ("GITHUB_ACTIONS", "true"),
        ("GITHUB_REPOSITORY", "bin9208/openpilot-rust"),
        ("RUNNER_ENVIRONMENT", "github-hosted"),
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
    let sdk = private_hosted_sdk(
        &job,
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
    let receipt = json!({
        "source_commit":source_sha, "absolute_cargo":sdk.join("bin/cargo"), "absolute_rustc":sdk.join("bin/rustc"),
        "runner_environment":"github-hosted", "runner_name":env("RUNNER_NAME")?,
        "exec_attempts":audit.calls.len(), "observed_rustup_exec_attempts":0, "observed_python_exec_attempts":0,
        "successful_absolute_cargo_execs":audit.cargo_successes, "successful_absolute_rustc_execs":audit.rustc_successes,
        "failed_exec_attempts":audit.calls.iter().filter(|call| !call.success).count(),
        "trace_sha256":&trace.sha256, "manifest_sha256":&manifest.sha256, "manifest_regular_files_verified":manifest_files,
        "audit_schema":"openpilot.prepared-cold-bootstrap.native.v1"
    });
    let mut temporary = tempfile::NamedTempFile::new_in(&proof).map_err(|e| e.to_string())?;
    let mut bytes = serde_json::to_vec_pretty(&receipt).map_err(|e| e.to_string())?;
    bytes.push(b'\n');
    temporary.write_all(&bytes).map_err(|e| e.to_string())?;
    manifest.verify_unchanged()?;
    trace.verify_unchanged()?;
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
#[ignore = "실제 github-hosted cold strace와 소유 private source/SDK manifest 필요; 합성 자료 qualification 금지"]
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
