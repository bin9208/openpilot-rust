//! Rust-owned Git fixtures for the action responses. The historical controller
//! remains the independent oracle, but its repositories are never reused here.

use super::{HttpPeer, OwnedServer, Pin, Result};
use crate::qa_process::{CommandSpec, NativeRunner, Runner};
use rustix::process::Signal;
use serde_json::{json, Value};
use std::{
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::Write,
    net::TcpListener,
    os::unix::fs::{symlink, MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::Mutex,
    time::Duration,
};

type OriginalObservation<'a> = dyn Fn(&str) -> Result<Value> + 'a;

pub(super) struct OwnedExecutablePair {
    candidate_source: PathBuf,
    child_source: PathBuf,
    candidate: PathBuf,
    process_child: PathBuf,
    source_candidate: Pin,
    source_child: Pin,
    alias_candidate: Pin,
    alias_child: Pin,
    receipt_path: PathBuf,
    receipt: Mutex<File>,
}

impl OwnedExecutablePair {
    pub(super) fn create(candidate: &Path, process_child: &Path, output: &Path) -> Result<Self> {
        let bin = output.join("native-bin");
        fs::create_dir(&bin)?;
        let owned_candidate = bin.join("recovery_actions");
        let owned_child = bin.join("openpilot-process-child");
        let source_candidate = Pin::open(candidate, 256 * 1024 * 1024)?;
        let source_child = Pin::open(process_child, 256 * 1024 * 1024)?;
        fs::hard_link(candidate, &owned_candidate)?;
        fs::hard_link(process_child, &owned_child)?;
        let receipt_path = output.join("owned-native-bin-bindings.jsonl");
        let receipt = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&receipt_path)?;
        let pair = Self {
            candidate_source: candidate.to_owned(),
            child_source: process_child.to_owned(),
            candidate: owned_candidate.clone(),
            process_child: owned_child.clone(),
            source_candidate,
            source_child,
            alias_candidate: Pin::open(&owned_candidate, 256 * 1024 * 1024)?,
            alias_child: Pin::open(&owned_child, 256 * 1024 * 1024)?,
            receipt_path,
            receipt: Mutex::new(receipt),
        };
        pair.record("created")?;
        Ok(pair)
    }

    pub(super) fn candidate(&self) -> &Path {
        &self.candidate
    }

    pub(super) fn process_child(&self) -> &Path {
        &self.process_child
    }

    pub(super) fn record(&self, stage: &str) -> Result<()> {
        for pin in [
            &self.source_candidate,
            &self.source_child,
            &self.alias_candidate,
            &self.alias_child,
        ] {
            pin.verify()?;
        }
        let binding = |source: &Pin,
                       alias: &Pin,
                       source_path: &Path,
                       alias_path: &Path|
         -> Result<Value> {
            let original = fs::symlink_metadata(source_path)?;
            let linked = fs::symlink_metadata(alias_path)?;
            if original.dev() != linked.dev()
                || original.ino() != linked.ino()
                || original.len() != linked.len()
                || source.digest() != alias.digest()
            {
                return Err("owned native executable alias identity changed".into());
            }
            Ok(
                json!({"source":source_path.display().to_string(),"alias":alias_path.display().to_string(),"dev":original.dev(),
                "ino":original.ino(),"len":original.len(),"sha256":source.digest()}),
            )
        };
        let candidate = binding(
            &self.source_candidate,
            &self.alias_candidate,
            &self.candidate_source,
            &self.candidate,
        )?;
        let child = binding(
            &self.source_child,
            &self.alias_child,
            &self.child_source,
            &self.process_child,
        )?;
        let mut receipt = self
            .receipt
            .lock()
            .map_err(|_| "owned executable receipt lock poisoned")?;
        let held = receipt.metadata()?;
        let named = fs::symlink_metadata(&self.receipt_path)?;
        if !named.is_file() || held.dev() != named.dev() || held.ino() != named.ino() {
            return Err("owned executable receipt identity changed".into());
        }
        serde_json::to_writer(
            &mut *receipt,
            &json!({"stage":stage,"candidate":candidate,"process_child":child}),
        )?;
        receipt.write_all(b"\n")?;
        receipt.flush()?;
        Ok(())
    }
}

fn command(
    argv: Vec<String>,
    cwd: &Path,
    environment: &[(&str, String)],
) -> Result<crate::qa_process::Captured> {
    let mut spec = CommandSpec::new(argv, Duration::from_secs(30));
    spec.cwd = Some(cwd.to_owned());
    for (key, value) in environment {
        spec.env.insert((*key).into(), value.clone());
    }
    spec.stdout_limit = 1024 * 1024;
    spec.stderr_limit = 1024 * 1024;
    NativeRunner.run(&spec)
}

pub(super) fn git(cwd: &Path, args: &[&str]) -> Result<String> {
    let mut argv = vec!["/usr/bin/git".into()];
    argv.extend(args.iter().map(|arg| (*arg).to_owned()));
    let output = command(argv, cwd, &[])?;
    if output.code != 0 {
        return Err(format!(
            "owned Git fixture failed: {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

fn object(bare: &Path, kind: &str, body: &[u8], expected: &str) -> Result<()> {
    let mut spec = CommandSpec::new(
        vec![
            "/usr/bin/git".into(),
            "hash-object".into(),
            "-w".into(),
            "--stdin".into(),
            "-t".into(),
            kind.into(),
        ],
        Duration::from_secs(30),
    );
    spec.cwd = Some(bare.to_owned());
    spec.input = Some(body.to_vec());
    let result = NativeRunner.run(&spec)?;
    if result.code != 0 || String::from_utf8(result.stdout)?.trim() != expected {
        return Err("source-derived Git object identity changed".into());
    }
    Ok(())
}

fn object_id_bytes(id: &str) -> Result<[u8; 20]> {
    if id.len() != 40 {
        return Err("invalid source Git object id".into());
    }
    let mut bytes = [0; 20];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&id[index * 2..index * 2 + 2], 16)?;
    }
    Ok(bytes)
}

/// Recreate the source's two-commit data fixture from its independently
/// captured object fields. Every object must hash to the original archive ID;
/// no historical checkout, hook, helper, or Python source is copied.
pub(super) fn frozen_bare(output: &Path) -> Result<PathBuf> {
    const INITIAL: &str = "669014cb9efb52f3c16e4fd45987db291de37d61";
    const ADVANCED: &str = "db38cc4abed36989964460e61724f62ead60b9a8";
    const INITIAL_BLOB: &str = "e79c5e8f964493290a409888d5413a737e8e5dd5";
    const ADVANCED_BLOB: &str = "d786f998a4e7f2f89c57a552dad18a377e06383a";
    const INITIAL_TREE: &str = "91251e1e364089d034ee30a48f18b10b79527ca1";
    const ADVANCED_TREE: &str = "9cd82ef0b2511f642255f0a2b066bd7529557f4d";
    let bare = output.join("source-data.git");
    git(
        output,
        &[
            "init",
            "--bare",
            "--initial-branch=main",
            bare.to_str().ok_or("non-UTF8 frozen Git path")?,
        ],
    )?;
    object(&bare, "blob", b"initial\n", INITIAL_BLOB)?;
    object(&bare, "blob", b"remote update\n", ADVANCED_BLOB)?;
    let mut initial_tree = b"100644 tracked\0".to_vec();
    initial_tree.extend_from_slice(&object_id_bytes(INITIAL_BLOB)?);
    object(&bare, "tree", &initial_tree, INITIAL_TREE)?;
    let mut advanced_tree = b"100644 advanced\0".to_vec();
    advanced_tree.extend_from_slice(&object_id_bytes(ADVANCED_BLOB)?);
    advanced_tree.extend_from_slice(b"100644 tracked\0");
    advanced_tree.extend_from_slice(&object_id_bytes(INITIAL_BLOB)?);
    object(&bare, "tree", &advanced_tree, ADVANCED_TREE)?;
    let initial_commit = format!("tree {INITIAL_TREE}\nauthor Recovery Fixture <recovery-fixture@example.invalid> 1792607520 +0000\ncommitter Recovery Fixture <recovery-fixture@example.invalid> 1792607520 +0000\n\ninitial recovery fixture 복구\n");
    object(&bare, "commit", initial_commit.as_bytes(), INITIAL)?;
    let advanced_commit = format!("tree {ADVANCED_TREE}\nparent {INITIAL}\nauthor Recovery Fixture <recovery-fixture@example.invalid> 1792607521 +0000\ncommitter Recovery Fixture <recovery-fixture@example.invalid> 1792607521 +0000\n\nadvance selected upstream\n");
    object(&bare, "commit", advanced_commit.as_bytes(), ADVANCED)?;
    git(&bare, &["update-ref", "refs/heads/topic", INITIAL])?;
    git(&bare, &["update-ref", "refs/heads/main", ADVANCED])?;
    git(&bare, &["fsck", "--full", "--strict", "--no-reflogs"])?;
    if git(&bare, &["rev-parse", &format!("{ADVANCED}^@")])? != INITIAL
        || git(&bare, &["show", &format!("{INITIAL}:tracked")])? != "initial"
        || git(&bare, &["show", &format!("{ADVANCED}:advanced")])? != "remote update"
    {
        return Err("frozen Git commit relation changed".into());
    }
    Ok(bare)
}

fn prepare(repo: &Path, name: &str) -> Result<()> {
    match name {
        "git_reset" | "git_pull" | "git_reset_repo" => {
            fs::write(repo.join("tracked"), "locally modified\n")?
        }
        "git_sync" => {
            git(repo, &["branch", "spare"])?;
        }
        "git_checkout_local" => {
            git(repo, &["branch", "local"])?;
        }
        "git_pull_repair" => {
            git(
                repo,
                &[
                    "config",
                    "--replace-all",
                    "remote.origin.fetch",
                    "+refs/heads/gone:refs/remotes/origin/gone",
                ],
            )?;
            git(repo, &["config", "branch.main.merge", "refs/heads/gone"])?;
        }
        "git_pull_preserve" => {
            git(repo, &["config", "branch.main.merge", "refs/heads/topic"])?;
        }
        "git_pull_multiple" => {
            git(
                repo,
                &["config", "--add", "branch.main.merge", "refs/heads/gone"],
            )?;
        }
        "git_pull_missing" => {
            git(repo, &["switch", "-c", "absent"])?;
        }
        "stale_index" | "fresh_index" | "symlink_index" => {
            let lock = repo.join(".git/index.lock");
            if name == "symlink_index" {
                symlink(repo.join("tracked"), lock)?;
            } else {
                fs::write(&lock, "owned fixture lock\n")?;
                if name == "stale_index" {
                    let output = command(
                        vec![
                            "/usr/bin/touch".into(),
                            "-d".into(),
                            "2 minutes ago".into(),
                            lock.display().to_string(),
                        ],
                        repo,
                        &[],
                    )?;
                    if output.code != 0 {
                        return Err("owned stale Git lock setup failed".into());
                    }
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn snapshot(repo: &Path) -> Result<Value> {
    Ok(json!({
        "head":git(repo,&["rev-parse","HEAD"])?,
        "branch":git(repo,&["branch","--show-current"])?,
        "status":git(repo,&["status","--short","--untracked-files=all"])?,
        "tracked":fs::read_to_string(repo.join("tracked"))?,
        "remote":git(repo,&["config","--get","remote.origin.url"])?,
        "refs":git(repo,&["for-each-ref","--format=%(refname) %(objectname) %(upstream)"])?,
        "fetch":git(repo,&["config","--get-all","remote.origin.fetch"])?.lines().map(str::to_owned).collect::<Vec<_>>(),
    }))
}

fn rebuild_setup(repo: &Path, output: &Path) -> Result<(PathBuf, PathBuf)> {
    let cache = output.join("cache");
    fs::create_dir(&cache)?;
    fs::write(cache.join("old"), "remove me")?;
    for name in ["prebuilt", ".sconsign.dblite"] {
        fs::write(repo.join(name), "remove me")?;
    }
    fs::write(repo.join("rust/runtime-manifest.json"), "old package")?;
    let tools = output.join("tools");
    fs::create_dir(&tools)?;
    for (name, marker) in [("scons", "scons.argv"), ("sudo", "sudo.argv")] {
        let path = tools.join(name);
        fs::write(
            &path,
            format!("#!/bin/sh\nprintf '%s\\n' \"$*\" > {marker}\n"),
        )?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700))?;
    }
    Ok((cache, tools))
}

fn tmux_command(tmux: &Path, socket: &Path, args: &[&str], cwd: &Path) -> Result<String> {
    let mut argv = vec![
        tmux.display().to_string(),
        "-S".into(),
        socket.display().to_string(),
    ];
    argv.extend(args.iter().map(|value| (*value).to_owned()));
    let mut spec = CommandSpec::new(
        argv,
        if args.first() == Some(&"wait-for") {
            Duration::from_secs(5)
        } else {
            Duration::from_secs(30)
        },
    );
    spec.cwd = Some(cwd.to_owned());
    spec.stdout_limit = 1024 * 1024;
    spec.stderr_limit = 1024 * 1024;
    let observed = NativeRunner.run(&spec)?;
    if observed.code != 0 {
        return Err(format!(
            "owned tmux command failed: {args:?}: {}",
            String::from_utf8_lossy(&observed.stderr)
        )
        .into());
    }
    Ok(String::from_utf8(observed.stdout)?.trim().to_owned())
}

struct TmuxLease<'a> {
    binary: &'a Path,
    socket: &'a Path,
    cwd: &'a Path,
}
impl Drop for TmuxLease<'_> {
    fn drop(&mut self) {
        let _ = tmux_command(self.binary, self.socket, &["kill-server"], self.cwd);
    }
}

pub(super) fn capture_tmux(
    repository: &Path,
    candidate: &Path,
    process_child: &Path,
    tmux: &Path,
    original_bare: &Path,
    original_capture: &Path,
    output: &Path,
) -> Result<Value> {
    fs::create_dir(output)?;
    let checkout = output.join("repo");
    git(
        output,
        &[
            "clone",
            original_bare
                .to_str()
                .ok_or("non-UTF8 original bare path")?,
            checkout.to_str().ok_or("non-UTF8 owned repo path")?,
        ],
    )?;
    let tools = output.join("tools");
    fs::create_dir(&tools)?;
    symlink(tmux, tools.join("tmux"))?;
    // tmux uses a Unix socket whose path must fit sun_path; the artifact root
    // can be much longer than the kernel limit on hosted runners.
    let socket_directory = tempfile::Builder::new()
        .prefix("op-rec-tmux-")
        .tempdir_in("/tmp")?;
    let socket = socket_directory.path().join("tmux.sock");
    let quote = |path: &Path| format!("'{}'", path.display().to_string().replace('\'', "'\"'\"'"));
    let shell = format!(
        "printf 'owned recovery capture\\n'; {} -S {} wait-for -S ready; exec /bin/cat",
        quote(tmux),
        quote(&socket)
    );
    tmux_command(
        tmux,
        &socket,
        &["new-session", "-d", "-s", "owned", &shell],
        output,
    )?;
    let _lease = TmuxLease {
        binary: tmux,
        socket: &socket,
        cwd: output,
    };
    tmux_command(tmux, &socket, &["wait-for", "ready"], output)?;
    let server_id = tmux_command(
        tmux,
        &socket,
        &["display-message", "-p", "#{pid},#{pane_id}"],
        output,
    )?;
    let path = std::env::var_os("PATH").ok_or("PATH required for tmux control")?;
    let mut owned_path = tools.as_os_str().to_os_string();
    owned_path.push(":");
    owned_path.push(path);
    let port = TcpListener::bind(("127.0.0.1", 0))?.local_addr()?.port();
    let log = output.join("native.log");
    let argv = [
        OsString::from(candidate),
        checkout.as_os_str().to_os_string(),
        log.as_os_str().to_os_string(),
        OsString::from(process_child),
        OsString::from(port.to_string()),
    ];
    let environment = [
        (OsString::from("PATH"), owned_path),
        (
            OsString::from("TMUX"),
            OsString::from(format!("{},{}", socket.display(), server_id)),
        ),
    ];
    let mut server = OwnedServer::spawn(
        &argv,
        repository,
        &output.join("process.log"),
        &environment,
        &[],
        false,
    )?;
    let peer = HttpPeer::new(port, Duration::from_secs(5));
    let observed = (|| -> Result<(u16, Value)> {
        peer.wait_ready("/api/support_terminal/status", Duration::from_secs(10))?;
        let response = peer.request("POST", "/api/action", br#"{"action":"send_tmux_log"}"#)?;
        Ok((response.status, serde_json::from_slice(&response.body)?))
    })();
    let stop = server.signal(Signal::TERM);
    let exit = server.finish(Duration::from_secs(5));
    let (status, response) = observed?;
    stop?;
    let exit = exit?;
    let captured = fs::read(&log)?;
    let source = fs::read(original_capture)?;
    let pass = status == 200
        && response == json!({"ok":true,"file":"/download/tmux.log"})
        && captured == source
        && captured
            .windows(b"owned recovery capture".len())
            .any(|window| window == b"owned recovery capture");
    let receipt = json!({"http_status":status,"response":response,"source_len":source.len(),"captured_len":captured.len(),"exit":exit,"pass":pass});
    fs::write(
        output.join("comparison.json"),
        serde_json::to_vec_pretty(&receipt)?,
    )?;
    if !pass {
        return Err("Rust-owned real tmux capture diverged from original".into());
    }
    Ok(receipt)
}

pub(super) fn inherited_pty_bound(
    pair: &OwnedExecutablePair,
    original_bare: &Path,
    output: &Path,
) -> Result<Value> {
    let script_binary = Path::new("/usr/bin/script");
    let script_pin = Pin::open(script_binary, 1024 * 1024)?;
    let checkout = output.join("repo");
    git(
        output,
        &[
            "clone",
            original_bare
                .to_str()
                .ok_or("non-UTF8 original bare path")?,
            checkout.to_str().ok_or("non-UTF8 owned repo path")?,
        ],
    )?;
    let quote =
        |value: &Path| format!("'{}'", value.display().to_string().replace('\'', "'\"'\"'"));
    let child_script = "test -t 0 && test -t 1 && test -t 2 && printf tty-ready; read -r line; printf '%s' \"$line\" > tty-input";
    let shell = format!(
        "{} __repo-update --repo {} bash -c '{}'",
        quote(pair.candidate()),
        quote(&checkout),
        child_script.replace('\'', "'\"'\"'")
    );
    let mut spec = CommandSpec::new(
        vec![
            script_binary.display().to_string(),
            "-q".into(),
            "-e".into(),
            "-c".into(),
            shell,
            "/dev/null".into(),
        ],
        Duration::from_secs(10),
    );
    spec.cwd = Some(checkout.clone());
    spec.input = Some(b"owned-terminal-input\n".to_vec());
    spec.stdout_limit = 1024 * 1024;
    spec.stderr_limit = 1024 * 1024;
    spec.env.insert(
        "CARROT_REPO_LOCK_PATH".into(),
        output.join("transaction.lock").display().to_string(),
    );
    let observed = NativeRunner.run(&spec)?;
    fs::write(output.join("script.stdout"), &observed.stdout)?;
    fs::write(output.join("script.stderr"), &observed.stderr)?;
    script_pin.verify()?;
    let input = fs::read(checkout.join("tty-input"))?;
    let pass = observed.code == 0
        && observed
            .stdout
            .windows(b"tty-ready".len())
            .any(|slice| slice == b"tty-ready")
        && input == b"owned-terminal-input";
    let receipt = json!({"exit":observed.code,"all_stdio_tty":pass,"stdin_observed":String::from_utf8_lossy(&input),"script_sha256":script_pin.digest(),"pass":pass});
    fs::write(
        output.join("comparison.json"),
        serde_json::to_vec_pretty(&receipt)?,
    )?;
    if !pass {
        return Err("Rust-owned inherited PTY did not preserve all descriptors".into());
    }
    Ok(receipt)
}

pub(super) fn run(
    repository: &Path,
    pair: &OwnedExecutablePair,
    original_bare: &Path,
    original_row: &Value,
    name: &str,
    output: &Path,
) -> Result<Value> {
    run_inner(
        repository,
        pair,
        original_bare,
        original_row,
        name,
        output,
        None,
    )
}

/// The frozen lane reads only original/source observations, never previously
/// captured candidate replies or Git states.
#[allow(clippy::too_many_arguments)]
pub(super) fn run_frozen(
    repository: &Path,
    pair: &OwnedExecutablePair,
    source_bare: &Path,
    original_row: &Value,
    name: &str,
    output: &Path,
    source: &dyn Fn(&str) -> Result<Value>,
) -> Result<Value> {
    run_inner(
        repository,
        pair,
        source_bare,
        original_row,
        name,
        output,
        Some(source),
    )
}

#[allow(clippy::too_many_arguments)]
fn run_inner(
    repository: &Path,
    pair: &OwnedExecutablePair,
    original_bare: &Path,
    original_row: &Value,
    name: &str,
    output: &Path,
    frozen: Option<&OriginalObservation<'_>>,
) -> Result<Value> {
    let bare = output.join("origin.git");
    git(
        output,
        &[
            "clone",
            "--bare",
            original_bare
                .to_str()
                .ok_or("non-UTF8 original Git fixture path")?,
            bare.to_str().ok_or("non-UTF8 owned Git fixture path")?,
        ],
    )?;
    let initial = git(&bare, &["rev-parse", "refs/heads/topic"])?;
    let advanced = git(&bare, &["rev-parse", "refs/heads/main"])?;
    // The unchanged controller advances its shared remote in git_pull, the
    // fifth scenario. Earlier clones see the initial commit; every later
    // scenario, including git_rebuild, starts from the advanced commit.
    if matches!(
        name,
        "git_branches" | "git_log" | "git_reset" | "git_sync" | "git_pull"
    ) {
        git(&bare, &["update-ref", "refs/heads/main", &initial])?;
    }
    let checkout = output.join("repo");
    git(
        output,
        &[
            "clone",
            bare.to_str().ok_or("non-UTF8 owned bare path")?,
            checkout.to_str().ok_or("non-UTF8 owned checkout path")?,
        ],
    )?;
    git(&checkout, &["config", "user.name", "Recovery Fixture"])?;
    git(
        &checkout,
        &["config", "user.email", "recovery-fixture@example.invalid"],
    )?;
    let retained_helper = repository.join("openpilot/common/repo_update.py");
    let copied_helper = checkout.join("openpilot/common/repo_update.py");
    fs::create_dir_all(copied_helper.parent().ok_or("owned helper parent absent")?)?;
    fs::copy(&retained_helper, &copied_helper)?;
    prepare(&checkout, name)?;
    git(
        &checkout,
        &[
            "remote",
            "set-url",
            "origin",
            "https://github.com/bin9208/openpilot-rust.git",
        ],
    )?;
    let helper = checkout.join("rust/bin/openpilot-runtime-install");
    fs::create_dir_all(helper.parent().ok_or("missing owned helper parent")?)?;
    fs::write(&helper, b"#!/bin/sh\nprintf '%s\\n' \"$@\" > package-attempt\necho 'Owned matching release unavailable'\nexit 17\n")?;
    fs::set_permissions(&helper, fs::Permissions::from_mode(0o755))?;
    fs::write(
        checkout.join(".git/info/exclude"),
        "\n/rust/\n/package-attempt\n",
    )?;
    let rebuild = if matches!(name, "git_rebuild" | "rebuild_all") {
        Some(rebuild_setup(&checkout, output)?)
    } else {
        None
    };
    let before = snapshot(&checkout)?;
    if name == "git_pull" {
        git(&bare, &["update-ref", "refs/heads/main", &advanced])?;
    }
    let global = output.join("global.gitconfig");
    fs::write(&global, format!("[url \"{}\"]\n\tinsteadOf = https://github.com/ajouatom/openpilot.git\n\tinsteadOf = https://github.com/bin9208/openpilot-rust.git\n",bare.display()))?;
    let busy_lock = if name == "busy_lock" {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(output.join("transaction.lock"))?;
        file.try_lock().map_err(|_| "owned busy lock failed")?;
        Some(file)
    } else {
        None
    };
    let port = TcpListener::bind(("127.0.0.1", 0))?.local_addr()?.port();
    let argv = [
        pair.candidate().as_os_str().to_os_string(),
        checkout.as_os_str().to_os_string(),
        OsString::from(output.join("tmux.log")),
        pair.process_child().as_os_str().to_os_string(),
        OsString::from(port.to_string()),
    ];
    let environment = [
        (
            OsString::from("CARROT_REPO_LOCK_PATH"),
            OsString::from(output.join("transaction.lock")),
        ),
        (
            OsString::from("GIT_CONFIG_GLOBAL"),
            global.as_os_str().to_os_string(),
        ),
        (OsString::from("GIT_CONFIG_NOSYSTEM"), OsString::from("1")),
        (OsString::from("GIT_ALLOW_PROTOCOL"), OsString::from("file")),
    ];
    let mut server = OwnedServer::spawn(
        &argv,
        repository,
        &output.join("process.log"),
        &environment,
        &[],
        false,
    )?;
    let peer = HttpPeer::new(port, Duration::from_secs(5));
    let payload = original_row
        .get("payload")
        .ok_or("missing independent action payload")?;
    let observed = (|| -> Result<(u16, Value)> {
        peer.wait_ready("/api/support_terminal/status", Duration::from_secs(10))?;
        let response = peer.request("POST", "/api/action", &serde_json::to_vec(payload)?)?;
        Ok((response.status, serde_json::from_slice(&response.body)?))
    })();
    let stop = server.signal(Signal::TERM);
    let finished = server.finish(Duration::from_secs(5));
    let (status, response) = observed?;
    stop?;
    let server_exit = finished?;
    let original_root = output
        .parent()
        .ok_or("missing owned replay parent")?
        .parent()
        .ok_or("missing original actions root")?;
    let previous: Value = if let Some(source) = frozen {
        source(&format!("actions/{name}/source-evidence/reply.json"))?
    } else {
        serde_json::from_slice(&fs::read(
            original_root.join(name).join("native-evidence/reply.json"),
        )?)?
    };
    let mut normalized = response.clone();
    let mut expected = previous.clone();
    let executable = response
        .get("command")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let has_command = executable.is_some();
    if frozen.is_some() && has_command {
        let action = original_row["payload"]["action"]
            .as_str()
            .ok_or("frozen action name absent")?;
        if let Some(verb) = super::actions_control::staged_verb(action) {
            let command = executable
                .as_deref()
                .ok_or("frozen action command absent")?;
            if !command.split_ascii_whitespace().any(|part| part == verb) {
                return Err("frozen staged verb changed".into());
            }
        }
    }
    if executable.is_some() {
        normalized
            .as_object_mut()
            .ok_or("action response not object")?
            .remove("command");
        expected
            .as_object_mut()
            .ok_or("prior action response not object")?
            .remove("command");
    }
    if status != 200 || normalized != expected {
        return Err(format!("independent action reply changed: {name}").into());
    }
    let mut shell_exit = None;
    if let Some(script) = executable {
        if ["scons", "git clean", "git reset --hard", "ajouatom"]
            .iter()
            .any(|part| script.contains(part))
        {
            return Err("unsafe maintenance command returned".into());
        }
        if name == "number_commit" {
            if !script
                .split_ascii_whitespace()
                .any(|part| part == "__stage-commit")
                || !script.ends_with(" __stage-commit 17")
            {
                return Err("integer commit command changed".into());
            }
        } else {
            let mut shell_environment = vec![
                (
                    "CARROT_REPO_LOCK_PATH",
                    output.join("transaction.lock").display().to_string(),
                ),
                ("GIT_CONFIG_GLOBAL", global.display().to_string()),
                ("GIT_CONFIG_NOSYSTEM", "1".into()),
                ("GIT_ALLOW_PROTOCOL", "file".into()),
            ];
            if let Some((_, tools)) = &rebuild {
                shell_environment.push((
                    "PATH",
                    format!("{}:{}", tools.display(), std::env::var("PATH")?),
                ));
            }
            let run = command(
                vec!["/bin/bash".into(), "-c".into(), script],
                &checkout,
                &shell_environment,
            )?;
            fs::write(output.join("shell.stdout"), &run.stdout)?;
            fs::write(output.join("shell.stderr"), &run.stderr)?;
            shell_exit = Some(run.code);
        }
    }
    drop(busy_lock);
    let after = snapshot(&checkout)?;
    let staged = if frozen.is_some() {
        has_command
            && matches!(
                original_row["payload"]["action"].as_str(),
                Some(
                    "git_reset"
                        | "git_sync"
                        | "git_pull"
                        | "git_rebuild"
                        | "rebuild_all"
                        | "git_checkout"
                        | "git_checkout_commit"
                        | "git_reset_repo"
                )
            )
    } else {
        original_row["staged_contract"] == true
    };
    let original_after: Value = if let Some(source) = frozen {
        source(&format!("actions/{name}/source-evidence/repository.json"))?
    } else {
        serde_json::from_slice(&fs::read(
            original_root
                .join(name)
                .join("native-evidence/repository.json"),
        )?)?
    };
    if frozen.is_some() {
        match name {
            "git_reset" | "git_pull" | "git_reset_repo" => {
                if original_after["tracked"] != "initial\n"
                    || before["tracked"] != "locally modified\n"
                {
                    return Err("frozen original destructive reset control changed".into());
                }
            }
            "git_sync" => {
                if !before["refs"]
                    .as_str()
                    .is_some_and(|refs| refs.contains("refs/heads/spare "))
                    || original_after["refs"]
                        .as_str()
                        .is_some_and(|refs| refs.contains("refs/heads/spare "))
                {
                    return Err("frozen original branch deletion control changed".into());
                }
            }
            _ => {}
        }
        if name == "git_reset_repo"
            && original_after["remote"] != "https://github.com/ajouatom/openpilot.git"
        {
            return Err("frozen original remote-reset control changed".into());
        }
    }
    for field in [
        "head", "branch", "status", "tracked", "remote", "refs", "fetch",
    ] {
        let expected_field = if frozen.is_some() && staged {
            if name == "git_pull" && field == "refs" {
                let before_refs = before["refs"].as_str().ok_or("frozen Git refs absent")?;
                let initial = git(&bare, &["rev-parse", "refs/heads/topic"])?;
                let advanced = git(&bare, &["rev-parse", "refs/heads/main"])?;
                let changed = before_refs
                    .replace(
                        &format!("refs/remotes/origin/HEAD {initial} "),
                        &format!("refs/remotes/origin/HEAD {advanced} "),
                    )
                    .replace(
                        &format!("refs/remotes/origin/main {initial} "),
                        &format!("refs/remotes/origin/main {advanced} "),
                    );
                if changed == before_refs || after["refs"] != changed {
                    return Err("frozen Git pull remote-ref relation changed".into());
                }
                continue;
            }
            &before[field]
        } else {
            &original_after[field]
        };
        if after[field] != *expected_field {
            return Err(
                format!("owned Git effect differs from original relation: {name}/{field}").into(),
            );
        }
    }
    let original_exit = if let Some(source) = frozen {
        if staged || !has_command {
            None
        } else {
            source(&format!("actions/{name}/source-evidence/shell.json"))?["exit"]
                .as_i64()
                .map(i32::try_from)
                .transpose()?
        }
    } else {
        original_row["exits"]
            .get("native")
            .and_then(Value::as_i64)
            .map(i32::try_from)
            .transpose()?
    };
    if rebuild.is_none() && (frozen.is_none() || !staged) && shell_exit != original_exit {
        return Err(format!(
            "owned Git command exit differs from original native observation: {name}"
        )
        .into());
    }
    if let Some((cache, _)) = &rebuild {
        let actual = json!({
            "scons":if checkout.join("scons.argv").exists(){fs::read_to_string(checkout.join("scons.argv"))?}else{String::new()},
            "prebuilt_removed":!checkout.join("prebuilt").exists(),
            "sign_removed":!checkout.join(".sconsign.dblite").exists(),
            "cache_removed":!cache.exists(),
            "sudo":if checkout.join("sudo.argv").exists(){fs::read_to_string(checkout.join("sudo.argv"))?}else{String::new()},
        });
        let effects_match = if frozen.is_some() {
            actual
                == json!({"scons":"","sudo":"","prebuilt_removed":false,
                "sign_removed":false,"cache_removed":false})
        } else {
            let original: Value = serde_json::from_slice(&fs::read(
                original_root.join(format!("{name}_effects/native-evidence/effects.json")),
            )?)?;
            actual == original
        };
        if !effects_match
            || fs::read_to_string(checkout.join("rust/runtime-manifest.json"))? != "old package"
            || shell_exit != Some(17)
        {
            return Err(format!("owned native rebuild changed destructive effects: {name}").into());
        }
    }
    if staged && (after["head"] != before["head"] || after["tracked"] != before["tracked"]) {
        return Err(format!("staged command changed checkout: {name}").into());
    }
    if name == "git_reset_repo"
        && after["remote"] != "https://github.com/bin9208/openpilot-rust.git"
    {
        return Err("staged reset changed owned remote".into());
    }
    if staged && name != "number_commit" {
        let expected = match name {
            "fresh_index" | "symlink_index" | "busy_lock" => Some(75),
            "git_sync" => Some(0),
            "quoted_branch" | "git_pull_missing" => None,
            _ => Some(17),
        };
        match (expected, shell_exit) {
            (Some(expected), Some(actual)) if expected == actual => {}
            (None, Some(actual)) if actual != 0 => {}
            _ => {
                return Err(
                    format!("owned staged command exit changed: {name}: {shell_exit:?}").into(),
                )
            }
        }
    }
    if staged && shell_exit == Some(17) {
        let attempted = fs::read_to_string(checkout.join("package-attempt"))?;
        let mut arguments = attempted.lines();
        if arguments.next() != Some("--stage-update")
            || arguments.next() != Some("--root")
            || arguments.next() != checkout.to_str()
        {
            return Err("owned package helper arguments changed".into());
        }
    }
    if checkout.join("INJECTED").exists() {
        return Err("quoted branch escaped into an operation".into());
    }
    if fs::read(&copied_helper)? != fs::read(&retained_helper)? {
        return Err("owned untracked retained helper changed".into());
    }
    let receipt = json!({"scenario":name,"before":before,"after":after,"response":response,
        "shell_exit":shell_exit,"server_exit":server_exit,"staged":staged,"pass":true});
    fs::write(
        output.join("comparison.json"),
        serde_json::to_vec_pretty(&receipt)?,
    )?;
    Ok(receipt)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owned_aliases_bind_both_sources_and_reject_replacement() {
        let temp = tempfile::tempdir().unwrap();
        let candidate = temp.path().join("candidate");
        let child = temp.path().join("child");
        let output = temp.path().join("owned");
        fs::write(&candidate, b"candidate ELF fixture").unwrap();
        fs::write(&child, b"process child ELF fixture").unwrap();
        fs::create_dir(&output).unwrap();
        let pair = OwnedExecutablePair::create(&candidate, &child, &output).unwrap();
        pair.record("before-invocation").unwrap();
        let lines = fs::read_to_string(&pair.receipt_path).unwrap();
        let row: Value = serde_json::from_str(lines.lines().last().unwrap()).unwrap();
        assert_eq!(row["candidate"]["source"], candidate.display().to_string());
        assert_eq!(
            row["candidate"]["alias"],
            pair.candidate().display().to_string()
        );
        assert_eq!(row["candidate"]["sha256"], pair.source_candidate.digest());
        assert_eq!(
            row["candidate"]["ino"],
            fs::metadata(&candidate).unwrap().ino()
        );
        assert_eq!(row["process_child"]["sha256"], pair.source_child.digest());
        fs::remove_file(pair.process_child()).unwrap();
        fs::write(pair.process_child(), b"foreign replacement").unwrap();
        assert!(pair.record("after-invocation").is_err());
        assert_eq!(
            fs::read(pair.process_child()).unwrap(),
            b"foreign replacement"
        );
    }

    #[test]
    fn owned_alias_receipt_replacement_is_not_clobbered() {
        let temp = tempfile::tempdir().unwrap();
        let candidate = temp.path().join("candidate");
        let child = temp.path().join("child");
        let output = temp.path().join("owned");
        fs::write(&candidate, b"candidate ELF fixture").unwrap();
        fs::write(&child, b"process child ELF fixture").unwrap();
        fs::create_dir(&output).unwrap();
        let pair = OwnedExecutablePair::create(&candidate, &child, &output).unwrap();
        fs::remove_file(&pair.receipt_path).unwrap();
        fs::write(&pair.receipt_path, b"foreign receipt").unwrap();
        assert!(pair.record("after-invocation").is_err());
        assert_eq!(fs::read(&pair.receipt_path).unwrap(), b"foreign receipt");
    }
}
