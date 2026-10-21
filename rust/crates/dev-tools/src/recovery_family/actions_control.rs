//! Independent Rust-owned Git, tmux and PTY effects against unchanged source
//! observations. The hosted family gate still executes the Python controller.

use super::{actions_effects, HttpPeer, OwnedServer, Pin, Result};
use crate::qa_process::{CommandSpec, NativeRunner, Runner};
use rustix::process::Signal;
use serde_json::{json, Value};
use std::{ffi::OsString, fs, net::TcpListener, path::Path, time::Duration};

pub struct ActionsCorpusConfig<'a> {
    pub repository: &'a Path,
    pub python: &'a Path,
    pub controller: &'a Path,
    pub fixtures: &'a Path,
    pub tools: &'a Path,
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

fn staged_verb(action: &str) -> Option<&'static str> {
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

fn replay_scenario(
    config: &ActionsCorpusConfig<'_>,
    name: &str,
    destination: &Path,
) -> Result<Value> {
    fs::create_dir(destination)?;
    let root = config.output.join(name);
    let comparison: Value = serde_json::from_slice(&fs::read(root.join("comparison.json"))?)?;
    let payload = comparison
        .get("payload")
        .ok_or("missing original actions payload")?;
    let action = payload["action"]
        .as_str()
        .ok_or("missing original action")?;
    let source: Value =
        serde_json::from_slice(&fs::read(root.join("source-evidence/reply.json"))?)?;
    let previous_native: Value =
        serde_json::from_slice(&fs::read(root.join("native-evidence/reply.json"))?)?;
    let corrected = staged_verb(action).is_some() && previous_native.get("command").is_some();
    if comparison["staged_contract"] != corrected {
        return Err("recovery staged action classification changed".into());
    }
    if corrected {
        let command = previous_native["command"]
            .as_str()
            .ok_or("missing native staged command")?;
        let verb = staged_verb(action).ok_or("missing staged verb")?;
        if !command.split_ascii_whitespace().any(|word| word == verb)
            || ["scons", "git clean", "git reset --hard", "ajouatom"]
                .iter()
                .any(|forbidden| command.contains(forbidden))
        {
            return Err("recovery staged action command contract changed".into());
        }
    } else if previous_native != source {
        return Err("recovery original/native equal-action control changed".into());
    }
    let port = TcpListener::bind(("127.0.0.1", 0))?.local_addr()?.port();
    let argv = vec![
        OsString::from(config.candidate),
        OsString::from(root.join("native")),
        OsString::from(destination.join("tmux.log")),
        OsString::from(config.process_child),
        OsString::from(port.to_string()),
    ];
    let environment = [
        (
            OsString::from("CARROT_REPO_LOCK_PATH"),
            OsString::from(root.join("transaction.lock")),
        ),
        (
            OsString::from("GIT_CONFIG_GLOBAL"),
            OsString::from(root.join("global.gitconfig")),
        ),
        (OsString::from("GIT_CONFIG_NOSYSTEM"), OsString::from("1")),
        (OsString::from("GIT_ALLOW_PROTOCOL"), OsString::from("file")),
    ];
    let mut native = OwnedServer::spawn(
        &argv,
        config.repository,
        &destination.join("process.log"),
        &environment,
        &[],
        false,
    )?;
    let peer = HttpPeer::new(port, Duration::from_secs(5));
    let observed = (|| -> Result<(u16, Value)> {
        peer.wait_ready("/api/support_terminal/status", Duration::from_secs(10))?;
        let response = peer.request("POST", "/api/action", &serde_json::to_vec(payload)?)?;
        Ok((response.status, serde_json::from_slice(&response.body)?))
    })();
    let stop = native.signal(Signal::TERM);
    let exit = native.finish(Duration::from_secs(5));
    let (status, actual) = observed?;
    stop?;
    let exit = exit?;
    let expected = if corrected { &previous_native } else { &source };
    let matching = status == 200 && &actual == expected;
    let receipt = json!({"scenario":name,"source":source,"prior_native":previous_native,"direct_native":actual,
        "staged_contract":corrected,"http_status":status,"exit":exit});
    fs::write(
        destination.join("comparison.json"),
        serde_json::to_vec_pretty(&receipt)?,
    )?;
    if !matching {
        return Err(format!("Rust-driven recovery action mismatch: {name}").into());
    }
    Ok(receipt)
}

pub fn admit_actions_corpus(config: &ActionsCorpusConfig<'_>) -> Result<()> {
    let pins = [
        Pin::open(config.controller, 1024 * 1024)?,
        Pin::open(config.fixtures, 1024 * 1024)?,
        Pin::open(config.tools, 1024 * 1024)?,
        Pin::open(config.source_server, 4 * 1024 * 1024)?,
        Pin::open(config.repo_update, 1024 * 1024)?,
        Pin::open(config.candidate, 256 * 1024 * 1024)?,
        Pin::open(config.tmux, 32 * 1024 * 1024)?,
        Pin::open(config.process_child, 256 * 1024 * 1024)?,
    ];
    let argv = vec![
        config.python.display().to_string(),
        "-P".into(),
        config.controller.display().to_string(),
        "--binary".into(),
        config.candidate.display().to_string(),
        "--output".into(),
        config.output.display().to_string(),
        "--tmux".into(),
        config.tmux.display().to_string(),
        "--phase".into(),
        "all".into(),
    ];
    let mut command = CommandSpec::new(argv.clone(), Duration::from_secs(25 * 60));
    command.cwd = Some(config.repository.to_owned());
    command.env_remove.push("PYTHONOPTIMIZE".into());
    command.stdout_limit = 4 * 1024 * 1024;
    command.stderr_limit = 4 * 1024 * 1024;
    let mut runner = NativeRunner;
    let observed = runner.run(&command)?;
    fs::write(config.output.join("controller.stdout"), &observed.stdout)?;
    fs::write(config.output.join("controller.stderr"), &observed.stderr)?;
    fs::write(
        config.output.join("invocation.json"),
        serde_json::to_vec_pretty(&json!({"argv": argv,"exit":observed.code}))?,
    )?;
    if observed.code != 0 {
        return Err("unchanged recovery actions corpus failed".into());
    }
    let summary: Value = serde_json::from_slice(&fs::read(config.output.join("summary.json"))?)?;
    if summary["pass"] != true
        || summary["phase"] != "all"
        || summary["paired_scenarios"].as_u64() != Some((SCENARIOS.len() + 12) as u64)
        || summary["native_cleanup_scenarios"] != 1
        || summary["source_sha256"].as_str() != Some(pins[3].digest())
        || summary["binary_sha256"].as_str() != Some(pins[5].digest())
    {
        return Err("recovery actions summary changed".into());
    }
    for name in SCENARIOS.iter().chain(EXTRAS) {
        let receipt: Value = serde_json::from_slice(&fs::read(
            config.output.join(*name).join("comparison.json"),
        )?)?;
        if receipt["pass"] != true || receipt["scenario"].as_str().is_none() {
            return Err(format!("recovery actions corpus receipt changed: {name}").into());
        }
    }
    // First compare every action through a separate native HTTP/process peer.
    // The second lane below creates fresh Git repositories and executes the
    // returned commands, comparing their effects to source-backed receipts.
    let replay = config.output.join("native-direct-replay");
    fs::create_dir(&replay)?;
    let mut direct = Vec::new();
    for name in SCENARIOS {
        direct.push(replay_scenario(config, name, &replay.join(name))?);
        fs::write(
            replay.join("comparisons.json"),
            serde_json::to_vec_pretty(&direct)?,
        )?;
    }
    let tmux_receipt = actions_effects::capture_tmux(
        config.repository,
        config.candidate,
        config.process_child,
        config.tmux,
        &config.output.join("origin.git"),
        &config.output.join("tmux_capture/source.log"),
        &config.output.join("rust-owned-tmux"),
    )?;
    let pty_receipt = actions_effects::inherited_pty(
        config.candidate,
        &config.output.join("origin.git"),
        &config.output.join("rust-owned-pty"),
    )?;
    let owned_effects = config.output.join("rust-owned-effects");
    fs::create_dir(&owned_effects)?;
    let mut effects = Vec::new();
    for name in SCENARIOS {
        let comparison: Value =
            serde_json::from_slice(&fs::read(config.output.join(name).join("comparison.json"))?)?;
        effects.push(actions_effects::run(
            config.repository,
            config.candidate,
            config.process_child,
            &config.output.join("origin.git"),
            &comparison,
            name,
            &owned_effects.join(name),
        )?);
        fs::write(
            owned_effects.join("comparisons.json"),
            serde_json::to_vec_pretty(&effects)?,
        )?;
    }
    let cleanup: Value = serde_json::from_slice(&fs::read(
        config.output.join("reboot_shutdown/cleanup.json"),
    )?)?;
    if cleanup["pass"] != true
        || cleanup["leader_reaped"] != true
        || cleanup["descendant_reaped"] != true
        || cleanup["unrelated_survived"] != true
    {
        return Err("recovery reboot descendant cleanup changed".into());
    }
    for pin in &pins {
        pin.verify()?;
    }
    fs::write(
        config.output.join("admission.json"),
        serde_json::to_vec_pretty(&json!({
            "pass":true,"source_controller_required":true,"scenario_count":SCENARIOS.len(),
        "extra_count":EXTRAS.len(),"rust_driven_native_action_pairs":direct.len(),"source_sha256":pins[3].digest(),
            "candidate_sha256":pins[5].digest(),"tmux_sha256":pins[6].digest(),
            "rust_owned_git_effects":effects.len(),
            "rust_owned_tmux":tmux_receipt["pass"],
            "rust_owned_pty":pty_receipt["pass"],
        }))?,
    )?;
    Ok(())
}
