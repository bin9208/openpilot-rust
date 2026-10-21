//! Fail-closed execution admission for the frozen recovery controller family.
//!
//! This module never derives an allowlist from a trace. Callers supply exact
//! program, argv, result, and count rules from source-authored fixtures, and
//! independent before/after executable seals. A diagnostic partial trace is
//! not a valid policy input.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsStr,
    fs::{self, File},
    io::{Read, Seek, SeekFrom, Write},
    os::unix::{
        ffi::OsStrExt,
        fs::{MetadataExt, PermissionsExt},
    },
    path::{Component, Path, PathBuf},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

const MAX_TRACE_FILES: usize = 8192;
const MAX_TRACE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 128 * 1024 * 1024;
const MAX_LINE_BYTES: usize = 128 * 1024;
const MAX_ATTEMPTS: usize = 100_000;
const MAX_ARGV: usize = 128;
const MAX_STRING_BYTES: usize = 32_768;
const MAX_PROGRAM_BYTES: u64 = 256 * 1024 * 1024;
const ACTION_SCENARIOS: [&str; 24] = [
    "git_branches",
    "git_log",
    "git_reset",
    "git_sync",
    "git_pull",
    "git_rebuild",
    "rebuild_all",
    "git_pull_repair",
    "git_pull_preserve",
    "git_pull_multiple",
    "git_pull_missing",
    "git_checkout",
    "git_checkout_local",
    "git_checkout_commit",
    "git_reset_repo",
    "quoted_branch",
    "stale_index",
    "fresh_index",
    "symlink_index",
    "busy_lock",
    "unknown",
    "missing_branch",
    "false_branch",
    "number_commit",
];

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub enum Outcome {
    Success,
    Errno(String),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Attempt {
    pub pid: u32,
    pub line: usize,
    pub program: Vec<u8>,
    pub argv: Vec<Vec<u8>>,
    pub outcome: Outcome,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LinkSeal {
    pub path: Vec<u8>,
    pub target: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProgramSeal {
    pub requested: Vec<u8>,
    pub resolved: Vec<u8>,
    pub links: Vec<LinkSeal>,
    pub dev: u64,
    pub ino: u64,
    pub len: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum CreatedProgramKind {
    /// The existing 52-row owned-alias receipt begins with `created` and
    /// precedes every action or PTY invocation.
    Alias { source: Vec<u8> },
    /// A controller-created symlink to a pre-harness native provider. Its
    /// creation inode and exact target are recorded before the first launch.
    SymlinkAlias { source: Vec<u8>, receipt: PathBuf },
    /// The writer's held-file receipt is emitted after the copy and before
    /// its first execution; `source` was sealed before the harness.
    CopiedElf { source: Vec<u8>, receipt: PathBuf },
    /// Exact bytes come from a source-authored literal, never a trace or a
    /// candidate-generated output. The writer must hold the file descriptor.
    AuthoredScript {
        bytes: Vec<u8>,
        mode: u32,
        receipt: PathBuf,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreatedProgramRule {
    pub final_seal: ProgramSeal,
    pub kind: CreatedProgramKind,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ExpectedAttempt {
    pub program: Vec<u8>,
    pub argv: Vec<Vec<u8>>,
    pub outcome: Outcome,
    pub count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum TypedArg {
    Exact(Vec<u8>),
    /// `tempfile::Builder::prefix("op-exec-")` and `NamedTempFile` in
    /// process-supervision. This is an argv path, never an executable seal.
    LaunchDescriptor,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ExpectedTypedAttempt {
    pub program: Vec<u8>,
    pub argv: Vec<TypedArg>,
    pub outcome: Outcome,
    pub count: usize,
}

/// Narrower executable-provenance admission. Raw argv is retained in the
/// audit receipt but is not claimed to equal an independent argv oracle.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ExpectedProgram {
    pub program: Vec<u8>,
    pub outcome: Outcome,
    pub minimum: usize,
    pub maximum: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AliasRelation {
    pub alias: Vec<u8>,
    pub source: Vec<u8>,
    pub same_inode: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScriptInterpreter {
    pub script: Vec<u8>,
    pub exact_shebang: Vec<u8>,
    pub interpreter: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OwnedAliasReceiptRule {
    pub path: PathBuf,
    pub candidate_source: Vec<u8>,
    pub candidate_alias: Vec<u8>,
    pub child_source: Vec<u8>,
    pub child_alias: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExplicitPolicy {
    pub expected: Vec<ExpectedAttempt>,
    pub typed_expected: Vec<ExpectedTypedAttempt>,
    pub allowed_programs: Vec<ExpectedProgram>,
    pub program_seals: Vec<(ProgramSeal, ProgramSeal)>,
    pub created_programs: Vec<CreatedProgramRule>,
    /// Independent source/provider digest, never computed from this trace.
    pub expected_program_sha256: Vec<(Vec<u8>, String)>,
    pub alias_relations: Vec<AliasRelation>,
    pub scripts: Vec<ScriptInterpreter>,
    pub alias_receipts: Vec<OwnedAliasReceiptRule>,
    pub admission_path: PathBuf,
    pub same_elf_digests: Vec<String>,
    pub historical_sources: usize,
    pub harness_exit_code: i32,
    pub harness_program: Vec<u8>,
    pub harness_argv: Vec<Vec<u8>>,
}

#[derive(Debug, Serialize)]
pub struct AuditReceipt {
    pub passed: bool,
    pub argv_scope: String,
    pub attempts: Vec<Attempt>,
    pub successful: usize,
    pub failed: usize,
    pub program_counts: Vec<(ExpectedProgram, usize)>,
    pub program_seals: Vec<(ProgramSeal, ProgramSeal)>,
    pub created_programs: Vec<CreatedProgramRule>,
    pub alias_relations: Vec<AliasRelation>,
    pub scripts: Vec<ScriptInterpreter>,
    pub alias_receipts: Vec<OwnedAliasReceiptRule>,
    pub admission_sha256: String,
    pub trace_files: Vec<TraceFileSeal>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TraceFileSeal {
    pub pid: u32,
    pub path: PathBuf,
    pub dev: u64,
    pub ino: u64,
    pub len: u64,
    pub sha256: String,
}

struct HeldTraceFile {
    seal: TraceFileSeal,
    file: File,
}

pub struct TraceCorpus {
    directory: PathBuf,
    files: Vec<HeldTraceFile>,
    attempts: Vec<Attempt>,
}

/// A program authored by the fixture after the pre-harness seal. The first
/// row is flushed before it can be launched. Drop writes the final row only
/// when the held descriptor and its original path still agree; an incomplete
/// receipt is rejected by `audit` even if a controller forgets a final check.
pub struct CreatedProgramGuard {
    path: PathBuf,
    held: File,
    receipt_path: PathBuf,
    receipt: File,
    seal: ProgramSeal,
    mode: u32,
    finished: bool,
}

/// Hold both the authored symlink inode and its pre-existing ELF target. A
/// separate receipt makes the creation boundary visible before any launch.
pub struct CreatedSymlinkGuard {
    path: PathBuf,
    target: PathBuf,
    source: File,
    source_seal: ProgramSeal,
    link: File,
    link_dev: u64,
    link_ino: u64,
    receipt_path: PathBuf,
    receipt: File,
    finished: bool,
}

impl CreatedSymlinkGuard {
    pub fn record(path: &Path, target: &Path, receipt_path: &Path) -> Result<Self> {
        let source = File::from(rustix::fs::open(
            target,
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::NONBLOCK
                | rustix::fs::OFlags::NOFOLLOW
                | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )?);
        let source_seal = capture_program_seal(target)?;
        let source_meta = source.metadata()?;
        let link = File::from(rustix::fs::open(
            path,
            rustix::fs::OFlags::PATH | rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )?);
        let link_meta = link.metadata()?;
        let named_link = fs::symlink_metadata(path)?;
        if !source_meta.is_file()
            || !link_meta.file_type().is_symlink()
            || !named_link.file_type().is_symlink()
            || (named_link.dev(), named_link.ino()) != (link_meta.dev(), link_meta.ino())
            || fs::read_link(path)? != target
            || source_seal.requested != target.as_os_str().as_bytes()
            || (source_meta.dev(), source_meta.ino(), source_meta.len())
                != (source_seal.dev, source_seal.ino, source_seal.len)
            || digest_held(&source, MAX_PROGRAM_BYTES, source_seal.len)? != source_seal.sha256
        {
            return Err(invalid(
                "created symlink did not bind sealed native provider",
            ));
        }
        let mut receipt = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(receipt_path)?;
        Self::write_row(
            &mut receipt,
            "created",
            path,
            target,
            &source_seal,
            &link_meta,
        )?;
        Ok(Self {
            path: path.to_owned(),
            target: target.to_owned(),
            source,
            source_seal,
            link,
            link_dev: link_meta.dev(),
            link_ino: link_meta.ino(),
            receipt_path: receipt_path.to_owned(),
            receipt,
            finished: false,
        })
    }

    fn write_row(
        file: &mut File,
        stage: &str,
        path: &Path,
        target: &Path,
        source: &ProgramSeal,
        link: &fs::Metadata,
    ) -> Result<()> {
        serde_json::to_writer(
            &mut *file,
            &json!({"stage":stage,"alias":path.as_os_str().as_bytes(),
                "target":target.as_os_str().as_bytes(),"link_dev":link.dev(),
                "link_ino":link.ino(),"source_sha256":source.sha256}),
        )?;
        file.write_all(b"\n")?;
        file.flush()?;
        Ok(())
    }

    pub fn finish(mut self) -> Result<()> {
        self.finished = true;
        self.verify_and_finish()
    }

    fn verify_and_finish(&mut self) -> Result<()> {
        let link = self.link.metadata()?;
        let named_link = fs::symlink_metadata(&self.path)?;
        let source = self.source.metadata()?;
        let receipt = self.receipt.metadata()?;
        let named_receipt = fs::symlink_metadata(&self.receipt_path)?;
        if !link.file_type().is_symlink()
            || !named_link.file_type().is_symlink()
            || (link.dev(), link.ino()) != (self.link_dev, self.link_ino)
            || (named_link.dev(), named_link.ino()) != (link.dev(), link.ino())
            || fs::read_link(&self.path)? != self.target
            || (source.dev(), source.ino(), source.len())
                != (
                    self.source_seal.dev,
                    self.source_seal.ino,
                    self.source_seal.len,
                )
            || digest_held(&self.source, MAX_PROGRAM_BYTES, self.source_seal.len)?
                != self.source_seal.sha256
            || capture_program_seal(&self.target)? != self.source_seal
            || !named_receipt.is_file()
            || (receipt.dev(), receipt.ino()) != (named_receipt.dev(), named_receipt.ino())
        {
            return Err(invalid(
                "created provider symlink changed before finalization",
            ));
        }
        Self::write_row(
            &mut self.receipt,
            "after-family",
            &self.path,
            &self.target,
            &self.source_seal,
            &link,
        )
    }
}

impl Drop for CreatedSymlinkGuard {
    fn drop(&mut self) {
        if !self.finished && self.verify_and_finish().is_ok() {
            self.finished = true;
        }
    }
}

impl CreatedProgramGuard {
    pub fn record(path: &Path, receipt_path: &Path) -> Result<Self> {
        let held = File::from(rustix::fs::open(
            path,
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::NONBLOCK
                | rustix::fs::OFlags::NOFOLLOW
                | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )?);
        let metadata = held.metadata()?;
        if !metadata.is_file() || metadata.len() > MAX_PROGRAM_BYTES {
            return Err(invalid("created executable is not bounded regular file"));
        }
        let seal = capture_program_seal(path)?;
        if !seal.links.is_empty()
            || (metadata.dev(), metadata.ino(), metadata.len()) != (seal.dev, seal.ino, seal.len)
            || digest_held(&held, MAX_PROGRAM_BYTES, seal.len)? != seal.sha256
        {
            return Err(invalid("created executable path changed while binding"));
        }
        let mode = metadata.permissions().mode() & 0o777;
        let mut receipt = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(receipt_path)?;
        Self::write_row(&mut receipt, "created", &seal, mode)?;
        Ok(Self {
            path: path.to_owned(),
            held,
            receipt_path: receipt_path.to_owned(),
            receipt,
            seal,
            mode,
            finished: false,
        })
    }

    fn write_row(file: &mut File, stage: &str, seal: &ProgramSeal, mode: u32) -> Result<()> {
        let path = std::str::from_utf8(&seal.requested)?;
        serde_json::to_writer(
            &mut *file,
            &json!({"stage":stage,"path":path,"dev":seal.dev,"ino":seal.ino,
                "len":seal.len,"sha256":seal.sha256,"mode":mode}),
        )?;
        file.write_all(b"\n")?;
        file.flush()?;
        Ok(())
    }

    pub fn finish(mut self) -> Result<()> {
        self.finished = true;
        self.verify_and_finish()
    }

    fn verify_and_finish(&mut self) -> Result<()> {
        let held = self.held.metadata()?;
        let receipt_held = self.receipt.metadata()?;
        let receipt_named = fs::symlink_metadata(&self.receipt_path)?;
        if !receipt_named.is_file()
            || (receipt_held.dev(), receipt_held.ino())
                != (receipt_named.dev(), receipt_named.ino())
            || (held.dev(), held.ino(), held.len()) != (self.seal.dev, self.seal.ino, self.seal.len)
            || held.permissions().mode() & 0o777 != self.mode
            || capture_program_seal(&self.path)? != self.seal
        {
            return Err(invalid(
                "created executable or receipt changed before finalization",
            ));
        }
        let digest = digest_held(&self.held, MAX_PROGRAM_BYTES, self.seal.len)?;
        if digest != self.seal.sha256 {
            return Err(invalid("held created executable contents changed"));
        }
        Self::write_row(&mut self.receipt, "after-family", &self.seal, self.mode)
    }
}

impl Drop for CreatedProgramGuard {
    fn drop(&mut self) {
        if !self.finished && self.verify_and_finish().is_ok() {
            self.finished = true;
        }
    }
}

fn digest_held(file: &File, maximum: u64, expected_len: u64) -> Result<String> {
    let mut clone = file.try_clone()?;
    clone.rewind()?;
    let mut limited = clone.take(maximum + 1);
    let mut hash = Sha256::new();
    let mut length = 0_u64;
    let mut chunk = [0_u8; 65_536];
    loop {
        let count = limited.read(&mut chunk)?;
        if count == 0 {
            break;
        }
        length += count as u64;
        hash.update(&chunk[..count]);
    }
    if length != expected_len || length > maximum {
        return Err(invalid("held executable length changed"));
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn invalid(message: &str) -> Box<dyn std::error::Error> {
    message.to_owned().into()
}

fn hex_nibble(byte: u8) -> Result<u8> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(invalid("nonhex strace byte escape")),
    }
}

fn escaped_string(input: &mut &[u8]) -> Result<Vec<u8>> {
    if !input.starts_with(b"\"") {
        return Err(invalid("missing strace quoted string"));
    }
    *input = &input[1..];
    let mut decoded = Vec::new();
    loop {
        if input.starts_with(b"\"") {
            *input = &input[1..];
            return Ok(decoded);
        }
        if input.len() < 4 || !input.starts_with(b"\\x") {
            return Err(invalid("undecoded or truncated -xx string"));
        }
        let byte = (hex_nibble(input[2])? << 4) | hex_nibble(input[3])?;
        if byte == 0 || decoded.len() == MAX_STRING_BYTES {
            return Err(invalid("invalid or oversized strace string"));
        }
        decoded.push(byte);
        *input = &input[4..];
    }
}

fn argv(input: &mut &[u8]) -> Result<Vec<Vec<u8>>> {
    if !input.starts_with(b"[") {
        return Err(invalid("missing strace argv"));
    }
    *input = &input[1..];
    let mut args = Vec::new();
    loop {
        if input.starts_with(b"]") {
            *input = &input[1..];
            if args.is_empty() {
                return Err(invalid("empty exec argv"));
            }
            return Ok(args);
        }
        if args.len() == MAX_ARGV {
            return Err(invalid("oversized exec argv"));
        }
        args.push(escaped_string(input)?);
        if input.starts_with(b", ") {
            *input = &input[2..];
        } else if !input.starts_with(b"]") {
            return Err(invalid("opaque strace argv separator"));
        }
    }
}

fn environment_shape(text: &str) -> Result<()> {
    let Some(pointer) = text.strip_prefix("0x") else {
        return Err(invalid("opaque strace environment"));
    };
    let (address, comment) = pointer.split_once(" /* ").unwrap_or((pointer, ""));
    if address.is_empty() || !address.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(invalid("invalid strace environment pointer"));
    }
    if !comment.is_empty() {
        let Some(number) = comment.strip_suffix(" vars */") else {
            return Err(invalid("opaque strace environment count"));
        };
        if number.is_empty() || !number.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(invalid("invalid strace environment count"));
        }
    }
    Ok(())
}

fn outcome(text: &str) -> Result<Outcome> {
    if text == "0" {
        return Ok(Outcome::Success);
    }
    let Some(rest) = text.strip_prefix("-1 ") else {
        return Err(invalid("unknown execve result"));
    };
    let Some((errno, description)) = rest.split_once(" (") else {
        return Err(invalid("missing execve errno description"));
    };
    if !description.ends_with(')')
        || errno.is_empty()
        || !errno
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
    {
        return Err(invalid("invalid execve errno"));
    }
    Ok(Outcome::Errno(errno.to_owned()))
}

enum ParsedLine {
    Start(Vec<u8>, Vec<Vec<u8>>),
    Complete(Vec<u8>, Vec<Vec<u8>>, Outcome),
    Resume(Outcome),
    Marker,
}

fn line(input: &[u8]) -> Result<ParsedLine> {
    let text = std::str::from_utf8(input)?;
    if text.starts_with("execveat(") || text.starts_with("<... execveat resumed>") {
        return Err(invalid("execveat is forbidden"));
    }
    if text.starts_with("--- SIG") && text.contains(" {si_signo=") && text.ends_with("} ---")
        || text.starts_with("+++ exited with ") && text.ends_with(" +++")
        || text.starts_with("+++ killed by SIG") && text.ends_with(" +++")
    {
        return Ok(ParsedLine::Marker);
    }
    if let Some(result) = text.strip_prefix("<... execve resumed>) = ") {
        return Ok(ParsedLine::Resume(outcome(result)?));
    }
    let Some(mut rest) = input.strip_prefix(b"execve(") else {
        return Err(invalid("unclassified strace line"));
    };
    let program = escaped_string(&mut rest)?;
    if !rest.starts_with(b", ") {
        return Err(invalid("missing execve argv separator"));
    }
    rest = &rest[2..];
    let args = argv(&mut rest)?;
    if !rest.starts_with(b", ") {
        return Err(invalid("missing execve environment separator"));
    }
    rest = &rest[2..];
    let remainder = std::str::from_utf8(rest)?;
    if let Some(env) = remainder.strip_suffix(" <unfinished ...>") {
        environment_shape(env)?;
        return Ok(ParsedLine::Start(program, args));
    }
    let Some((env, result)) = remainder.split_once(") = ") else {
        return Err(invalid("incomplete execve call"));
    };
    environment_shape(env)?;
    Ok(ParsedLine::Complete(program, args, outcome(result)?))
}

fn read_bound_regular(path: &Path, maximum: u64) -> Result<Vec<u8>> {
    let mut file = File::from(rustix::fs::open(
        path,
        rustix::fs::OFlags::RDONLY
            | rustix::fs::OFlags::NONBLOCK
            | rustix::fs::OFlags::NOFOLLOW
            | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )?);
    let held = file.metadata()?;
    if !held.is_file() || held.len() > maximum {
        return Err(invalid("invalid bounded trace input"));
    }
    let mut bytes = Vec::with_capacity(held.len() as usize);
    (&mut file).take(maximum + 1).read_to_end(&mut bytes)?;
    file.seek(SeekFrom::Start(0))?;
    let mut confirmed = Vec::with_capacity(held.len() as usize);
    (&mut file).take(maximum + 1).read_to_end(&mut confirmed)?;
    let named = fs::symlink_metadata(path)?;
    if !named.is_file()
        || named.file_type().is_symlink()
        || named.dev() != held.dev()
        || named.ino() != held.ino()
        || named.len() != held.len()
        || bytes.len() as u64 != held.len()
        || bytes != confirmed
    {
        return Err(invalid("bounded trace input changed"));
    }
    Ok(bytes)
}

fn scan_trace_dir(directory: &Path) -> Result<Vec<TraceFileSeal>> {
    let mut files = Vec::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_str().ok_or("non-UTF8 trace filename")?;
        let Some(suffix) = name.strip_prefix("exec.") else {
            continue;
        };
        if suffix.is_empty() || !suffix.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(invalid("malformed exec trace filename"));
        }
        let pid = suffix.parse::<u32>()?;
        let metadata = fs::symlink_metadata(entry.path())?;
        if !metadata.is_file() || metadata.len() > MAX_TRACE_BYTES {
            return Err(invalid("nonregular or oversized exec trace"));
        }
        files.push(TraceFileSeal {
            pid,
            path: entry.path(),
            dev: metadata.dev(),
            ino: metadata.ino(),
            len: metadata.len(),
            sha256: String::new(),
        });
        if files.len() > MAX_TRACE_FILES {
            return Err(invalid("too many exec trace files"));
        }
    }
    files.sort_by_key(|file| file.pid);
    if files.is_empty() || files.windows(2).any(|pair| pair[0].pid == pair[1].pid) {
        return Err(invalid("missing or repeated exec trace PID"));
    }
    let total = files.iter().map(|file| file.len).sum::<u64>();
    if total > MAX_TOTAL_BYTES {
        return Err(invalid("exec trace corpus exceeds bound"));
    }
    Ok(files)
}

impl TraceCorpus {
    pub fn verify(&mut self) -> Result<()> {
        let current = scan_trace_dir(&self.directory)?;
        if current.len() != self.files.len() {
            return Err(invalid("exec trace PID roster changed"));
        }
        for (named, held) in current.iter().zip(&mut self.files) {
            let expected = &held.seal;
            if named.pid != expected.pid
                || named.path != expected.path
                || named.dev != expected.dev
                || named.ino != expected.ino
                || named.len != expected.len
            {
                return Err(invalid("exec trace identity or PID roster changed"));
            }
            let metadata = held.file.metadata()?;
            if !metadata.is_file()
                || metadata.dev() != expected.dev
                || metadata.ino() != expected.ino
                || metadata.len() != expected.len
            {
                return Err(invalid("held exec trace identity changed"));
            }
            held.file.seek(SeekFrom::Start(0))?;
            let mut bytes = Vec::with_capacity(expected.len as usize);
            (&mut held.file)
                .take(MAX_TRACE_BYTES + 1)
                .read_to_end(&mut bytes)?;
            if bytes.len() as u64 != expected.len
                || format!("{:x}", Sha256::digest(&bytes)) != expected.sha256
            {
                return Err(invalid("exec trace content changed"));
            }
        }
        Ok(())
    }

    pub fn seals(&self) -> Vec<TraceFileSeal> {
        self.files.iter().map(|file| file.seal.clone()).collect()
    }

    pub fn attempts(&self) -> &[Attempt] {
        &self.attempts
    }
}

pub fn parse_trace_dir(directory: &Path) -> Result<TraceCorpus> {
    let files = scan_trace_dir(directory)?;
    let mut attempts = Vec::new();
    let mut held_files = Vec::new();
    for mut seal in files {
        let mut file = File::from(rustix::fs::open(
            &seal.path,
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::NONBLOCK
                | rustix::fs::OFlags::NOFOLLOW
                | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )?);
        let metadata = file.metadata()?;
        if !metadata.is_file()
            || metadata.dev() != seal.dev
            || metadata.ino() != seal.ino
            || metadata.len() != seal.len
        {
            return Err(invalid("changed exec trace"));
        }
        let mut bytes = Vec::with_capacity(seal.len as usize);
        (&mut file)
            .take(MAX_TRACE_BYTES + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 != seal.len {
            return Err(invalid("changed exec trace length"));
        }
        seal.sha256 = format!("{:x}", Sha256::digest(&bytes));
        if bytes.is_empty() {
            held_files.push(HeldTraceFile { seal, file });
            continue;
        }
        if !bytes.ends_with(b"\n") {
            return Err(invalid("changed or partial exec trace"));
        }
        let mut pending = None;
        for (index, raw) in bytes[..bytes.len() - 1]
            .split(|byte| *byte == b'\n')
            .enumerate()
        {
            if raw.len() > MAX_LINE_BYTES || raw.is_empty() {
                return Err(invalid("empty or oversized exec trace line"));
            }
            match line(raw)? {
                ParsedLine::Start(program, argv) => {
                    if pending.replace((program, argv, index + 1)).is_some() {
                        return Err(invalid("nested unfinished execve"));
                    }
                }
                ParsedLine::Complete(program, argv, outcome) => {
                    if pending.is_some() {
                        return Err(invalid("unfinished execve before next call"));
                    }
                    attempts.push(Attempt {
                        pid: seal.pid,
                        line: index + 1,
                        program,
                        argv,
                        outcome,
                    });
                }
                ParsedLine::Resume(outcome) => {
                    let (program, argv, started) = pending.take().ok_or("orphan resumed execve")?;
                    attempts.push(Attempt {
                        pid: seal.pid,
                        line: started,
                        program,
                        argv,
                        outcome,
                    });
                }
                ParsedLine::Marker => {}
            }
            if attempts.len() > MAX_ATTEMPTS {
                return Err(invalid("too many exec attempts"));
            }
        }
        if pending.is_some() {
            return Err(invalid("unfinished execve at end of PID trace"));
        }
        held_files.push(HeldTraceFile { seal, file });
    }
    if attempts.is_empty() {
        return Err(invalid("exec trace contains no attempts"));
    }
    let mut corpus = TraceCorpus {
        directory: directory.to_owned(),
        files: held_files,
        attempts,
    };
    corpus.verify()?;
    Ok(corpus)
}

fn digest_regular(path: &Path, maximum: u64) -> Result<(u64, u64, u64, String)> {
    let file = File::from(rustix::fs::open(
        path,
        rustix::fs::OFlags::RDONLY
            | rustix::fs::OFlags::NONBLOCK
            | rustix::fs::OFlags::NOFOLLOW
            | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )?);
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > maximum {
        return Err(invalid("invalid executable seal input"));
    }
    let mut limited = file.take(maximum + 1);
    let mut hash = Sha256::new();
    let mut observed = 0_u64;
    let mut chunk = [0_u8; 65_536];
    loop {
        let count = limited.read(&mut chunk)?;
        if count == 0 {
            break;
        }
        observed += count as u64;
        hash.update(&chunk[..count]);
    }
    if observed != metadata.len() {
        return Err(invalid("executable changed during seal"));
    }
    Ok((
        metadata.dev(),
        metadata.ino(),
        metadata.len(),
        format!("{:x}", hash.finalize()),
    ))
}

pub fn capture_program_seal(path: &Path) -> Result<ProgramSeal> {
    if !path.is_absolute() {
        return Err(invalid("executable seal path must be absolute"));
    }
    if path
        .components()
        .any(|component| !matches!(component, Component::RootDir | Component::Normal(_)))
    {
        return Err(invalid("non-normal executable path"));
    }
    let mut links = Vec::new();
    let mut visited = BTreeSet::new();
    let resolved = resolve_program_path(path, &mut links, &mut visited)?;
    if fs::canonicalize(path)? != resolved {
        return Err(invalid("executable symlink resolution changed"));
    }
    let (dev, ino, len, sha256) = digest_regular(&resolved, MAX_PROGRAM_BYTES)?;
    Ok(ProgramSeal {
        requested: path.as_os_str().as_bytes().to_vec(),
        resolved: resolved.as_os_str().as_bytes().to_vec(),
        links,
        dev,
        ino,
        len,
        sha256,
    })
}

fn resolve_program_path(
    path: &Path,
    links: &mut Vec<LinkSeal>,
    visited: &mut BTreeSet<Vec<u8>>,
) -> Result<PathBuf> {
    let mut prefix = PathBuf::new();
    for component in path.components() {
        match component {
            Component::RootDir | Component::Normal(_) => prefix.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                if !prefix.pop() {
                    return Err(invalid("executable symlink target escapes root"));
                }
            }
            Component::Prefix(_) => return Err(invalid("non-Unix executable path")),
        }
        let metadata = fs::symlink_metadata(&prefix)?;
        if metadata.file_type().is_symlink() {
            if links.len() >= 40 || !visited.insert(prefix.as_os_str().as_bytes().to_vec()) {
                return Err(invalid("executable symlink cycle or depth exceeded"));
            }
            let target = fs::read_link(&prefix)?;
            links.push(LinkSeal {
                path: prefix.as_os_str().as_bytes().to_vec(),
                target: target.as_os_str().as_bytes().to_vec(),
            });
            let next = if target.is_absolute() {
                target
            } else {
                prefix
                    .parent()
                    .ok_or("executable symlink without parent")?
                    .join(target)
            };
            prefix = resolve_program_path(&next, links, visited)?;
        }
    }
    Ok(prefix)
}

fn forbidden_interpreter(program: &[u8], argv0: &[u8]) -> bool {
    [program, argv0].into_iter().any(|word| {
        let name = word.rsplit(|byte| *byte == b'/').next().unwrap_or(word);
        let name = name.iter().map(u8::to_ascii_lowercase).collect::<Vec<_>>();
        name.starts_with(b"python") || name.starts_with(b"pypy") || name.ends_with(b".py")
    })
}

fn program_prefix(seal: &ProgramSeal) -> Result<Vec<u8>> {
    let path = Path::new(OsStr::from_bytes(&seal.resolved));
    let mut file = File::from(rustix::fs::open(
        path,
        rustix::fs::OFlags::RDONLY
            | rustix::fs::OFlags::NONBLOCK
            | rustix::fs::OFlags::NOFOLLOW
            | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )?);
    let mut prefix = vec![0_u8; 4096.min(seal.len as usize)];
    file.read_exact(&mut prefix)?;
    Ok(prefix)
}

fn alias_binding(source: &ProgramSeal, alias: &ProgramSeal) -> Result<Value> {
    let source_path = std::str::from_utf8(&source.requested)?;
    let alias_path = std::str::from_utf8(&alias.requested)?;
    Ok(
        json!({"source":source_path,"alias":alias_path,"dev":source.dev,
        "ino":source.ino,"len":source.len,"sha256":&source.sha256}),
    )
}

fn verify_created_receipt(path: &Path, seal: &ProgramSeal, mode: u32) -> Result<()> {
    let bytes = read_bound_regular(path, 4096)?;
    let text = std::str::from_utf8(&bytes)?;
    let rows = text.lines().collect::<Vec<_>>();
    if rows.len() != 2 || !text.ends_with('\n') {
        return Err(invalid("created executable receipt incomplete"));
    }
    let program = std::str::from_utf8(&seal.requested)?;
    for (stage, row) in ["created", "after-family"].into_iter().zip(rows) {
        let actual: Value = serde_json::from_str(row)?;
        let expected = json!({"stage":stage,"path":program,"dev":seal.dev,
            "ino":seal.ino,"len":seal.len,"sha256":seal.sha256,"mode":mode});
        if actual != expected {
            return Err(invalid("created executable identity changed"));
        }
    }
    Ok(())
}

fn verify_created_symlink_receipt(
    path: &Path,
    seal: &ProgramSeal,
    source: &ProgramSeal,
) -> Result<()> {
    let bytes = read_bound_regular(path, 4096)?;
    let text = std::str::from_utf8(&bytes)?;
    let rows = text.lines().collect::<Vec<_>>();
    let alias = Path::new(OsStr::from_bytes(&seal.requested));
    let target = Path::new(OsStr::from_bytes(&source.requested));
    let link = fs::symlink_metadata(alias)?;
    if rows.len() != 2
        || !text.ends_with('\n')
        || !link.file_type().is_symlink()
        || fs::read_link(alias)? != target
    {
        return Err(invalid(
            "created native provider symlink receipt incomplete",
        ));
    }
    for (stage, row) in ["created", "after-family"].into_iter().zip(rows) {
        let actual: Value = serde_json::from_str(row)?;
        let expected = json!({"stage":stage,"alias":seal.requested,
            "target":source.requested,"link_dev":link.dev(),
            "link_ino":link.ino(),"source_sha256":source.sha256});
        if actual != expected {
            return Err(invalid("created native provider symlink identity changed"));
        }
    }
    Ok(())
}

fn verify_alias_receipt(
    rule: &OwnedAliasReceiptRule,
    seals: &BTreeMap<Vec<u8>, &ProgramSeal>,
    relations: &[AliasRelation],
) -> Result<()> {
    let mut stages = vec!["created".to_owned()];
    for name in ACTION_SCENARIOS {
        stages.push(format!("before-{name}"));
        stages.push(format!("after-{name}"));
    }
    stages.extend([
        "before-pty".into(),
        "after-pty".into(),
        "after-family".into(),
    ]);
    for (alias, source) in [
        (&rule.candidate_alias, &rule.candidate_source),
        (&rule.child_alias, &rule.child_source),
    ] {
        if !relations.iter().any(|relation| {
            relation.alias.as_slice() == alias.as_slice()
                && relation.source.as_slice() == source.as_slice()
                && relation.same_inode
        }) {
            return Err(invalid("owned executable alias lacks hardlink relation"));
        }
    }
    let candidate = alias_binding(
        seals
            .get(&rule.candidate_source)
            .ok_or("unsealed action source")?,
        seals
            .get(&rule.candidate_alias)
            .ok_or("unsealed action alias")?,
    )?;
    let child = alias_binding(
        seals
            .get(&rule.child_source)
            .ok_or("unsealed child source")?,
        seals.get(&rule.child_alias).ok_or("unsealed child alias")?,
    )?;
    let bytes = read_bound_regular(&rule.path, 2 * 1024 * 1024)?;
    let text = std::str::from_utf8(&bytes)?;
    let rows = text.lines().collect::<Vec<_>>();
    if rows.len() != stages.len() || !text.ends_with('\n') {
        return Err(invalid("owned executable binding receipt incomplete"));
    }
    for (row, stage) in rows.into_iter().zip(&stages) {
        let actual: Value = serde_json::from_str(row)?;
        let expected =
            json!({"stage":stage,"candidate":candidate.clone(),"process_child":child.clone()});
        if actual != expected {
            return Err(invalid("owned executable binding receipt changed"));
        }
    }
    Ok(())
}

fn launch_descriptor_path(path: &[u8]) -> bool {
    let Some(rest) = path.strip_prefix(b"/tmp/op-exec-") else {
        return false;
    };
    let mut components = rest.splitn(2, |byte| *byte == b'/');
    let (Some(directory), Some(file)) = (components.next(), components.next()) else {
        return false;
    };
    let Some(file) = file.strip_prefix(b".tmp") else {
        return false;
    };
    directory.len() == 6
        && file.len() == 6
        && directory.iter().chain(file).all(u8::is_ascii_alphanumeric)
}

fn matches_typed(rule: &ExpectedTypedAttempt, attempt: &Attempt) -> bool {
    rule.program == attempt.program
        && rule.outcome == attempt.outcome
        && rule.argv.len() == attempt.argv.len()
        && rule
            .argv
            .iter()
            .zip(&attempt.argv)
            .all(|(rule, value)| match rule {
                TypedArg::Exact(expected) => expected == value,
                TypedArg::LaunchDescriptor => launch_descriptor_path(value),
            })
}

pub fn audit(mut corpus: TraceCorpus, policy: &ExplicitPolicy) -> Result<AuditReceipt> {
    corpus.verify()?;
    let mut receipt = audit_attempts(corpus.attempts.clone(), policy)?;
    for (before, after) in &policy.program_seals {
        if capture_program_seal(Path::new(OsStr::from_bytes(&before.requested)))? != *after {
            return Err(invalid("executable changed before trace audit completed"));
        }
    }
    for created in &policy.created_programs {
        if capture_program_seal(Path::new(OsStr::from_bytes(&created.final_seal.requested)))?
            != created.final_seal
        {
            return Err(invalid(
                "created executable changed before trace audit completed",
            ));
        }
    }
    corpus.verify()?;
    receipt.trace_files = corpus.seals();
    Ok(receipt)
}

fn audit_attempts(attempts: Vec<Attempt>, policy: &ExplicitPolicy) -> Result<AuditReceipt> {
    if policy.harness_exit_code != 0
        || policy.historical_sources != 13
        || policy.same_elf_digests.len() != 6
        || policy.alias_receipts.len() != 2
    {
        return Err(invalid("frozen harness failed"));
    }
    let admission = read_bound_regular(&policy.admission_path, 64 * 1024)?;
    let receipt: Value = serde_json::from_slice(&admission)?;
    if receipt["pass"] != true
        || receipt["lanes"].as_u64() != Some(2)
        || receipt["controllers_per_lane"].as_u64() != Some(8)
        || receipt["historical_sources"].as_u64() != Some(policy.historical_sources as u64)
        || receipt["same_elf_digests"] != serde_json::to_value(&policy.same_elf_digests)?
    {
        return Err(invalid("incomplete frozen recovery admission"));
    }
    let mut seals = BTreeMap::new();
    for (before, after) in &policy.program_seals {
        if before != after
            || capture_program_seal(Path::new(OsStr::from_bytes(&before.requested)))? != *after
            || forbidden_interpreter(&before.resolved, &before.resolved)
        {
            return Err(invalid("executable, symlink alias, or interpreter changed"));
        }
        if seals.insert(before.requested.clone(), before).is_some() {
            return Err(invalid("duplicate executable seal"));
        }
    }
    for created in &policy.created_programs {
        let seal = &created.final_seal;
        if capture_program_seal(Path::new(OsStr::from_bytes(&seal.requested)))? != *seal
            || forbidden_interpreter(&seal.resolved, &seal.resolved)
            || seals.insert(seal.requested.clone(), seal).is_some()
        {
            return Err(invalid("created executable seal changed or duplicated"));
        }
    }
    let mut digests = BTreeMap::new();
    for (path, digest) in &policy.expected_program_sha256 {
        if digest.len() != 64
            || !digest.bytes().all(|byte| byte.is_ascii_hexdigit())
            || digests.insert(path.clone(), digest.clone()).is_some()
        {
            return Err(invalid("invalid or duplicate expected program digest"));
        }
    }
    if digests.len() != seals.len() {
        return Err(invalid("program digest and seal rosters differ"));
    }
    for (path, seal) in &seals {
        if digests.get(path).map(String::as_str) != Some(seal.sha256.as_str()) {
            return Err(invalid("executable bytes differ from independent digest"));
        }
    }
    if policy
        .same_elf_digests
        .iter()
        .any(|digest| !seals.values().any(|seal| seal.sha256 == *digest))
    {
        return Err(invalid("frozen harness ELF digest lacks executable seal"));
    }
    let mut related = BTreeSet::new();
    for relation in &policy.alias_relations {
        if !related.insert(&relation.alias) {
            return Err(invalid("duplicate executable alias relation"));
        }
        let alias = seals
            .get(&relation.alias)
            .ok_or("unsealed executable alias")?;
        let source = seals.get(&relation.source).ok_or("unsealed alias source")?;
        if alias.sha256 != source.sha256
            || alias.len != source.len
            || relation.same_inode && (alias.dev != source.dev || alias.ino != source.ino)
        {
            return Err(invalid("native alias/source relation changed"));
        }
    }
    let mut alias_receipt_paths = BTreeSet::new();
    for receipt in &policy.alias_receipts {
        if !alias_receipt_paths.insert(&receipt.path) {
            return Err(invalid("duplicate owned executable binding receipt"));
        }
        verify_alias_receipt(receipt, &seals, &policy.alias_relations)?;
    }
    for created in &policy.created_programs {
        let seal = &created.final_seal;
        let named = fs::symlink_metadata(Path::new(OsStr::from_bytes(&seal.requested)))?;
        let symlink_alias = matches!(&created.kind, CreatedProgramKind::SymlinkAlias { .. });
        if (!symlink_alias
            && (!named.is_file() || named.file_type().is_symlink() || !seal.links.is_empty()))
            || (symlink_alias && !named.file_type().is_symlink())
        {
            return Err(invalid(
                "created executable path has unexpected file type or link chain",
            ));
        }
        let mode = named.permissions().mode() & 0o777;
        match &created.kind {
            CreatedProgramKind::Alias { source } => {
                let original = seals.get(source).ok_or("unsealed hardlink source")?;
                if !policy.alias_relations.iter().any(|relation| {
                    relation.alias.as_slice() == seal.requested.as_slice()
                        && relation.source.as_slice() == source.as_slice()
                        && relation.same_inode
                }) || !policy.alias_receipts.iter().any(|receipt| {
                    (receipt.candidate_alias.as_slice() == seal.requested.as_slice()
                        && receipt.candidate_source.as_slice() == source.as_slice())
                        || (receipt.child_alias.as_slice() == seal.requested.as_slice()
                            && receipt.child_source.as_slice() == source.as_slice())
                }) || (seal.dev, seal.ino, seal.len, &seal.sha256)
                    != (original.dev, original.ino, original.len, &original.sha256)
                {
                    return Err(invalid("created alias lacks original held hardlink proof"));
                }
            }
            CreatedProgramKind::SymlinkAlias { source, receipt } => {
                let original = seals.get(source).ok_or("unsealed symlink provider")?;
                let mut expected_links = vec![LinkSeal {
                    path: seal.requested.clone(),
                    target: source.clone(),
                }];
                expected_links.extend(original.links.clone());
                if seal.links != expected_links
                    || (seal.dev, seal.ino, seal.len, &seal.sha256)
                        != (original.dev, original.ino, original.len, &original.sha256)
                    || seal.resolved != original.resolved
                {
                    return Err(invalid("created symlink differs from held native provider"));
                }
                verify_created_symlink_receipt(receipt, seal, original)?;
            }
            CreatedProgramKind::CopiedElf { source, receipt } => {
                let original = seals.get(source).ok_or("unsealed copied ELF source")?;
                if seal.sha256 != original.sha256 || seal.len != original.len {
                    return Err(invalid("created ELF differs from sealed source"));
                }
                verify_created_receipt(receipt, seal, mode)?;
            }
            CreatedProgramKind::AuthoredScript {
                bytes,
                mode: authored_mode,
                receipt,
            } => {
                if bytes.len() > 4096
                    || !bytes.starts_with(b"#!")
                    || format!("{:x}", Sha256::digest(bytes)) != seal.sha256
                    || mode != *authored_mode
                {
                    return Err(invalid(
                        "created script differs from authored bytes or mode",
                    ));
                }
                verify_created_receipt(receipt, seal, mode)?;
            }
        }
    }
    let mut scripts = BTreeMap::new();
    for script in &policy.scripts {
        if scripts.insert(script.script.clone(), script).is_some()
            || !seals.contains_key(&script.script)
            || !seals.contains_key(&script.interpreter)
            || !script.exact_shebang.starts_with(b"#!")
            || forbidden_interpreter(&script.interpreter, &script.interpreter)
        {
            return Err(invalid("unsealed or duplicate script interpreter"));
        }
        let command = script.exact_shebang[2..]
            .iter()
            .copied()
            .skip_while(|byte| *byte == b' ' || *byte == b'\t')
            .take_while(|byte| *byte != b' ' && *byte != b'\t')
            .collect::<Vec<_>>();
        if command != script.interpreter {
            return Err(invalid("script shebang interpreter changed"));
        }
    }
    for (path, seal) in &seals {
        let prefix = program_prefix(seal)?;
        if let Some(script) = scripts.get(path) {
            let line = prefix.split(|byte| *byte == b'\n').next().unwrap_or(&[]);
            if line != script.exact_shebang {
                return Err(invalid("source-authored script shebang changed"));
            }
        } else if !prefix.starts_with(b"\x7fELF") {
            return Err(invalid("undeclared non-ELF executable"));
        }
    }
    let mut expected = BTreeMap::new();
    for rule in &policy.expected {
        if rule.count == 0 || rule.argv.is_empty() || rule.argv.len() > MAX_ARGV {
            return Err(invalid("invalid explicit exec rule"));
        }
        if let Outcome::Errno(errno) = &rule.outcome {
            let owned_permission_fixture = rule
                .program
                .ends_with(b"/actions/permission_tmux/tools/tmux");
            if errno != "ENOENT" && !(errno == "EACCES" && owned_permission_fixture) {
                return Err(invalid("unadmitted exec failure result"));
            }
        }
        let key = (
            rule.program.clone(),
            rule.argv.clone(),
            rule.outcome.clone(),
        );
        if expected.insert(key, rule.count).is_some() {
            return Err(invalid("duplicate explicit exec rule"));
        }
        if rule.outcome == Outcome::Success && !seals.contains_key(&rule.program) {
            return Err(invalid("successful executable lacks source/provider seal"));
        }
    }
    for rule in &policy.typed_expected {
        if rule.count == 0
            || rule.argv.is_empty()
            || rule.argv.len() > MAX_ARGV
            || rule.outcome != Outcome::Success
            || rule
                .argv
                .iter()
                .filter(|arg| matches!(arg, TypedArg::LaunchDescriptor))
                .count()
                != 1
            || !seals.contains_key(&rule.program)
            || !rule.program.ends_with(b"openpilot-process-child")
            || rule.argv.first() != Some(&TypedArg::Exact(rule.program.clone()))
        {
            return Err(invalid("invalid typed launch-descriptor exec rule"));
        }
    }
    let mut allowed = BTreeMap::new();
    for rule in &policy.allowed_programs {
        if rule.maximum == 0
            || rule.maximum > MAX_ATTEMPTS
            || rule.minimum > rule.maximum
            || rule.program.is_empty()
            || (rule.outcome == Outcome::Success && !seals.contains_key(&rule.program))
        {
            return Err(invalid("invalid executable-provenance rule"));
        }
        if let Outcome::Errno(errno) = &rule.outcome {
            let permission = rule
                .program
                .ends_with(b"/actions/permission_tmux/tools/tmux");
            if errno != "ENOENT" && !(permission && errno == "EACCES") {
                return Err(invalid("unadmitted executable failure"));
            }
        }
        if allowed
            .insert((rule.program.clone(), rule.outcome.clone()), rule)
            .is_some()
        {
            return Err(invalid("duplicate executable-provenance rule"));
        }
    }
    let harness_key = (
        policy.harness_program.clone(),
        policy.harness_argv.clone(),
        Outcome::Success,
    );
    if expected.get(&harness_key) != Some(&1)
        && !allowed.contains_key(&(policy.harness_program.clone(), Outcome::Success))
    {
        return Err(invalid("exact frozen harness invocation absent"));
    }
    let mut seen = BTreeMap::new();
    let mut seen_typed = vec![0_usize; policy.typed_expected.len()];
    let mut seen_programs = BTreeMap::new();
    let mut successful = 0;
    let mut failed = 0;
    for attempt in &attempts {
        if attempt.argv.is_empty() || forbidden_interpreter(&attempt.program, &attempt.argv[0]) {
            return Err(invalid("Python, empty, or opaque executable attempt"));
        }
        if attempt.outcome == Outcome::Success
            && attempt.program.ends_with(b"openpilot-process-child")
            && (attempt.argv.len() != 2
                || attempt.argv[0] != attempt.program
                || !launch_descriptor_path(&attempt.argv[1]))
        {
            return Err(invalid("process-child launch descriptor argv changed"));
        }
        let key = (
            attempt.program.clone(),
            attempt.argv.clone(),
            attempt.outcome.clone(),
        );
        let typed = policy
            .typed_expected
            .iter()
            .enumerate()
            .filter(|(_, rule)| matches_typed(rule, attempt))
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        let program_key = (attempt.program.clone(), attempt.outcome.clone());
        let program_admitted = allowed.contains_key(&program_key);
        if (expected.contains_key(&key) as usize) + typed.len() + (program_admitted as usize) != 1 {
            return Err(invalid(&format!(
                "exec path, argv, or result not explicitly admitted: pid={} program={:?} outcome={:?}",
                attempt.pid, String::from_utf8_lossy(&attempt.program), attempt.outcome
            )));
        }
        if let Some(index) = typed.first() {
            seen_typed[*index] += 1;
        } else if program_admitted {
            *seen_programs.entry(program_key).or_insert(0_usize) += 1;
        } else {
            *seen.entry(key).or_insert(0_usize) += 1;
        }
        match attempt.outcome {
            Outcome::Success => successful += 1,
            Outcome::Errno(_) => failed += 1,
        }
    }
    if seen != expected
        || seen_typed
            .iter()
            .zip(&policy.typed_expected)
            .any(|(seen, rule)| *seen != rule.count)
        || allowed.iter().any(|(key, rule)| {
            let seen = seen_programs.get(key).copied().unwrap_or(0);
            seen < rule.minimum || seen > rule.maximum
        })
        || attempts
            .iter()
            .filter(|attempt| {
                attempt.program == policy.harness_program
                    && attempt.argv == policy.harness_argv
                    && attempt.outcome == Outcome::Success
            })
            .count()
            != 1
        || successful == 0
    {
        return Err(invalid("exec attempt count differs from explicit policy"));
    }
    let admission_sha256 = format!("{:x}", Sha256::digest(&admission));
    Ok(AuditReceipt {
        passed: true,
        argv_scope: if policy.allowed_programs.is_empty() {
            "exact argv and count".into()
        } else {
            "sealed executable provenance and result; raw argv retained without parity claim".into()
        },
        attempts,
        successful,
        failed,
        program_counts: policy
            .allowed_programs
            .iter()
            .map(|rule| {
                (
                    rule.clone(),
                    seen_programs
                        .get(&(rule.program.clone(), rule.outcome.clone()))
                        .copied()
                        .unwrap_or(0),
                )
            })
            .collect(),
        program_seals: policy.program_seals.clone(),
        created_programs: policy.created_programs.clone(),
        alias_relations: policy.alias_relations.clone(),
        scripts: policy.scripts.clone(),
        alias_receipts: policy.alias_receipts.clone(),
        admission_sha256,
        trace_files: Vec::new(),
    })
}

pub fn write_receipt_exclusive(path: &Path, receipt: &AuditReceipt) -> Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    serde_json::to_writer_pretty(&mut file, receipt)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encoded(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("\\x{byte:02x}")).collect()
    }

    #[test]
    fn exact_hex_trace_reassembles_only_same_pid_and_preserves_raw_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let call = format!(
            "execve(\"{}\", [\"{}\", \"{}\"], 0xabc /* 2 vars */ <unfinished ...>\n<... execve resumed>) = -1 ENOENT (No such file or directory)\n",
            encoded(b"/bin/tool"), encoded(b"tool"), encoded(b"\xff")
        );
        fs::write(dir.path().join("exec.21"), call).unwrap();
        let parsed = parse_trace_dir(dir.path()).unwrap();
        assert_eq!(parsed.attempts.len(), 1);
        assert_eq!(parsed.attempts[0].pid, 21);
        assert_eq!(parsed.attempts[0].program, b"/bin/tool");
        assert_eq!(parsed.attempts[0].argv[1], b"\xff");
        assert_eq!(parsed.attempts[0].outcome, Outcome::Errno("ENOENT".into()));
    }

    #[test]
    fn unknown_truncated_and_cross_pid_lines_fail_closed() {
        let dir = tempfile::tempdir().unwrap();
        for text in [
            "execveat(AT_FDCWD, \"x\", [], 0, 0) = 0\n",
            "execve(\"...\", [\"\\x61\"], 0x1) = 0\n",
            "<... execve resumed>) = 0\n",
            "execve(\"\\x61\", [\"\\x61\"], 0x1 <unfinished ...>\n",
            "???( <unfinished ...>\n",
        ] {
            fs::write(dir.path().join("exec.21"), text).unwrap();
            assert!(parse_trace_dir(dir.path()).is_err(), "accepted {text}");
        }
    }

    #[test]
    fn held_trace_rejects_same_length_rewrite_and_new_pid() {
        let dir = tempfile::tempdir().unwrap();
        let trace = dir.path().join("exec.21");
        let valid = "execve(\"\\x2f\\x62\\x69\\x6e\\x2f\\x61\", [\"\\x61\"], 0x1) = 0\n";
        fs::write(&trace, valid).unwrap();
        let mut corpus = parse_trace_dir(dir.path()).unwrap();
        assert_eq!(corpus.seals().len(), 1);
        let changed = valid.replace("\\x61", "\\x62");
        assert_eq!(valid.len(), changed.len());
        fs::write(&trace, changed).unwrap();
        assert!(corpus.verify().is_err(), "same-length rewrite passed");

        fs::write(&trace, valid).unwrap();
        let mut corpus = parse_trace_dir(dir.path()).unwrap();
        fs::write(dir.path().join("exec.22"), valid).unwrap();
        assert!(corpus.verify().is_err(), "new PID escaped the roster");
    }

    #[test]
    fn exact_policy_rejects_wrong_result_argv_and_python() {
        let dir = tempfile::tempdir().unwrap();
        let mut sources = Vec::new();
        for index in 0..6 {
            let path = dir.path().join(format!("source-{index}"));
            fs::write(&path, format!("\u{7f}ELFsource-{index}")).unwrap();
            sources.push(path);
        }
        let program = &sources[0];
        let program_bytes = program.as_os_str().as_bytes().to_vec();
        let argv = vec![program_bytes.clone(), b"fixed".to_vec()];
        let mut expected = vec![ExpectedAttempt {
            program: program_bytes.clone(),
            argv: argv.clone(),
            outcome: Outcome::Success,
            count: 1,
        }];
        let mut attempts = vec![Attempt {
            pid: 1,
            line: 1,
            program: program_bytes.clone(),
            argv: argv.clone(),
            outcome: Outcome::Success,
        }];
        let mut program_seals = Vec::new();
        let mut expected_program_sha256 = Vec::new();
        let mut same_elf_digests = Vec::new();
        for path in &sources {
            let seal = capture_program_seal(path).unwrap();
            same_elf_digests.push(seal.sha256.clone());
            expected_program_sha256.push((seal.requested.clone(), seal.sha256.clone()));
            program_seals.push((seal.clone(), seal));
        }
        let mut alias_relations = Vec::new();
        let mut alias_receipts = Vec::new();
        let mut created_programs = Vec::new();
        let mut stages = vec!["created".to_owned()];
        for name in ACTION_SCENARIOS {
            stages.push(format!("before-{name}"));
            stages.push(format!("after-{name}"));
        }
        stages.extend([
            "before-pty".into(),
            "after-pty".into(),
            "after-family".into(),
        ]);
        for lane in 0..2 {
            let directory = dir.path().join(format!("lane-{lane}"));
            fs::create_dir(&directory).unwrap();
            let candidate_alias = directory.join("recovery_actions");
            let child_alias = directory.join("openpilot-process-child");
            fs::hard_link(&sources[0], &candidate_alias).unwrap();
            fs::hard_link(&sources[1], &child_alias).unwrap();
            let candidate = capture_program_seal(&candidate_alias).unwrap();
            let child = capture_program_seal(&child_alias).unwrap();
            for (source, alias) in [(&sources[0], &candidate), (&sources[1], &child)] {
                alias_relations.push(AliasRelation {
                    alias: alias.requested.clone(),
                    source: source.as_os_str().as_bytes().to_vec(),
                    same_inode: true,
                });
                expected_program_sha256.push((alias.requested.clone(), alias.sha256.clone()));
                created_programs.push(CreatedProgramRule {
                    final_seal: alias.clone(),
                    kind: CreatedProgramKind::Alias {
                        source: source.as_os_str().as_bytes().to_vec(),
                    },
                });
                let mut alias_argv = vec![alias.requested.clone()];
                if alias.requested.ends_with(b"openpilot-process-child") {
                    alias_argv.push(b"/tmp/op-exec-pJpgwF/.tmphIfUAU".to_vec());
                }
                expected.push(ExpectedAttempt {
                    program: alias.requested.clone(),
                    argv: alias_argv.clone(),
                    outcome: Outcome::Success,
                    count: 1,
                });
                let pid = attempts.len() as u32 + 1;
                attempts.push(Attempt {
                    pid,
                    line: 1,
                    program: alias.requested.clone(),
                    argv: alias_argv,
                    outcome: Outcome::Success,
                });
            }
            let receipt_path = directory.join("owned-native-bin-bindings.jsonl");
            let candidate_binding = alias_binding(&program_seals[0].0, &candidate).unwrap();
            let child_binding = alias_binding(&program_seals[1].0, &child).unwrap();
            let mut rows = Vec::new();
            for stage in &stages {
                rows.extend(
                    serde_json::to_vec(&json!({"stage":stage,
                    "candidate":candidate_binding.clone(),"process_child":child_binding.clone()}))
                    .unwrap(),
                );
                rows.push(b'\n');
            }
            fs::write(&receipt_path, rows).unwrap();
            alias_receipts.push(OwnedAliasReceiptRule {
                path: receipt_path,
                candidate_source: sources[0].as_os_str().as_bytes().to_vec(),
                candidate_alias: candidate.requested,
                child_source: sources[1].as_os_str().as_bytes().to_vec(),
                child_alias: child.requested,
            });
        }
        let admission = dir.path().join("admission.json");
        fs::write(
            &admission,
            serde_json::to_vec(&json!({"pass":true,"lanes":2,"controllers_per_lane":8,
                "historical_sources":13,"same_elf_digests":same_elf_digests.clone()}))
            .unwrap(),
        )
        .unwrap();
        let policy = ExplicitPolicy {
            expected,
            typed_expected: vec![],
            allowed_programs: vec![],
            program_seals,
            created_programs,
            expected_program_sha256,
            alias_relations,
            scripts: vec![],
            alias_receipts,
            admission_path: admission,
            same_elf_digests,
            historical_sources: 13,
            harness_exit_code: 0,
            harness_program: program_bytes.clone(),
            harness_argv: argv,
        };
        assert!(audit_attempts(attempts.clone(), &policy).is_ok());
        let mut provenance = policy.clone();
        provenance.allowed_programs = provenance
            .expected
            .iter()
            .map(|rule| ExpectedProgram {
                program: rule.program.clone(),
                outcome: rule.outcome.clone(),
                minimum: rule.count,
                maximum: rule.count,
            })
            .collect();
        provenance.expected.clear();
        let proven = audit_attempts(attempts.clone(), &provenance).unwrap();
        assert!(proven.argv_scope.contains("without parity claim"));
        let mut changed_argv = attempts.clone();
        changed_argv[0]
            .argv
            .push(b"different-but-retained".to_vec());
        assert!(audit_attempts(changed_argv, &provenance).is_err());
        let mut with_failure = policy.clone();
        with_failure.expected.push(ExpectedAttempt {
            program: b"/missing/git".to_vec(),
            argv: vec![b"git".to_vec(), b"status".to_vec()],
            outcome: Outcome::Errno("ENOENT".into()),
            count: 1,
        });
        let missing = Attempt {
            pid: 2,
            line: 1,
            program: b"/missing/git".to_vec(),
            argv: vec![b"git".to_vec(), b"status".to_vec()],
            outcome: Outcome::Errno("ENOENT".into()),
        };
        let mut with_failed_attempt = attempts.clone();
        with_failed_attempt.push(missing.clone());
        let receipt = audit_attempts(with_failed_attempt.clone(), &with_failure).unwrap();
        assert_eq!((receipt.successful, receipt.failed), (5, 1));
        let permission_path = b"/owned/actions/permission_tmux/tools/tmux".to_vec();
        let mut permission = policy.clone();
        permission.expected.push(ExpectedAttempt {
            program: permission_path.clone(),
            argv: vec![permission_path.clone()],
            outcome: Outcome::Errno("EACCES".into()),
            count: 1,
        });
        let mut denied_permission = attempts.clone();
        denied_permission.push(Attempt {
            pid: 20,
            line: 1,
            program: permission_path,
            argv: vec![b"/owned/actions/permission_tmux/tools/tmux".to_vec()],
            outcome: Outcome::Errno("EACCES".into()),
        });
        assert!(audit_attempts(denied_permission, &permission).is_ok());
        let mut denied = missing;
        denied.outcome = Outcome::Errno("EACCES".into());
        *with_failed_attempt.last_mut().unwrap() = denied;
        assert!(audit_attempts(with_failed_attempt, &with_failure).is_err());
        let mut wrong = attempts.clone();
        wrong[0].argv.push(b"extra".to_vec());
        assert!(audit_attempts(wrong, &policy).is_err());
        let mut wrong = attempts.clone();
        wrong[0].outcome = Outcome::Errno("ENOENT".into());
        assert!(audit_attempts(wrong, &policy).is_err());
        let mut wrong = attempts;
        wrong[0].program = b"/usr/bin/python3".to_vec();
        assert!(audit_attempts(wrong, &policy).is_err());
    }

    #[test]
    fn program_seal_records_symlink_target_and_rejects_retargeting() {
        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("first");
        let second = dir.path().join("second");
        let alias = dir.path().join("alias");
        fs::write(&first, b"\x7fELFfirst").unwrap();
        fs::write(&second, b"\x7fELFfirst").unwrap();
        std::os::unix::fs::symlink("first", &alias).unwrap();
        let before = capture_program_seal(&alias).unwrap();
        assert_eq!(before.links.len(), 1);
        assert_eq!(before.links[0].target, b"first");
        fs::remove_file(&alias).unwrap();
        std::os::unix::fs::symlink("second", &alias).unwrap();
        assert_ne!(before, capture_program_seal(&alias).unwrap());
    }

    #[test]
    fn program_seal_covers_symlinks_inside_target_and_resolved_interpreter() {
        let dir = tempfile::tempdir().unwrap();
        let python = dir.path().join("python3");
        let same_bytes = dir.path().join("native");
        fs::write(&python, b"\x7fELFsame").unwrap();
        fs::write(&same_bytes, b"\x7fELFsame").unwrap();
        let nested = dir.path().join("nested");
        fs::create_dir(&nested).unwrap();
        let inner = nested.join("inner");
        std::os::unix::fs::symlink(&python, &inner).unwrap();
        let outer = dir.path().join("outer");
        std::os::unix::fs::symlink("nested/inner", &outer).unwrap();
        let before = capture_program_seal(&outer).unwrap();
        assert_eq!(before.links.len(), 2);
        assert_eq!(before.resolved, python.as_os_str().as_bytes());
        assert!(forbidden_interpreter(&before.resolved, &before.resolved));
        fs::remove_file(&inner).unwrap();
        std::os::unix::fs::symlink(&same_bytes, &inner).unwrap();
        let after = capture_program_seal(&outer).unwrap();
        assert_ne!(before, after, "nested link retargeting was not sealed");
        assert!(!forbidden_interpreter(&after.resolved, &after.resolved));
    }

    #[test]
    fn created_writer_receipt_requires_held_identity_and_exact_final_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("script");
        let receipt = dir.path().join("created.jsonl");
        fs::write(&script, b"#!/bin/sh\nexit 0\n").unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).unwrap();
        let guard = CreatedProgramGuard::record(&script, &receipt).unwrap();
        let first = fs::read_to_string(&receipt).unwrap();
        assert_eq!(first.lines().count(), 1);
        assert_eq!(
            serde_json::from_str::<Value>(&first).unwrap()["stage"],
            "created"
        );
        guard.finish().unwrap();
        let sealed = capture_program_seal(&script).unwrap();
        verify_created_receipt(&receipt, &sealed, 0o700).unwrap();

        let replacement = dir.path().join("replacement");
        fs::write(&replacement, b"#!/bin/sh\nexit 0\n").unwrap();
        fs::set_permissions(&replacement, fs::Permissions::from_mode(0o700)).unwrap();
        let second_receipt = dir.path().join("replaced.jsonl");
        let guard = CreatedProgramGuard::record(&script, &second_receipt).unwrap();
        fs::rename(&replacement, &script).unwrap();
        assert!(guard.finish().is_err());
        assert_eq!(
            fs::read_to_string(&second_receipt).unwrap().lines().count(),
            1
        );
    }

    #[test]
    fn created_writer_does_not_clobber_foreign_receipt_or_accept_same_length_mutation() {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("script");
        let receipt = dir.path().join("created.jsonl");
        fs::write(&script, b"#!/bin/sh\nexit 0\n").unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).unwrap();
        fs::write(&receipt, b"foreign receipt").unwrap();
        assert!(CreatedProgramGuard::record(&script, &receipt).is_err());
        assert_eq!(fs::read(&receipt).unwrap(), b"foreign receipt");
        fs::remove_file(&receipt).unwrap();
        let guard = CreatedProgramGuard::record(&script, &receipt).unwrap();
        fs::write(&script, b"#!/bin/sh\nexit 7\n").unwrap();
        assert!(guard.finish().is_err());
        assert_eq!(fs::read_to_string(&receipt).unwrap().lines().count(), 1);
    }

    #[test]
    fn temporary_launch_descriptor_is_one_bounded_nonexecuting_argv_shape() {
        assert!(launch_descriptor_path(b"/tmp/op-exec-pJpgwF/.tmphIfUAU"));
        for path in [
            b"/tmp/op-exec-pJpgwF/.tmphIfUAU/extra".as_slice(),
            b"/tmp/op-exec-pJpgwF/../../python".as_slice(),
            b"/tmp/op-exec-pJpgwF/.tmphIfUAU.py".as_slice(),
            b"/tmp/op-exec-pJpgwF/.tmphIfUA".as_slice(),
            b"/tmp/op-exec-pJpgwF/other".as_slice(),
            b"/tmp/op-exec-pJpgwF/.tmp\xffAAAA".as_slice(),
        ] {
            assert!(!launch_descriptor_path(path));
        }
    }

    #[test]
    fn created_provider_symlink_receipt_binds_target_and_inode() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("native-tmux");
        let alias = directory.path().join("tools-tmux");
        let receipt = directory.path().join("created-tmux.jsonl");
        fs::write(&target, b"\x7fELF-owned-provider").unwrap();
        std::os::unix::fs::symlink(&target, &alias).unwrap();
        let guard = CreatedSymlinkGuard::record(&alias, &target, &receipt).unwrap();
        guard.finish().unwrap();
        let seal = capture_program_seal(&alias).unwrap();
        let source = capture_program_seal(&target).unwrap();
        verify_created_symlink_receipt(&receipt, &seal, &source).unwrap();
        let wrong = directory.path().join("foreign-native");
        fs::write(&wrong, b"\x7fELF-foreign-provider").unwrap();
        fs::remove_file(&alias).unwrap();
        std::os::unix::fs::symlink(&wrong, &alias).unwrap();
        assert!(verify_created_symlink_receipt(&receipt, &seal, &source).is_err());
    }

    #[test]
    fn active_created_provider_guard_rejects_same_target_symlink_replacement() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("native-tmux");
        let alias = directory.path().join("tools-tmux");
        let receipt = directory.path().join("created-tmux.jsonl");
        fs::write(&target, b"\x7fELF-owned-provider").unwrap();
        std::os::unix::fs::symlink(&target, &alias).unwrap();
        let guard = CreatedSymlinkGuard::record(&alias, &target, &receipt).unwrap();
        fs::remove_file(&alias).unwrap();
        std::os::unix::fs::symlink(&target, &alias).unwrap();
        assert!(guard.finish().is_err());
        assert_eq!(fs::read_to_string(&receipt).unwrap().lines().count(), 1);
    }
}
