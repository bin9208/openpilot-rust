//! Cache successful results, including empty fallbacks, but never execution/decoding errors.
//!
//! Cwd keys preserve lexical path spelling. `None` means an explicit Python `None` argument;
//! the three `_default` variants model omitted cwd arguments used by `is_dirty`. `get_head`
//! and `get_head_date` model the source metadata callsites that omit the revision argument.
//! Rust callers do not expose Python's additional positional/keyword cache-key distinctions.
//! All fallible helpers return I/O/UTF-8/cache errors; nonzero Git exits become empty strings.
use crate::Error;
use std::{
    collections::HashMap,
    ffi::OsString,
    path::Path,
    process::{Command, Stdio},
    sync::{Mutex, OnceLock},
};

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
enum Operation {
    Commit(String),
    CommitDate(String),
    Head,
    HeadDate,
    ShortBranch,
    Branch,
    Origin,
    NormalizedOrigin,
    DefaultOrigin,
    DefaultShortBranch,
    DefaultBranch,
}
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct Key {
    operation: Operation,
    cwd: Option<OsString>,
}
static CACHE: OnceLock<Mutex<HashMap<Key, String>>> = OnceLock::new();

fn cached(
    operation: Operation,
    cwd: Option<&Path>,
    compute: impl FnOnce() -> Result<String, Error>,
) -> Result<String, Error> {
    let key = Key {
        operation,
        cwd: cwd.map(|path| path.as_os_str().to_os_string()),
    };
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(value) = cache.lock().map_err(|_| Error::CachePoisoned)?.get(&key) {
        return Ok(value.clone());
    }
    let value = compute()?;
    cache
        .lock()
        .map_err(|_| Error::CachePoisoned)?
        .insert(key, value.clone());
    Ok(value)
}

fn run(arguments: &[&str], cwd: Option<&Path>) -> Result<String, Error> {
    let mut command = Command::new("git");
    command.args(arguments).stderr(Stdio::inherit());
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    let output = command.output()?;
    // check_output decodes before checking exit status, even for a failed command.
    let text = String::from_utf8(output.stdout)?
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    if !output.status.success() {
        return Err(Error::GitExit(output.status));
    }
    Ok(text
        .trim_matches(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c))
        .into())
}
fn run_default(arguments: &[&str], cwd: Option<&Path>) -> Result<String, Error> {
    match run(arguments, cwd) {
        Err(Error::GitExit(_)) => Ok(String::new()),
        result => result,
    }
}
/// Resolve a revision; unsuccessful Git exit is cached as an empty string.
pub fn get_commit(cwd: Option<&Path>, branch: &str) -> Result<String, Error> {
    cached(Operation::Commit(branch.into()), cwd, || {
        run_default(&["rev-parse", branch], cwd)
    })
}
/// Source `get_commit(cwd)` call with its revision argument omitted.
pub fn get_head(cwd: Option<&Path>) -> Result<String, Error> {
    cached(Operation::Head, cwd, || {
        run_default(&["rev-parse", "HEAD"], cwd)
    })
}
/// Source `get_commit_date(cwd)` call with its commit argument omitted.
pub fn get_head_date(cwd: Option<&Path>) -> Result<String, Error> {
    cached(Operation::HeadDate, cwd, || {
        run_default(&["show", "--no-patch", "--format='%ct %ci'", "HEAD"], cwd)
    })
}
/// Return Git's timestamp/date, including the source's literal surrounding apostrophes.
pub fn get_commit_date(cwd: Option<&Path>, commit: &str) -> Result<String, Error> {
    cached(Operation::CommitDate(commit.into()), cwd, || {
        run_default(&["show", "--no-patch", "--format='%ct %ci'", commit], cwd)
    })
}
/// Read `rev-parse --abbrev-ref HEAD`, retaining `HEAD` when detached.
pub fn get_short_branch(cwd: Option<&Path>) -> Result<String, Error> {
    cached(Operation::ShortBranch, cwd, || {
        run_default(&["rev-parse", "--abbrev-ref", "HEAD"], cwd)
    })
}
/// Read the tracking branch, returning an empty fallback when no upstream is configured.
pub fn get_branch(cwd: Option<&Path>) -> Result<String, Error> {
    cached(Operation::Branch, cwd, || {
        run_default(
            &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"],
            cwd,
        )
    })
}
/// Prefer the current branch tracking remote, then fall back to remote.origin.url.
pub fn get_origin(cwd: Option<&Path>) -> Result<String, Error> {
    cached(Operation::Origin, cwd, || origin(cwd))
}
fn origin(cwd: Option<&Path>) -> Result<String, Error> {
    let tracked = || {
        let branch = run(&["name-rev", "--name-only", "HEAD"], cwd)?;
        let remote = run(&["config", &format!("branch.{branch}.remote")], cwd)?;
        run(&["config", &format!("remote.{remote}.url")], cwd)
    };
    match tracked() {
        Err(Error::GitExit(_)) => run_default(&["config", "--get", "remote.origin.url"], cwd),
        result => result,
    }
}
/// Source no-argument call: its cache is distinct from an explicit `None` cwd.
pub fn get_origin_default() -> Result<String, Error> {
    cached(Operation::DefaultOrigin, None, || origin(None))
}
/// Source `get_short_branch()` omitted-cwd cache entry.
pub fn get_short_branch_default() -> Result<String, Error> {
    cached(Operation::DefaultShortBranch, None, || {
        run_default(&["rev-parse", "--abbrev-ref", "HEAD"], None)
    })
}
/// Source `get_branch()` omitted-cwd cache entry.
pub fn get_branch_default() -> Result<String, Error> {
    cached(Operation::DefaultBranch, None, || {
        run_default(
            &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"],
            None,
        )
    })
}
/// Read and normalize the origin with a separate successful-result cache.
pub fn get_normalized_origin(cwd: Option<&Path>) -> Result<String, Error> {
    cached(Operation::NormalizedOrigin, cwd, || {
        Ok(normalize_origin(&get_origin(cwd)?))
    })
}
/// Preserve the source's ordered replace-once operations, including non-prefix matches.
pub fn normalize_origin(origin: &str) -> String {
    origin
        .replacen("git@", "", 1)
        .replacen(".git", "", 1)
        .replacen("https://", "", 1)
        .replacen(':', "/", 1)
}
