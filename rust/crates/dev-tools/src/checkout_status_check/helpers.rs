use super::{
    peer::{Binaries, Peer},
    require, Result,
};
use serde_json::{json, Value};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};
pub struct Helpers {
    pub captured: PathBuf,
    pub handshake: PathBuf,
}
fn probe(
    peer: &mut Peer,
    tools: &Helpers,
    name: &str,
    request: &Value,
    results: &mut Vec<Value>,
) -> Result<Value> {
    let output = peer.probe(name, &tools.captured, &[], request)?;
    let response: Value = serde_json::from_slice(&output.stdout)?;
    results.push(json!({"name":name,"response":response}));
    Ok(response)
}
pub fn native(binaries: &Binaries, tools: &Helpers, output: &Path) -> Result<Value> {
    let mut results = Vec::new();
    let mut peer = Peer::new(binaries, "rust", &output.join("handshake"), true)?;
    fs::create_dir(peer.repo.join(".git"))?;
    peer.fixture(json!({"stdout":"a".repeat(40).as_bytes()}))?;
    let request = json!({"launcher":crate::path_resolve::resolve_non_strict(&binaries.launcher)?,"cwd":peer.repo,"argv":["/bin/true"]});
    let response = probe(&mut peer, tools, "successful-eof", &request, &mut results)?;
    require(
        response["kind"] == "child_exit"
            && response["code"] == 0
            && response["stdout"] == json!([]),
        "successful exec EOF differs",
    )?;
    for (name, key, value) in [
        (
            "failed-target-after-connect",
            "argv",
            json!(["./missing-target"]),
        ),
        (
            "failed-cwd-after-connect",
            "cwd",
            json!(peer.repo.join("absent")),
        ),
        (
            "missing-helper",
            "launcher",
            json!(peer.root.join("missing-helper")),
        ),
    ] {
        let mut changed = request.clone();
        changed[key] = value;
        let response = probe(&mut peer, tools, name, &changed, &mut results)?;
        require(
            response["kind"] == "spawn_error",
            format!("{name}: expected spawn error"),
        )?;
        if name != "missing-helper" {
            require(response["errno"] == 2, format!("{name}: errno differs"))?;
        }
    }
    let mut fake = request.clone();
    fake["launcher"] = json!(crate::path_resolve::resolve_non_strict(&tools.handshake)?);
    for (mode, kind) in [
        ("exit_before", "spawn_error"),
        ("corrupt_descriptor", "spawn_error"),
        ("exit_connected", "child_exit"),
        ("signal_connected", "child_exit"),
        ("invalid_marker", "spawn_error"),
    ] {
        fs::write(peer.control.join("helper.json"), serde_json::to_vec(&mode)?)?;
        peer.record(json!({"helper_mode":mode}))?;
        let response = probe(&mut peer, tools, mode, &fake, &mut results)?;
        require(
            response["kind"] == kind,
            format!("{mode}: handshake outcome differs"),
        )?;
        if mode == "exit_connected" {
            require(
                response["code"] == 7 && response["signal"].is_null(),
                "connected child exit differs",
            )?;
        }
        if mode == "signal_connected" {
            require(
                response["code"].is_null() && response["signal"] == 9,
                "connected signal differs",
            )?;
        }
    }
    fs::remove_dir(&peer.launches)?;
    fs::write(&peer.launches, "not a directory")?;
    peer.record(json!({"fixture_fs":"TMPDIR replaced with owned regular file"}))?;
    let direct = super::peer::capture(
        peer.command("/bin/true"),
        None,
        None,
        std::time::Duration::from_secs(4),
    )?;
    require(
        direct.status.success(),
        "direct exec invalid-TMPDIR control failed",
    )?;
    let response = probe(
        &mut peer,
        tools,
        "invalid-tmpdir-does-not-control-exec-storage",
        &request,
        &mut results,
    )?;
    require(
        response["kind"] == "child_exit" && response["code"] == 0,
        "TMPDIR controlled exec storage",
    )?;
    fs::remove_file(&peer.launches)?;
    fs::create_dir(&peer.launches)?;
    peer.close()?;
    let delayed = Binaries {
        launcher: tools.handshake.clone(),
        ..binaries.clone()
    };
    let mut peer = Peer::new(&delayed, "rust", &output.join("deadline"), true)?;
    fs::create_dir(peer.repo.join(".git"))?;
    fs::write(peer.control.join("helper.json"), "\"delayed_launch\"")?;
    peer.record(json!({"helper_mode":"delayed_launch"}))?;
    peer.fixture(json!({"stdout":"a".repeat(40).as_bytes(),"delay_ms":500}))?;
    let response = peer.op("read", 0.0)?;
    require(
        response["commit"] == "a".repeat(40)
            && response["elapsed"].as_f64().is_some_and(|v| v > 1.1),
        "Git timeout incorrectly began before exec",
    )?;
    require(
        peer.helper_calls()?
            .first()
            .is_some_and(|v| v["mode"] == "delayed_launch"),
        "delayed helper did not execute",
    )?;
    results.push(json!({"name":"timeout-begins-after-exec","response":response}));
    peer.close()?;
    require(results.len() == 11, "native helper coverage differs")?;
    Ok(json!(results))
}
pub fn inherited(binaries: &Binaries, tools: &Helpers, output: &Path) -> Result<Value> {
    let mut peer = Peer::new(binaries, "rust", &output.join("inherited"), true)?;
    peer.env
        .insert("PARAMS_COPY_PATH".into(), "parent-copy-path".into());
    let bad = peer.repo.join("bad-format");
    fs::write(&bad, "exit 9\n")?;
    fs::set_permissions(&bad, fs::Permissions::from_mode(0o755))?;
    fs::write(peer.repo.join("denied"), "denied")?;
    let mut results = Vec::new();
    for (name, argv, kind, code) in [
        ("exit-zero", "git", "child_exit", 0),
        ("exit-one", "git", "child_exit", 1),
        ("environment-override", "git", "child_exit", 0),
        ("missing", "./absent", "spawn_error", 2),
        ("exec-format", "./bad-format", "spawn_error", 8),
        ("permission", "./denied", "spawn_error", 13),
    ] {
        peer.fixture(json!({"stdout":b"inherited stdout","stderr":b"inherited stderr","exit_code":i32::from(name=="exit-one")}))?;
        let mut pair = Vec::new();
        for implementation in ["reference", "rust"] {
            let report = peer.output.join(format!("{name}-{implementation}.json"));
            let request = json!({"launcher":crate::path_resolve::resolve_non_strict(&binaries.launcher)?,"cwd":peer.repo,"argv":[argv],"inherit":true,"report":report,
                "environment":if name=="environment-override"{vec![("PARAMS_COPY_PATH","child-copy-path")]}else{vec![]}});
            let executable = if implementation == "reference" {
                &binaries.reference
            } else {
                &tools.captured
            };
            let arguments = if implementation == "reference" {
                vec!["--inherited"]
            } else {
                vec![]
            };
            let child = peer.probe(name, executable, &arguments, &request)?;
            let response: Value = serde_json::from_slice(&fs::read(report)?)?;
            require(
                response["parent_environment_before"] == "parent-copy-path"
                    && response["parent_environment_after"] == "parent-copy-path",
                "child environment override leaked to parent",
            )?;
            let actual = if response["kind"] == "child_exit" {
                response["code"].clone()
            } else {
                response["errno"].clone()
            };
            require(
                response["kind"] == kind && actual == code,
                format!("inherited {name}/{implementation}: {response}"),
            )?;
            pair.push(json!([
                [kind, actual],
                std::str::from_utf8(&child.stdout)?,
                std::str::from_utf8(&child.stderr)?
            ]));
        }
        require(
            pair[0] == pair[1],
            format!("inherited {name}: source and candidate differ"),
        )?;
        if kind == "child_exit" {
            require(
                pair[0][1] == "inherited stdout" && pair[0][2] == "inherited stderr",
                "inherited raw streams differ",
            )?;
            let calls = peer.calls()?;
            for call in calls.iter().rev().take(2) {
                require(
                    call["inherited_fd_target"].is_null()
                        && call["manager_daemon"] == "checkout-parent",
                    "inherited child descriptor/manager differs",
                )?;
                require(
                    call["params_copy_path"]
                        == if name == "environment-override" {
                            "child-copy-path"
                        } else {
                            "parent-copy-path"
                        },
                    "child environment override differs",
                )?;
            }
        }
        results.push(json!({"name":name,"pair":pair}));
    }
    peer.close()?;
    Ok(json!(results))
}
