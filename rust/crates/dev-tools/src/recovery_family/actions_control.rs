//! Independent Rust-owned Git, tmux and PTY effects against sealed original
//! observations.

use super::{
    actions_effects, original_fixture::OriginalFixture, HttpPeer, OwnedServer, Pin, Result,
};
use crate::recovery_exec_audit::CreatedProgramGuard;
use rustix::process::Signal;
use serde_json::{json, Value};
use std::{
    ffi::OsString,
    fs,
    net::TcpListener,
    os::unix::fs::PermissionsExt,
    path::Path,
    process::Command,
    thread,
    time::{Duration, Instant},
};

pub struct ActionsFrozenConfig<'a> {
    pub repository: &'a Path,
    pub source_server: &'a Path,
    pub repo_update: &'a Path,
    pub candidate: &'a Path,
    pub process_child: &'a Path,
    pub tmux: &'a Path,
    pub output: &'a Path,
}

const SCENARIOS: &[&str] = &[
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

const EXTRAS: &[&str] = &[
    "tmux_capture",
    "missing_tmux",
    "permission_tmux",
    "nonzero_tmux",
    "whitespace_tmux",
    "replacement_tmux",
    "sudo_reboot",
    "fallback_reboot",
    "missing_reboot",
    "git_rebuild_effects",
    "rebuild_all_effects",
    "inherited_pty",
];

fn frozen_payload(name: &str) -> Result<Value> {
    let payload = match name {
        "git_pull_repair" | "git_pull_preserve" | "git_pull_multiple" | "git_pull_missing" => {
            json!({"action":"git_pull"})
        }
        "git_checkout" => json!({"action":"git_checkout","branch":"origin/topic"}),
        "git_checkout_local" => json!({"action":"git_checkout","branch":"local"}),
        "git_checkout_commit" => json!({"action":"git_checkout_commit",
            "commit":"669014cb9efb52f3c16e4fd45987db291de37d61"}),
        "git_reset_repo" => json!({"action":"git_reset_repo","branch":"main"}),
        "quoted_branch" => json!({"action":"git_checkout","branch":"topic; touch INJECTED"}),
        "stale_index" | "fresh_index" | "symlink_index" | "busy_lock" => {
            json!({"action":"git_reset"})
        }
        "missing_branch" => json!({"action":"git_checkout"}),
        "false_branch" => json!({"action":"git_checkout","branch":false}),
        "number_commit" => json!({"action":"git_checkout_commit","commit":17}),
        name if SCENARIOS.contains(&name) => json!({"action":name}),
        _ => return Err("unknown frozen actions scenario".into()),
    };
    Ok(payload)
}

fn replay_frozen_primary(
    config: &ActionsFrozenConfig<'_>,
    original: &OriginalFixture,
    bare: &Path,
    pair: &actions_effects::OwnedExecutablePair,
) -> Result<(Vec<Value>, Vec<String>)> {
    let effects_root = config.output.join("rust-owned-effects");
    fs::create_dir(&effects_root)?;
    let mut effects = Vec::with_capacity(SCENARIOS.len());
    let mut failures = Vec::new();
    for name in SCENARIOS {
        let row = original.observation(&format!("actions/{name}/comparison.json"))?;
        if row["scenario"] != *name
            || row["payload"] != frozen_payload(name)?
            || row.get("native").is_some()
            || row.get("staged_contract").is_some()
            || !original
                .observation(&format!("actions/{name}/source-evidence/reply.json"))?
                .is_object()
            || !original
                .observation(&format!("actions/{name}/source-evidence/repository.json"))?
                .is_object()
        {
            return Err(format!("frozen original action row changed: {name}").into());
        }
        if let Some(source_exit) = row.get("source_exit") {
            let shell =
                original.observation(&format!("actions/{name}/source-evidence/shell.json"))?;
            if &shell["exit"] != source_exit {
                return Err(format!("frozen original action shell exit changed: {name}").into());
            }
        }
        let effect_output = effects_root.join(name);
        fs::create_dir(&effect_output)?;
        pair.record(&format!("before-{name}"))?;
        let attempt = actions_effects::run_frozen(
            config.repository,
            pair,
            bare,
            &row,
            name,
            &effect_output,
            &|path| original.observation(path),
        );
        pair.record(&format!("after-{name}"))?;
        match attempt {
            Ok(receipt) => effects.push(receipt),
            Err(error) => failures.push(format!("owned effect {name}: {error}")),
        }
        fs::write(
            effects_root.join("comparisons.json"),
            serde_json::to_vec_pretty(&effects)?,
        )?;
    }
    fs::write(
        effects_root.join("failures.json"),
        serde_json::to_vec_pretty(&failures)?,
    )?;
    Ok((effects, failures))
}

fn frozen_checkout(bare: &Path, output: &Path) -> Result<std::path::PathBuf> {
    let repo = output.join("repo");
    actions_effects::git(
        output,
        &[
            "clone",
            bare.to_str().ok_or("non-UTF8 frozen Git source")?,
            repo.to_str().ok_or("non-UTF8 frozen checkout")?,
        ],
    )?;
    Ok(repo)
}

fn frozen_request(
    config: &ActionsFrozenConfig<'_>,
    repo: &Path,
    output: &Path,
    env: &[(OsString, OsString)],
    payload: &Value,
) -> Result<Value> {
    let port = TcpListener::bind(("127.0.0.1", 0))?.local_addr()?.port();
    let argv = [
        OsString::from(config.candidate),
        repo.as_os_str().to_os_string(),
        output.join("tmux.log").as_os_str().to_os_string(),
        OsString::from(config.process_child),
        OsString::from(port.to_string()),
    ];
    let mut native = OwnedServer::spawn(
        &argv,
        config.repository,
        &output.join("process.log"),
        env,
        &[],
        false,
    )?;
    let peer = HttpPeer::new(port, Duration::from_secs(5));
    let observed = (|| -> Result<Value> {
        peer.wait_ready("/api/support_terminal/status", Duration::from_secs(10))?;
        let reply = peer.request("POST", "/api/action", &serde_json::to_vec(payload)?)?;
        if reply.status != 200 {
            return Err("frozen action HTTP status changed".into());
        }
        Ok(serde_json::from_slice(&reply.body)?)
    })();
    let stop = native.signal(Signal::TERM);
    let exit = native.finish(Duration::from_secs(5));
    let response = observed?;
    stop?;
    if exit? != 0 {
        return Err("frozen actions server failed cleanup".into());
    }
    Ok(response)
}

fn frozen_tmux_failures(
    config: &ActionsFrozenConfig<'_>,
    original: &OriginalFixture,
    bare: &Path,
) -> Result<Vec<Value>> {
    let scripts = [
        ("missing_tmux", None, 0),
        ("permission_tmux", Some("#!/bin/sh\nexit 0\n"), 0o644),
        (
            "nonzero_tmux",
            Some("#!/bin/sh\nprintf 'owned stderr\\n' >&2; printf ignored; exit 7\n"),
            0o755,
        ),
        (
            "whitespace_tmux",
            Some("#!/bin/sh\nprintf ' \\n' >&2; exit 7\n"),
            0o755,
        ),
        (
            "replacement_tmux",
            Some("#!/bin/sh\nprintf 'owned\\377\\r\\n'\n"),
            0o755,
        ),
    ];
    let mut results = Vec::with_capacity(scripts.len());
    for (name, script, mode) in scripts {
        let output = config.output.join(name);
        fs::create_dir(&output)?;
        let repo = frozen_checkout(bare, &output)?;
        let tools = output.join("tools");
        fs::create_dir(&tools)?;
        let created_program = if let Some(script) = script {
            let program = tools.join("tmux");
            fs::write(&program, script)?;
            fs::set_permissions(&program, fs::Permissions::from_mode(mode))?;
            Some(CreatedProgramGuard::record(
                &program,
                &output.join("created-tmux.jsonl"),
            )?)
        } else {
            None
        };
        let log = output.join("tmux.log");
        fs::write(&log, "old capture must disappear\n")?;
        let env = [(OsString::from("PATH"), tools.into_os_string())];
        let reply = frozen_request(
            config,
            &repo,
            &output,
            &env,
            &json!({"action":"send_tmux_log"}),
        )?;
        let source = original.observation(&format!("actions/{name}/observed.json"))?;
        let bytes = if log.exists() {
            fs::read(&log)?
        } else {
            Vec::new()
        };
        let expected = source["source_bytes"]
            .as_array()
            .ok_or("frozen tmux source bytes absent")?
            .iter()
            .map(|byte| {
                byte.as_u64()
                    .and_then(|value| u8::try_from(value).ok())
                    .ok_or("frozen tmux source byte invalid")
            })
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let receipt = json!({"scenario":name,"source":source["source"],"native":reply,
            "source_bytes":expected,"native_bytes":bytes});
        fs::write(
            output.join("comparison.json"),
            serde_json::to_vec_pretty(&receipt)?,
        )?;
        if reply != source["source"]
            || bytes != expected
            || (name != "replacement_tmux" && log.exists())
        {
            return Err(format!("frozen tmux capture mismatch: {name}").into());
        }
        if let Some(program) = created_program {
            program.finish()?;
        }
        results.push(receipt);
    }
    Ok(results)
}

fn frozen_reboot_cases(
    config: &ActionsFrozenConfig<'_>,
    original: &OriginalFixture,
    bare: &Path,
) -> Result<Vec<Value>> {
    let mut receipts = Vec::new();
    for (name, program) in [
        ("sudo_reboot", Some("sudo")),
        ("fallback_reboot", Some("reboot")),
        ("missing_reboot", None),
    ] {
        let output = config.output.join(name);
        fs::create_dir(&output)?;
        let repo = frozen_checkout(bare, &output)?;
        let tools = output.join("tools");
        fs::create_dir(&tools)?;
        let created_program = if let Some(program) = program {
            let path = tools.join(program);
            fs::write(&path, "#!/bin/sh\nprintf '%s\\n' \"$$\" > \"$CARROT_FIXTURE_REBOOT_PID\"\nprintf '%s\\n' \"$0 $*\" >> \"$CARROT_FIXTURE_REBOOT_RECORD\"\n")?;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
            Some(CreatedProgramGuard::record(
                &path,
                &output.join("created-reboot-tool.jsonl"),
            )?)
        } else {
            None
        };
        let record = output.join("reboot.argv");
        let pid_record = output.join("reboot.pid");
        let env = [
            (OsString::from("PATH"), tools.as_os_str().to_os_string()),
            (
                OsString::from("CARROT_FIXTURE_REBOOT_RECORD"),
                record.as_os_str().to_os_string(),
            ),
            (
                OsString::from("CARROT_FIXTURE_REBOOT_PID"),
                pid_record.as_os_str().to_os_string(),
            ),
        ];
        let port = TcpListener::bind(("127.0.0.1", 0))?.local_addr()?.port();
        let argv = [
            OsString::from(config.candidate),
            repo.as_os_str().to_os_string(),
            output.join("tmux.log").as_os_str().to_os_string(),
            OsString::from(config.process_child),
            OsString::from(port.to_string()),
        ];
        let mut native = OwnedServer::spawn(
            &argv,
            config.repository,
            &output.join("process.log"),
            &env,
            &[],
            false,
        )?;
        let peer = HttpPeer::new(port, Duration::from_secs(5));
        let observed = (|| -> Result<Value> {
            peer.wait_ready("/api/support_terminal/status", Duration::from_secs(10))?;
            let response = peer.request("POST", "/api/action", br#"{"action":"git_reboot"}"#)?;
            if response.status != 200 {
                return Err("frozen reboot HTTP status changed".into());
            }
            let reply: Value = serde_json::from_slice(&response.body)?;
            if program.is_some() {
                let deadline = Instant::now() + Duration::from_secs(3);
                while !pid_record.exists() || !record.exists() {
                    if Instant::now() >= deadline {
                        return Err("frozen reboot child did not execute".into());
                    }
                    thread::sleep(Duration::from_millis(10));
                }
                let pid: u32 = fs::read_to_string(&pid_record)?.trim().parse()?;
                if pid == 0 {
                    return Err("frozen reboot child PID invalid".into());
                }
                while Path::new(&format!("/proc/{pid}")).exists() {
                    if Instant::now() >= deadline {
                        return Err("frozen reboot child was not reaped while server alive".into());
                    }
                    thread::sleep(Duration::from_millis(10));
                }
            }
            Ok(reply)
        })();
        let stop = native.signal(Signal::TERM);
        let exit = native.finish(Duration::from_secs(5));
        let reply = observed?;
        stop?;
        if exit? != 0 {
            return Err("frozen reboot server cleanup failed".into());
        }
        let source = original.observation(&format!("actions/{name}/source-evidence/reply.json"))?;
        let observed_record = if record.exists() {
            fs::read_to_string(&record)?
        } else {
            String::new()
        };
        if let Some(program) = program {
            let source_record = String::from_utf8(
                original
                    .opaque_payload(&format!("actions/{name}/source-evidence/reboot.argv.json"))?,
            )?;
            let suffix = if program == "sudo" {
                "/tools/sudo reboot\n"
            } else {
                "/tools/reboot \n"
            };
            let expected = format!(
                "{}{}\n",
                tools.join(program).display(),
                if program == "sudo" { " reboot" } else { " " }
            );
            if !source_record.ends_with(suffix) || observed_record != expected {
                return Err(format!("frozen reboot executable/argv changed: {name}").into());
            }
        } else if !observed_record.is_empty() || pid_record.exists() {
            return Err("missing reboot unexpectedly executed a child".into());
        }
        let receipt = json!({"scenario":name,"source":source,"native":reply,"record":observed_record,
            "program":program,"reaped":program.is_some()});
        fs::write(
            output.join("comparison.json"),
            serde_json::to_vec_pretty(&receipt)?,
        )?;
        if source != reply {
            return Err(format!("frozen reboot reply changed: {name}").into());
        }
        if let Some(program) = created_program {
            program.finish()?;
        }
        receipts.push(receipt);
    }
    Ok(receipts)
}

fn frozen_reboot_cleanup(config: &ActionsFrozenConfig<'_>, bare: &Path) -> Result<Value> {
    let output = config.output.join("reboot_shutdown");
    fs::create_dir(&output)?;
    let repo = frozen_checkout(bare, &output)?;
    let tools = output.join("tools");
    fs::create_dir(&tools)?;
    let sudo = tools.join("sudo");
    fs::write(&sudo, "#!/bin/sh\nprintf '%s\\n' \"$$\" > \"$CARROT_FIXTURE_REBOOT_PID\"\n/bin/sleep 60 &\nprintf '%s\\n' \"$!\" > \"$CARROT_FIXTURE_REBOOT_DESCENDANT\"\nwait\n")?;
    fs::set_permissions(&sudo, fs::Permissions::from_mode(0o755))?;
    let created_sudo =
        CreatedProgramGuard::record(&sudo, &output.join("created-reboot-cleanup-sudo.jsonl"))?;
    let leader_path = output.join("child.pid");
    let descendant_path = output.join("descendant.pid");
    let mut unrelated = Command::new("/bin/sleep").arg("60").spawn()?;
    let observed = (|| -> Result<Value> {
        let env = [
            (OsString::from("PATH"), tools.as_os_str().to_os_string()),
            (
                OsString::from("CARROT_FIXTURE_REBOOT_PID"),
                leader_path.as_os_str().to_os_string(),
            ),
            (
                OsString::from("CARROT_FIXTURE_REBOOT_DESCENDANT"),
                descendant_path.as_os_str().to_os_string(),
            ),
        ];
        let port = TcpListener::bind(("127.0.0.1", 0))?.local_addr()?.port();
        let argv = [
            OsString::from(config.candidate),
            repo.as_os_str().to_os_string(),
            output.join("tmux.log").as_os_str().to_os_string(),
            OsString::from(config.process_child),
            OsString::from(port.to_string()),
        ];
        let mut native = OwnedServer::spawn(
            &argv,
            config.repository,
            &output.join("process.log"),
            &env,
            &[],
            false,
        )?;
        let peer = HttpPeer::new(port, Duration::from_secs(5));
        let request = (|| -> Result<(Value, f64, u32, u32)> {
            peer.wait_ready("/api/support_terminal/status", Duration::from_secs(10))?;
            let started = Instant::now();
            let response = peer.request("POST", "/api/action", br#"{"action":"git_reboot"}"#)?;
            let elapsed = started.elapsed().as_secs_f64();
            let reply: Value = serde_json::from_slice(&response.body)?;
            if response.status != 200
                || reply != json!({"ok":true,"rebooting":true})
                || elapsed >= 2.0
            {
                return Err("frozen asynchronous reboot response changed".into());
            }
            let deadline = Instant::now() + Duration::from_secs(3);
            while !leader_path.exists() || !descendant_path.exists() {
                if Instant::now() >= deadline {
                    return Err("frozen reboot descendants did not start".into());
                }
                thread::sleep(Duration::from_millis(10));
            }
            let leader: u32 = fs::read_to_string(&leader_path)?.trim().parse()?;
            let descendant: u32 = fs::read_to_string(&descendant_path)?.trim().parse()?;
            if leader == 0
                || descendant == 0
                || leader == descendant
                || !Path::new(&format!("/proc/{leader}")).exists()
                || !Path::new(&format!("/proc/{descendant}")).exists()
                || unrelated.try_wait()?.is_some()
            {
                return Err("frozen reboot ownership before cleanup changed".into());
            }
            Ok((reply, elapsed, leader, descendant))
        })();
        let stop = native.signal(Signal::TERM);
        let exit = native.finish(Duration::from_secs(5));
        let (reply, elapsed, leader, descendant) = request?;
        stop?;
        if exit? != 0
            || Path::new(&format!("/proc/{leader}")).exists()
            || Path::new(&format!("/proc/{descendant}")).exists()
            || unrelated.try_wait()?.is_some()
        {
            return Err("frozen reboot owned-group cleanup changed".into());
        }
        Ok(
            json!({"scenario":"native owned asynchronous reboot cleanup", "reply":reply,
            "response_seconds":elapsed,"leader_reaped":true,"descendant_reaped":true,
            "unrelated_survived":true,"pass":true}),
        )
    })();
    let _ = unrelated.kill();
    let _ = unrelated.wait();
    if let Ok(receipt) = &observed {
        fs::write(
            output.join("cleanup.json"),
            serde_json::to_vec_pretty(receipt)?,
        )?;
    }
    if observed.is_ok() {
        created_sudo.finish()?;
    }
    observed
}

/// Execute the complete Rust-owned action/effect roster against sealed
/// original-only responses and source Git data. The Python controller and
/// its two helper modules are never imported or executed here.
pub fn run_actions_frozen(
    config: &ActionsFrozenConfig<'_>,
    original: &OriginalFixture,
) -> Result<()> {
    original.require_complete_family()?;
    fs::create_dir(config.output)?;
    let source = Pin::open(config.source_server, 4 * 1024 * 1024)?;
    let repo_update = Pin::open(config.repo_update, 1024 * 1024)?;
    let candidate = Pin::open(config.candidate, 256 * 1024 * 1024)?;
    let child = Pin::open(config.process_child, 256 * 1024 * 1024)?;
    let tmux = Pin::open(config.tmux, 32 * 1024 * 1024)?;
    let historical = original.observation("actions/controller-execution.json")?;
    if historical["exit"] != 0
        || historical["phase"] != "all"
        || historical["paired_scenarios"] != 36
        || historical["source_capture_complete"] != true
        || historical["native_owned_effects_accepted"] != false
        || historical["source_sha256"].as_str() != Some(source.digest())
        || original.original_input_sha256("openpilot/common/repo_update.py")?
            != repo_update.digest()
        || historical["argv"]
            .as_array()
            .is_none_or(|argv| argv.len() != 11)
    {
        return Err("frozen actions independent capture changed".into());
    }
    let pair_output = config.output.join("rust-owned-native-bin");
    fs::create_dir(&pair_output)?;
    let pair = actions_effects::OwnedExecutablePair::create(
        config.candidate,
        config.process_child,
        &pair_output,
    )?;
    let bare = actions_effects::frozen_bare(config.output)?;
    let (primary, mut failures) = replay_frozen_primary(config, original, &bare, &pair)?;
    let source_log = original.opaque_payload("actions/tmux_capture/source.log.json")?;
    let source_log_path = config.output.join("source-tmux.log");
    fs::write(&source_log_path, source_log)?;
    let tmux_receipt = actions_effects::capture_tmux(
        config.repository,
        config.candidate,
        config.process_child,
        config.tmux,
        &bare,
        &source_log_path,
        &config.output.join("rust-owned-tmux"),
    );
    let source_tmux = original.observation("actions/tmux_capture/comparison.json")?;
    match &tmux_receipt {
        Ok(receipt) if receipt["response"] == source_tmux["source"] => {}
        Ok(_) => failures.push("frozen original tmux response changed".into()),
        Err(error) => failures.push(format!("real tmux capture: {error}")),
    }
    let pty_output = config.output.join("rust-owned-pty");
    fs::create_dir(&pty_output)?;
    pair.record("before-pty")?;
    let pty_receipt = actions_effects::inherited_pty_bound(&pair, &bare, &pty_output);
    pair.record("after-pty")?;
    let source_pty = original.observation("actions/inherited_pty/comparison.json")?;
    match &pty_receipt {
        Ok(receipt)
            if source_pty["source"]["exit"] == receipt["exit"]
                && source_pty["source"]["all_stdio_tty"] == receipt["all_stdio_tty"]
                && source_pty["source"]["stdin_observed"] == receipt["stdin_observed"] => {}
        Ok(_) => failures.push("frozen original inherited PTY relation changed".into()),
        Err(error) => failures.push(format!("inherited PTY: {error}")),
    }
    let failed_tmux = frozen_tmux_failures(config, original, &bare);
    if let Err(error) = &failed_tmux {
        failures.push(format!("tmux failure controls: {error}"));
    }
    let reboot = frozen_reboot_cases(config, original, &bare);
    if let Err(error) = &reboot {
        failures.push(format!("reboot controls: {error}"));
    }
    let cleanup = frozen_reboot_cleanup(config, &bare);
    if let Err(error) = &cleanup {
        failures.push(format!("reboot group cleanup: {error}"));
    }
    for name in ["git_rebuild", "rebuild_all"] {
        let source_effects = original.observation(&format!(
            "actions/{name}_effects/source-evidence/effects.json"
        ))?;
        if source_effects["scons"] != "-c\n"
            || source_effects["prebuilt_removed"] != true
            || (name == "git_rebuild"
                && (source_effects["sign_removed"] != true
                    || source_effects["cache_removed"] != true))
            || (name == "rebuild_all" && source_effects["sudo"] != "reboot\n")
        {
            failures.push(format!(
                "frozen original destructive rebuild control changed: {name}"
            ));
        }
    }
    for pin in [&source, &repo_update, &candidate, &child, &tmux] {
        pin.verify()?;
    }
    pair.record("after-family")?;
    fs::write(
        config.output.join("rust-owned-failures.json"),
        serde_json::to_vec_pretty(&failures)?,
    )?;
    if !failures.is_empty() {
        return Err(format!(
            "{} independent frozen actions failed: {}",
            failures.len(),
            failures.join(" | ")
        )
        .into());
    }
    let tmux_receipt = tmux_receipt?;
    let pty_receipt = pty_receipt?;
    let failed_tmux = failed_tmux?;
    let reboot = reboot?;
    let cleanup = cleanup?;
    let cases = primary.len() + failed_tmux.len() + reboot.len() + 1 + 1 + 2;
    if cases != SCENARIOS.len() + EXTRAS.len() || cleanup["pass"] != true {
        return Err("frozen actions scenario roster incomplete".into());
    }
    fs::write(
        config.output.join("admission.json"),
        serde_json::to_vec_pretty(&json!({"pass":true,"original_only":true,
            "source_scenarios":36,"owned_primary":primary.len(),"owned_tmux_errors":failed_tmux.len(),
            "owned_reboot":reboot.len(),"owned_rebuild_effects":2,
            "owned_real_tmux":tmux_receipt["pass"],"owned_inherited_pty":pty_receipt["pass"],
            "owned_reboot_cleanup":cleanup["pass"],"source_server_sha256":source.digest(),
            "repo_update_sha256":repo_update.digest(),"candidate_sha256":candidate.digest(),
            "child_sha256":child.digest(),"tmux_sha256":tmux.digest()}))?,
    )?;
    Ok(())
}

pub(super) fn staged_verb(action: &str) -> Option<&'static str> {
    match action {
        "git_pull" => Some("__git-pull"),
        "git_sync" => Some("__fetch-native"),
        "git_reset" | "git_rebuild" | "rebuild_all" => Some("__stage-repair"),
        "git_checkout" => Some("__stage-checkout"),
        "git_checkout_commit" => Some("__stage-commit"),
        "git_reset_repo" => Some("__stage-reset-repo"),
        _ => None,
    }
}
