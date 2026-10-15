//! Stage source and its matching native package together; switch only at startup.
use crate::{activation::sync_directory, checkout_sha, identity, invalid, Options, Result};
use serde::{Deserialize, Serialize};
use std::{
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::Write,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    process::Command,
};

pub const ORIGIN: &str = "https://github.com/bin9208/openpilot-rust.git";
const READY: &str = ".runtime-update-ready.json";
const JOURNAL: &str = ".runtime-update-journal.json";
const PENDING_PREFIX: &str = ".pending-update-";
const PENDING_NONCE_LENGTH: usize = 6;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Ready {
    schema: u32,
    previous_commit: String,
    target_commit: String,
    nonce: String,
}
impl Ready {
    fn validate(&self) -> Result<()> {
        if self.schema != 1
            || !identity::hex(&self.previous_commit, 40)
            || !identity::hex(&self.target_commit, 40)
            || self.nonce.is_empty()
            || self.nonce.len() > 32
            || !self.nonce.bytes().all(|b| b.is_ascii_alphanumeric())
        {
            return Err(invalid("invalid native update identity"));
        }
        Ok(())
    }
    fn backup(&self, staging: &Path) -> PathBuf {
        staging.join(format!("old_openpilot.{}", self.nonce))
    }
}

pub struct StageOptions {
    root: PathBuf,
    source: Option<PathBuf>,
    target: String,
    staging: Option<PathBuf>,
    params: Option<PathBuf>,
    branch: Option<String>,
    detach: bool,
    ff_only: bool,
    offline: Option<(PathBuf, PathBuf)>,
    reserve: u64,
}
impl StageOptions {
    pub fn parse(mut args: impl Iterator<Item = OsString>) -> Result<Self> {
        let mut root = None;
        let mut source = None;
        let mut target = None;
        let mut staging = None;
        let mut params = None;
        let mut branch = None;
        let mut detach = false;
        let mut ff_only = false;
        let mut archive = None;
        let mut descriptor = None;
        let mut reserve = super::DEFAULT_RESERVE;
        while let Some(arg) = args.next() {
            match arg.to_str() {
                Some("--root") if root.is_none() => root = Some(PathBuf::from(super::value(&mut args)?)),
                Some("--source") if source.is_none() => source = Some(PathBuf::from(super::value(&mut args)?)),
                Some("--target") if target.is_none() => target = Some(text(super::value(&mut args)?)?),
                Some("--staging") if staging.is_none() => staging = Some(PathBuf::from(super::value(&mut args)?)),
                Some("--params-root") if params.is_none() => params = Some(PathBuf::from(super::value(&mut args)?)),
                Some("--branch") if branch.is_none() => branch = Some(text(super::value(&mut args)?)?),
                Some("--detach") if !detach => detach = true,
                Some("--ff-only") if !ff_only => ff_only = true,
                Some("--archive") if archive.is_none() => archive = Some(PathBuf::from(super::value(&mut args)?)),
                Some("--descriptor") if descriptor.is_none() => descriptor = Some(PathBuf::from(super::value(&mut args)?)),
                Some("--reserve-bytes") => reserve = text(super::value(&mut args)?)?.parse().map_err(|_| invalid("invalid reserve"))?,
                _ => return Err(invalid("usage: openpilot-runtime-install --stage-update --root ROOT --target REV [--source CHECKOUT] [--staging PATH] [--params-root PATH] [--branch NAME | --detach] [--archive PATH --descriptor PATH] [--reserve-bytes BYTES]")),
            }
        }
        if detach && branch.is_some() {
            return Err(invalid("--branch and --detach are mutually exclusive"));
        }
        let offline = match (archive, descriptor) {
            (Some(a), Some(d)) => Some((a, d)),
            (None, None) => None,
            _ => {
                return Err(invalid(
                    "--archive and --descriptor must be provided together",
                ))
            }
        };
        Ok(Self {
            root: root.ok_or_else(|| invalid("--root required"))?,
            source,
            target: target.ok_or_else(|| invalid("--target required"))?,
            staging,
            params,
            branch,
            detach,
            ff_only,
            offline,
            reserve,
        })
    }
}

fn text(value: OsString) -> Result<String> {
    value
        .into_string()
        .map_err(|_| invalid("non UTF8 update argument"))
}
fn git_command(root: &Path) -> Command {
    let mut command = Command::new("git");
    command
        .args([
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "core.fsmonitor=false",
        ])
        .env("GIT_LFS_SKIP_SMUDGE", "1")
        .current_dir(root);
    command
}
fn output(command: &mut Command) -> Result<String> {
    let output = command.output()?;
    if !output.status.success() {
        return Err(invalid(format!(
            "native update Git command failed: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    String::from_utf8(output.stdout).map_err(|_| invalid("Git returned non UTF8 output"))
}
fn git(root: &Path, args: &[&str]) -> Result<String> {
    output(git_command(root).args(args))
}
fn supported_origin(value: &str) -> bool {
    matches!(
        value.trim(),
        ORIGIN
            | "https://github.com/bin9208/openpilot-rust"
            | "git@github.com:bin9208/openpilot-rust.git"
            | "ssh://git@github.com/bin9208/openpilot-rust.git"
    )
}
fn params_root(root: &Path) -> PathBuf {
    std::env::var_os("PARAMS_ROOT").map_or_else(
        || root.parent().unwrap_or(Path::new("/data")).join("params"),
        PathBuf::from,
    )
}
fn offroad(params: &Path) -> Result<()> {
    if fs::read(params.join("d/IsOnroad")).ok().as_deref() != Some(b"0") {
        return Err(invalid(
            "native source/package updates require verified offroad state",
        ));
    }
    Ok(())
}
fn acquire(staging: &Path) -> Result<File> {
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(staging.join(".runtime-update.lock"))?;
    lock.try_lock()
        .map_err(|e| invalid(format!("native update transaction is busy: {e}")))?;
    Ok(lock)
}
fn pending_nonce(name: &str) -> Option<&str> {
    name.strip_prefix(PENDING_PREFIX).filter(|nonce| {
        nonce.len() == PENDING_NONCE_LENGTH && nonce.bytes().all(|b| b.is_ascii_alphanumeric())
    })
}
fn directory_identity(path: &Path) -> Result<(u64, u64)> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_dir() {
        return Err(invalid(format!(
            "native checkout/staging must be a real directory: {}",
            path.display()
        )));
    }
    Ok((metadata.dev(), metadata.ino()))
}
fn unchanged_directory(path: &Path, expected: (u64, u64)) -> Result<()> {
    if directory_identity(path)? != expected {
        return Err(invalid(format!(
            "native update directory changed during verification: {}",
            path.display()
        )));
    }
    Ok(())
}

// A killed preparer cannot run TempDir::drop. The shared staging lock proves
// no cooperative preparer still owns these unpublished directories. Never follow
// a symlink, and leave ordinary files and complete rollback checkouts untouched.
fn clean_pending(staging: &Path) -> Result<()> {
    for entry in fs::read_dir(staging)? {
        let entry = entry?;
        if entry.file_name().to_str().and_then(pending_nonce).is_some()
            && entry.file_type()?.is_dir()
        {
            fs::remove_dir_all(entry.path())?;
        }
    }
    sync_directory(staging)
}

fn write_json(path: &Path, ready: &Ready) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| invalid("update marker has no parent"))?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(&serde_json::to_vec(ready)?)?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|e| e.error)?;
    sync_directory(parent)
}
fn read_ready(root: &Path) -> Result<Ready> {
    let ready: Ready = identity::read_json(&root.join(READY))?;
    ready.validate()?;
    Ok(ready)
}
fn verify(root: &Path, ready: &Ready) -> Result<()> {
    directory_identity(root)?;
    ready.validate()?;
    if checkout_sha(root)? != ready.target_commit
        || !supported_origin(&git(root, &["config", "--get", "remote.origin.url"])?)
    {
        return Err(invalid(
            "staged source does not match the verified target and origin",
        ));
    }
    if !fs::symlink_metadata(root.join(".git"))?.is_dir()
        || root.join(".git/objects/info/alternates").exists()
    {
        return Err(invalid("staged checkout must have independent Git objects"));
    }
    let changes = git(root, &["diff", "--name-only", "-z", "HEAD", "--"])?;
    for path in changes.split('\0').filter(|path| !path.is_empty()) {
        if !identity::source_asset(Path::new(path)) {
            return Err(invalid(format!("staged tracked source changed: {path}")));
        }
    }
    super::check_installed(root, &ready.target_commit)
}
fn tree_bytes(root: &Path) -> Result<u64> {
    let mut bytes = 0_u64;
    let mut pending = vec![root.to_path_buf()];
    while let Some(path) = pending.pop() {
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.is_dir() {
            for entry in fs::read_dir(&path)? {
                pending.push(entry?.path());
            }
        } else {
            bytes = bytes
                .checked_add(metadata.len())
                .ok_or_else(|| invalid("source size overflow"))?;
        }
    }
    Ok(bytes)
}
fn sync_tree(root: &Path) -> Result<()> {
    let mut pending = vec![root.to_path_buf()];
    let mut directories = Vec::new();
    while let Some(path) = pending.pop() {
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.is_dir() {
            for entry in fs::read_dir(&path)? {
                pending.push(entry?.path());
            }
            directories.push(path);
        } else if metadata.is_file() {
            File::open(path)?.sync_all()?;
        }
    }
    for directory in directories.iter().rev() {
        sync_directory(directory)?;
    }
    Ok(())
}

// Source exchange moves the old executable store to a rollback path. Its live
// daemons would no longer be provably same-store owners to the new bootstrap.
// Refuse that exchange; only this boot-locked launcher may cross it and hand off.
fn require_fresh_start(root: &Path) -> Result<()> {
    let root = root.canonicalize()?;
    let mut inaccessible = false;
    for entry in fs::read_dir("/proc")? {
        let entry = entry?;
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<u32>().ok())
        else {
            continue;
        };
        let executable = match fs::read_link(entry.path().join("exe")) {
            Ok(path) => path,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
                inaccessible = true;
                continue;
            }
            Err(error) => return Err(error.into()),
        };
        let raw = executable.as_os_str().as_encoded_bytes();
        let raw = raw.strip_suffix(b" (deleted)").unwrap_or(raw);
        let executable =
            Path::new(<std::ffi::OsStr as std::os::unix::ffi::OsStrExt>::from_bytes(raw));
        if !executable.starts_with(&root) {
            continue;
        }
        if pid == std::process::id()
            && executable
                .file_name()
                .is_some_and(|name| name == "openpilot-launch")
        {
            continue;
        }
        return Err(invalid(format!(
            "live old runtime owner {pid} at {} prevents source exchange; reboot required",
            executable.display()
        )));
    }
    // Ordinary /proc access cannot establish the executable identity of another
    // security domain. Do not strand every startup on an unrelated OS process;
    // production activation remains a normal reboot, with the boot lock held.
    if inaccessible {
        eprintln!("native activation inventory could not inspect some OS processes; normal reboot boundary remains required");
    }
    Ok(())
}

/// Prepare an immutable source/package pair without changing active HEAD or bins.
pub fn prepare(options: StageOptions, download: &impl super::Download) -> Result<String> {
    let root = options.root.canonicalize()?;
    let params = options.params.unwrap_or_else(|| params_root(&root));
    offroad(&params)?;
    if !supported_origin(&git(&root, &["config", "--get", "remote.origin.url"])?) {
        return Err(invalid("origin must be bin9208/openpilot-rust"));
    }
    let previous = checkout_sha(&root)?;
    let source = options
        .source
        .unwrap_or_else(|| root.clone())
        .canonicalize()?;
    if !supported_origin(&git(&source, &["config", "--get", "remote.origin.url"])?) {
        return Err(invalid(
            "staged source origin must be bin9208/openpilot-rust",
        ));
    }
    let target = git(
        &source,
        &[
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{}^{{commit}}", options.target),
        ],
    )?
    .trim()
    .to_owned();
    if !identity::hex(&target, 40) {
        return Err(invalid("target is not an exact source commit"));
    }
    if options.ff_only {
        git(&root, &["merge-base", "--is-ancestor", &previous, &target])?;
    }
    let staging = options
        .staging
        .or_else(|| std::env::var_os("STAGING_ROOT").map(PathBuf::from))
        .unwrap_or_else(|| {
            root.parent()
                .unwrap_or(Path::new("/data"))
                .join("safe_staging")
        });
    fs::create_dir_all(&staging)?;
    let staging = staging.canonicalize()?;
    if staging.starts_with(&root)
        || root.starts_with(&staging)
        || fs::metadata(&root)?.dev() != fs::metadata(&staging)?.dev()
    {
        return Err(invalid(
            "source and staging must be separate directories on the same filesystem",
        ));
    }
    let _lock = acquire(&staging)?;
    if staging.join(JOURNAL).try_exists()? {
        return Err(invalid(
            "pending activation journal requires startup recovery",
        ));
    }
    clean_pending(&staging)?;
    let finalized = staging.join("finalized");
    if finalized.try_exists()? {
        let ready = read_ready(&finalized)?;
        if ready.target_commit != target || ready.previous_commit != previous {
            return Err(invalid(
                "another complete native update is pending; reboot before preparing another",
            ));
        }
        verify(&finalized, &ready)?;
        return Ok(target);
    }
    let common = PathBuf::from(
        git(
            &source,
            &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        )?
        .trim(),
    );
    let source_bytes = git(&source, &["ls-tree", "-r", "-l", &target])?
        .lines()
        .try_fold(0_u64, |bytes, line| {
            let size = line
                .split_whitespace()
                .nth(3)
                .and_then(|size| size.parse::<u64>().ok())
                .unwrap_or(0);
            bytes
                .checked_add(size)
                .ok_or_else(|| invalid("source size overflow"))
        })?;
    super::ensure_space(
        &staging,
        options.reserve,
        tree_bytes(&common)?
            .checked_add(source_bytes)
            .ok_or_else(|| invalid("source size overflow"))?,
    )?;
    let pending = tempfile::Builder::new()
        .prefix(PENDING_PREFIX)
        .rand_bytes(PENDING_NONCE_LENGTH)
        .tempdir_in(&staging)?;
    let candidate = pending.path().join("checkout");
    output(
        git_command(&source)
            .args([
                "clone",
                "--no-hardlinks",
                "--no-checkout",
                "--dissociate",
                "--",
            ])
            .arg(&source)
            .arg(&candidate),
    )?;
    // A local clone invents origin refs from local heads. They are not the
    // upstream branch inventory used by maintenance/UI, so replace that namespace
    // with the source's actual fetched origin refs before selecting a branch.
    git(&candidate, &["remote", "remove", "origin"])?;
    git(&candidate, &["remote", "add", "origin", ORIGIN])?;
    let upstreams = git(
        &source,
        &[
            "for-each-ref",
            "--format=%(objectname)%09%(refname)%09%(symref)",
            "refs/remotes/origin/",
        ],
    )?;
    for symbolic in [false, true] {
        for line in upstreams.lines() {
            let fields: Vec<_> = line.split('\t').collect();
            if fields.len() != 3
                || !identity::hex(fields[0], 40)
                || !fields[1].starts_with("refs/remotes/origin/")
                || (!fields[2].is_empty() && !fields[2].starts_with("refs/remotes/origin/"))
            {
                return Err(invalid("invalid upstream branch identity"));
            }
            if symbolic && !fields[2].is_empty() {
                git(&candidate, &["symbolic-ref", fields[1], fields[2]])?;
            } else if !symbolic && fields[2].is_empty() {
                git(&candidate, &["update-ref", fields[1], fields[0]])?;
            }
        }
    }
    // Clone normally turns the other local branches into remote-tracking refs.
    // Keep the user's local branch names and commits in the new active checkout.
    let heads = git(
        &root,
        &[
            "for-each-ref",
            "--format=%(objectname)%09%(refname)",
            "refs/heads/",
        ],
    )?;
    for line in heads.lines() {
        let (sha, reference) = line
            .split_once('\t')
            .ok_or_else(|| invalid("invalid source branch reference"))?;
        if !identity::hex(sha, 40) || !reference.starts_with("refs/heads/") {
            return Err(invalid("invalid source branch identity"));
        }
        git(&candidate, &["update-ref", reference, sha])?;
    }

    // The durable selection stamp is generated state, not a local source edit.
    let mut exclude = OpenOptions::new()
        .create(true)
        .append(true)
        .open(candidate.join(".git/info/exclude"))?;
    writeln!(exclude, "\n/{READY}")?;
    drop(exclude);
    let implicit_branch = options.branch.is_none();
    let branch = if options.detach {
        None
    } else {
        options.branch.or_else(|| {
            git(&root, &["branch", "--show-current"])
                .ok()
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty())
        })
    };
    if let Some(branch) = branch {
        git(&candidate, &["check-ref-format", "--branch", &branch])?;
        git(&candidate, &["checkout", "-B", &branch, &target])?;
        // An implicit update follows the active branch's configured origin
        // upstream, which need not have the same name as that local branch.
        let merge = if implicit_branch
            && git(
                &root,
                &["config", "--get", &format!("branch.{branch}.remote")],
            )
            .is_ok_and(|remote| remote.trim() == "origin")
        {
            git(
                &root,
                &["config", "--get", &format!("branch.{branch}.merge")],
            )
            .ok()
            .map(|merge| merge.trim().to_owned())
            .filter(|merge| merge.starts_with("refs/heads/"))
        } else {
            None
        }
        .unwrap_or_else(|| format!("refs/heads/{branch}"));
        git(
            &candidate,
            &["config", &format!("branch.{branch}.remote"), "origin"],
        )?;
        git(
            &candidate,
            &["config", &format!("branch.{branch}.merge"), &merge],
        )?;
    } else {
        git(&candidate, &["checkout", "--detach", &target])?;
    }
    let offline = options.offline.or_else(|| {
        if target == previous {
            super::repair_package(&root, &target)
        } else {
            None
        }
    });
    super::run(
        &Options {
            root: candidate.clone(),
            check: false,
            offline,
            reserve_bytes: options.reserve,
        },
        download,
    )?;
    let nonce = pending
        .path()
        .file_name()
        .and_then(|s| s.to_str())
        .and_then(pending_nonce)
        .ok_or_else(|| invalid("invalid update transaction nonce"))?
        .to_owned();
    let ready = Ready {
        schema: 1,
        previous_commit: previous,
        target_commit: target.clone(),
        nonce,
    };
    publish_ready(&root, &candidate, &params, &ready)?;
    fs::rename(&candidate, &finalized)?;
    sync_directory(&staging)?;
    Ok(target)
}

fn publish_ready(root: &Path, candidate: &Path, params: &Path, ready: &Ready) -> Result<()> {
    verify(candidate, ready)?;
    if checkout_sha(root)? != ready.previous_commit {
        return Err(invalid("active source changed while preparing update"));
    }
    offroad(params)?;
    sync_tree(candidate)?;
    write_json(&candidate.join(READY), ready)?;
    File::create(candidate.join(".overlay_consistent"))?.sync_all()?;
    sync_directory(candidate)
}

/// Verify an independently finalized legacy updater checkout through the same
/// native package gate and durable selection stamp before startup activation.
pub fn adopt(options: StageOptions, download: &impl super::Download) -> Result<String> {
    let root = options.root.canonicalize()?;
    let params = options.params.unwrap_or_else(|| params_root(&root));
    offroad(&params)?;
    if !supported_origin(&git(&root, &["config", "--get", "remote.origin.url"])?) {
        return Err(invalid("origin must be bin9208/openpilot-rust"));
    }
    let staging = options
        .staging
        .or_else(|| std::env::var_os("STAGING_ROOT").map(PathBuf::from))
        .unwrap_or_else(|| {
            root.parent()
                .unwrap_or(Path::new("/data"))
                .join("safe_staging")
        })
        .canonicalize()?;
    if staging.starts_with(&root)
        || root.starts_with(&staging)
        || fs::metadata(&root)?.dev() != fs::metadata(&staging)?.dev()
    {
        return Err(invalid(
            "source and staging must be separate directories on the same filesystem",
        ));
    }
    let _lock = acquire(&staging)?;
    if staging.join(JOURNAL).try_exists()? {
        return Err(invalid(
            "pending activation journal requires startup recovery",
        ));
    }
    clean_pending(&staging)?;
    let candidate = staging.join("finalized");
    let target = checkout_sha(&candidate)?;
    let previous = checkout_sha(&root)?;
    if candidate.join(READY).try_exists()? {
        let ready = read_ready(&candidate)?;
        if ready.target_commit != target || ready.previous_commit != previous {
            return Err(invalid("another native update stamp already exists"));
        }
        verify(&candidate, &ready)?;
        return Ok(target);
    }
    match fs::remove_file(candidate.join(".overlay_consistent")) {
        Ok(()) => sync_directory(&candidate)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
        Err(error) => return Err(error.into()),
    }
    super::run(
        &Options {
            root: candidate.clone(),
            check: false,
            offline: options.offline,
            reserve_bytes: options.reserve,
        },
        download,
    )?;
    let token = tempfile::Builder::new()
        .prefix(".update-token-")
        .tempfile_in(&staging)?;
    let nonce = token
        .path()
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_prefix(".update-token-"))
        .ok_or_else(|| invalid("invalid update token"))?
        .to_owned();
    let ready = Ready {
        schema: 1,
        previous_commit: previous,
        target_commit: target.clone(),
        nonce,
    };
    publish_ready(&root, &candidate, &params, &ready)?;
    Ok(target)
}

/// Called by the native launcher before starting children, under its boot lock.
/// Atomic exchange keeps a complete checkout at the normal startup path at every
/// crash point. No nonatomic rename fallback is permitted on other filesystems.
pub fn activate(root: &Path, staging: &Path, params: &Path) -> Result<bool> {
    if !staging.try_exists()? {
        return Ok(false);
    }
    let root_identity = directory_identity(root)?;
    let staging_identity = directory_identity(staging)?;
    let physical_root = root.canonicalize()?;
    let physical_staging = staging.canonicalize()?;
    if physical_root.starts_with(&physical_staging)
        || physical_staging.starts_with(&physical_root)
        || root_identity.0 != staging_identity.0
    {
        return Err(invalid(
            "source and staging must be separate real directories on the same filesystem",
        ));
    }
    let _lock = acquire(staging)?;
    let journal = staging.join(JOURNAL);
    let finalized = staging.join("finalized");
    let pending = match fs::symlink_metadata(&journal) {
        Ok(metadata) if metadata.is_file() => Some(identity::read_json::<Ready>(&journal)?),
        Ok(_) => return Err(invalid("native update journal is not a regular file")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e.into()),
    };
    let recovering = pending.is_some();
    let ready = match pending {
        Some(ready) => ready,
        None => {
            if !finalized.join(READY).try_exists()? {
                return Ok(false);
            }
            read_ready(&finalized)?
        }
    };
    ready.validate()?;
    let exchanged = read_ready(root).is_ok_and(|actual| actual == ready);
    if !exchanged {
        if checkout_sha(root)? != ready.previous_commit {
            if recovering {
                return Err(invalid(
                    "active source changed during native update recovery",
                ));
            }
            eprintln!("native update postponed: active source changed since staging");
            return Ok(false);
        }
        if offroad(params).is_err() {
            eprintln!("native update postponed until verified offroad startup");
            return Ok(false);
        }
        if read_ready(&finalized)? != ready {
            return Err(invalid("native update readiness does not match journal"));
        }
        let candidate_identity = directory_identity(&finalized)?;
        verify(&finalized, &ready)?;
        require_fresh_start(root)?;
        if !recovering {
            write_json(&journal, &ready)?;
        }
        // Recheck namespace identity immediately before exchange. The owned
        // transaction/boot locks exclude cooperating writers; do not exchange a
        // checkout or staging path replaced during Git/package verification.
        unchanged_directory(root, root_identity)?;
        unchanged_directory(staging, staging_identity)?;
        unchanged_directory(&finalized, candidate_identity)?;
        rustix::fs::renameat_with(
            rustix::fs::CWD,
            root,
            rustix::fs::CWD,
            &finalized,
            rustix::fs::RenameFlags::EXCHANGE,
        )
        .map_err(std::io::Error::from)?;
        sync_directory(
            root.parent()
                .ok_or_else(|| invalid("active root has no parent"))?,
        )?;
        sync_directory(staging)?;
    }
    verify(root, &ready)?;
    let backup = ready.backup(staging);
    if finalized.try_exists()? {
        if backup.try_exists()? || checkout_sha(&finalized)? != ready.previous_commit {
            return Err(invalid(
                "conflicting native update backup; preserving both checkouts",
            ));
        }
        fs::rename(&finalized, &backup)?;
        sync_directory(staging)?;
    } else if !backup.is_dir() {
        return Err(invalid("native update backup is missing"));
    }
    fs::remove_file(journal)?;
    sync_directory(staging)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn changed_directory_identity_is_rejected_without_removing_either_checkout() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root");
        let original = temp.path().join("original");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("source"), "old source").unwrap();
        let identity = directory_identity(&root).unwrap();
        fs::rename(&root, &original).unwrap();
        fs::create_dir(&root).unwrap();
        fs::write(root.join("source"), "replacement source").unwrap();
        assert!(unchanged_directory(&root, identity).is_err());
        assert_eq!(
            fs::read_to_string(original.join("source")).unwrap(),
            "old source"
        );
        assert_eq!(
            fs::read_to_string(root.join("source")).unwrap(),
            "replacement source"
        );
    }
}
