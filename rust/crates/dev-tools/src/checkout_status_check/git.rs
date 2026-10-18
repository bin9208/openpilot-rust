use super::{peer::Peer, require, Result};
use serde_json::{json, Value};
use std::{
    fs,
    os::unix::fs::{symlink, PermissionsExt},
};
pub fn real(peer: &mut Peer, format: &str) -> Result<Value> {
    peer.git(&[
        "init",
        "--quiet",
        "--initial-branch=fixture",
        &format!("--object-format={format}"),
    ])?;
    require(
        peer.op("read", 0.0)?["commit"].is_null(),
        "empty Git commit must be unavailable",
    )?;
    peer.git(&["commit", "--quiet", "--allow-empty", "-m", "original"])?;
    let first = peer.git(&["rev-parse", "HEAD"])?;
    require(
        first.len() == if format == "sha1" { 40 } else { 64 },
        "real Git object-format identity differs",
    )?;
    peer.metadata(&"f".repeat(40))?;
    require(
        peer.op("read", 0.0)?["commit"] == first,
        "Git must take precedence over build metadata",
    )?;
    let initial = peer.op("capture", 0.0)?;
    require(
        initial["running_commit"] == first && initial["reboot_required"] == false,
        "startup identity differs",
    )?;
    fs::write(peer.repo.join("dirty"), "untracked change\n")?;
    require(
        peer.op("read", 0.0)?["commit"] == first,
        "untracked change altered identity",
    )?;
    peer.git(&["commit", "--quiet", "--allow-empty", "-m", "changed"])?;
    let second = peer.git(&["rev-parse", "HEAD"])?;
    require(first != second, "Git fixture commit did not change")?;
    let changes = vec![
        peer.op("update", 0.0)?["returned"].clone(),
        peer.op("update", 5.0)?["returned"].clone(),
    ];
    require(
        changes == [json!(false), json!(true)],
        "Git debounce differs",
    )?;
    peer.git(&["checkout", "--quiet", "--detach", &first])?;
    let reverted = peer.op("update", 10.0)?;
    require(
        reverted["returned"] == false && reverted["running_commit"] == first,
        "Git revert changed startup identity",
    )?;
    Ok(
        json!({"first":first,"second":second,"changed_reads":changes,"reverted":reverted["returned"]}),
    )
}
pub fn file(peer: &mut Peer) -> Result<Value> {
    let separate = format!("--separate-git-dir={}", peer.root.join("gitdir").display());
    peer.git(&["init", "--quiet", "--initial-branch=fixture", &separate])?;
    peer.git(&[
        "commit",
        "--quiet",
        "--allow-empty",
        "-m",
        "separate git directory",
    ])?;
    let expected = peer.git(&["rev-parse", "HEAD"])?;
    require(
        peer.repo.join(".git").is_file(),
        "separate Git fixture not a file",
    )?;
    let response = peer.op("read", 0.0)?;
    require(
        response["commit"] == expected,
        "separate Git file identity differs",
    )?;
    Ok(json!({"commit":response["commit"],"git_is_file":true}))
}
pub fn fixture(peer: &mut Peer) -> Result<Value> {
    let a = "A".repeat(40);
    fs::create_dir(peer.repo.join(".git"))?;
    let mut results = Vec::new();
    let rows = vec![
        (
            "valid",
            json!({"stdout":a.as_bytes()}),
            Some(a.to_lowercase()),
        ),
        (
            "unicode-trim",
            json!({"stdout":format!("\u{1c}\u{1d}\u{1e}\u{1f}\u{85}\u{2003}\r\n{a}\u{3000}\n").as_bytes()}),
            Some(a.to_lowercase()),
        ),
        (
            "valid64",
            json!({"stdout":"BC09".repeat(16).as_bytes()}),
            Some("bc09".repeat(16)),
        ),
        (
            "bad-status",
            json!({"stdout":a.as_bytes(),"exit_code":7}),
            None,
        ),
        ("bad-stdout-utf8", json!({"stdout":[255]}), None),
        (
            "bad-stderr-utf8",
            json!({"stdout":a.as_bytes(),"stderr":[255]}),
            None,
        ),
        (
            "multiline",
            json!({"stdout":format!("{a}\n{a}").as_bytes()}),
            None,
        ),
        ("nonhex", json!({"stdout":"g".repeat(40).as_bytes()}), None),
        ("nul", json!({"stdout":format!("{a}\0").as_bytes()}), None),
        (
            "signal",
            json!({"stdout":a.as_bytes(),"mode":"signal"}),
            None,
        ),
        (
            "large-pipes",
            json!({"stdout":a.as_bytes(),"padding":1024*1024}),
            Some(a.to_lowercase()),
        ),
        ("timeout", json!({"mode":"sleep"}), None),
        ("closed-pipes-timeout", json!({"mode":"close_sleep"}), None),
    ];
    for (name, behavior, expected) in rows {
        peer.fixture(behavior)?;
        let response = peer.op("read", 0.0)?;
        require(
            response["commit"].as_str() == expected.as_deref(),
            format!("Git executable {name}: {response}"),
        )?;
        if name.ends_with("timeout") {
            let elapsed = response["elapsed"].as_f64().ok_or("missing Git elapsed")?;
            require(
                elapsed > 0.9 && elapsed < 3.0,
                format!("{name} deadline differs: {elapsed}"),
            )?;
        }
        results.push(json!({"name":name,"commit":response["commit"]}));
    }
    let calls = peer.calls()?;
    require(calls.len() == 13, "Git invocation count differs")?;
    let process = peer.process_id()?;
    let stdin = fs::read_link(format!("/proc/{process}/fd/0"))?;
    let group = rustix::process::getpgid(rustix::process::Pid::from_raw(i32::try_from(process)?))?
        .as_raw_pid();
    for call in &calls {
        require(
            call["argv"]
                == json!([
                    "git",
                    "--no-optional-locks",
                    "rev-parse",
                    "--verify",
                    "HEAD^{commit}"
                ]),
            "Git argv differs",
        )?;
        require(
            call["cwd"] == json!(peer.repo)
                && call["inherited_fd_target"].is_null()
                && call["manager_daemon"] == "checkout-parent",
            "Git cwd/descriptor/environment differs",
        )?;
        require(
            call["stdin_target"] == json!(stdin) && call["process_group"] == group,
            "Git stdin or process group not inherited",
        )?;
    }
    let command = peer.bin.join("git");
    fs::remove_file(&command)?;
    for (name, setup) in [
        ("missing-executable", 0),
        ("executable-format", 1),
        ("executable-permission", 2),
        ("path-after-exec-format", 3),
    ] {
        if setup == 1 {
            fs::write(&command, format!("printf '%s' '{}'\n", "f".repeat(40)))?;
            fs::set_permissions(&command, fs::Permissions::from_mode(0o700))?;
        }
        if setup == 2 {
            fs::set_permissions(&command, fs::Permissions::from_mode(0o0))?;
        }
        if setup == 3 {
            fs::set_permissions(&command, fs::Permissions::from_mode(0o700))?;
            symlink(
                crate::path_resolve::resolve_non_strict(&peer.binaries.fixture)?,
                peer.fallback.join("git"),
            )?;
            peer.fixture(json!({"stdout":a.as_bytes()}))?;
        }
        let response = peer.op("read", 0.0)?;
        let expected = if setup == 3 {
            Some(a.to_lowercase())
        } else {
            None
        };
        require(
            response["commit"].as_str() == expected.as_deref(),
            format!("Git executable {name}: {response}"),
        )?;
        require(
            peer.calls()?.len() == 13 + usize::from(setup == 3),
            "failed exec unexpectedly invoked fixture",
        )?;
        results.push(json!({"name":name,"commit":response["commit"]}));
    }
    Ok(json!(results))
}
