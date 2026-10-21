//! Rust-owned Git fixtures for the action responses. The historical controller
//! remains the independent oracle, but its repositories are never reused here.

use super::{HttpPeer, OwnedServer, Pin, Result};
use crate::qa_process::{CommandSpec, NativeRunner, Runner};
use rustix::process::Signal;
use serde_json::{json, Value};
use std::{
    ffi::OsString,
    fs::{self, OpenOptions},
    net::TcpListener,
    os::unix::fs::{symlink, PermissionsExt},
    path::{Path, PathBuf},
    time::Duration,
};

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
    NativeRunner.run(&spec).map_err(Into::into)
}

fn git(cwd: &Path, args: &[&str]) -> Result<String> {
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
        "tracked":fs::read_to_string(repo.join("tracked"))?,
        "remote":git(repo,&["config","--get","remote.origin.url"])?,
        "refs":git(repo,&["for-each-ref","--format=%(refname) %(objectname) %(upstream)"])?,
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

pub(super) fn inherited_pty(
    candidate: &Path,
    original_bare: &Path,
    output: &Path,
) -> Result<Value> {
    fs::create_dir(output)?;
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
        quote(candidate),
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
    candidate: &Path,
    process_child: &Path,
    original_bare: &Path,
    original_row: &Value,
    name: &str,
    output: &Path,
) -> Result<Value> {
    fs::create_dir(output)?;
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
    git(&bare, &["update-ref", "refs/heads/main", &initial])?;
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
        OsString::from(candidate),
        checkout.as_os_str().to_os_string(),
        OsString::from(output.join("tmux.log")),
        OsString::from(process_child),
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
    let previous: Value = serde_json::from_slice(&fs::read(
        output
            .parent()
            .ok_or("missing owned replay parent")?
            .parent()
            .ok_or("missing original actions root")?
            .join(name)
            .join("native-evidence/reply.json"),
    )?)?;
    let mut normalized = response.clone();
    let mut expected = previous.clone();
    let executable = response
        .get("command")
        .and_then(Value::as_str)
        .map(str::to_owned);
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
    drop(busy_lock);
    let after = snapshot(&checkout)?;
    let original_root = output
        .parent()
        .ok_or("missing owned replay parent")?
        .parent()
        .ok_or("missing original actions root")?;
    let original_after: Value = serde_json::from_slice(&fs::read(
        original_root
            .join(name)
            .join("native-evidence/repository.json"),
    )?)?;
    for field in ["head", "branch", "tracked", "remote", "refs"] {
        if after[field] != original_after[field] {
            return Err(format!(
                "owned Git effect differs from original native observation: {name}/{field}"
            )
            .into());
        }
    }
    let original_exit = original_row["exits"]
        .get("native")
        .and_then(Value::as_i64)
        .map(i32::try_from)
        .transpose()?;
    if rebuild.is_none() && shell_exit != original_exit {
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
        let original: Value = serde_json::from_slice(&fs::read(
            original_root.join(format!("{name}_effects/native-evidence/effects.json")),
        )?)?;
        if actual != original
            || fs::read_to_string(checkout.join("rust/runtime-manifest.json"))? != "old package"
            || shell_exit != Some(17)
        {
            return Err(format!("owned native rebuild changed destructive effects: {name}").into());
        }
    }
    let staged = original_row["staged_contract"] == true;
    if staged && (after["head"] != before["head"] || after["tracked"] != before["tracked"]) {
        return Err(format!("staged command changed checkout: {name}").into());
    }
    if name == "git_reset_repo"
        && after["remote"] != "https://github.com/bin9208/openpilot-rust.git"
    {
        return Err("staged reset changed owned remote".into());
    }
    if staged {
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
    let receipt = json!({"scenario":name,"before":before,"after":after,"response":response,
        "shell_exit":shell_exit,"server_exit":server_exit,"staged":staged,"pass":true});
    fs::write(
        output.join("comparison.json"),
        serde_json::to_vec_pretty(&receipt)?,
    )?;
    Ok(receipt)
}
