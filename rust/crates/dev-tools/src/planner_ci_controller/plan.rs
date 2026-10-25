//! 기존 12개 실제 비교 leaf와 PASS provenance. Python은 현재 원본 비교 leaf에만 명시한다.
use super::{write_json, Check};
use crate::{
    qa_process::{CommandSpec, Runner},
    seal::Result,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};

pub const CHECK_NAMES: [&str; 12] = [
    "abi",
    "policy",
    "radar",
    "gap",
    "owner",
    "main",
    "fault",
    "volkswagen",
    "stock-longitudinal",
    "radar-off",
    "ipc",
    "lifecycle",
];

#[derive(Clone, Debug)]
pub struct Inputs {
    pub root: PathBuf,
    pub python: PathBuf,
    pub binaries: PathBuf,
    pub artifact: PathBuf,
    pub source: PathBuf,
    pub binding: PathBuf,
    pub scheduler: PathBuf,
}

fn text(path: &Path) -> Result<String> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| "Planner paths must be UTF-8".into())
}

pub fn runtime_checks(
    inputs: &Inputs,
    growth: &BTreeMap<String, u64>,
    timeout: Duration,
) -> Result<Vec<Check>> {
    if growth.len() != CHECK_NAMES.len()
        || CHECK_NAMES.iter().any(|name| !growth.contains_key(*name))
    {
        return Err(
            "growth admission must name every one of the 12 Planner operations exactly once".into(),
        );
    }
    if timeout.is_zero() {
        return Err("Planner command timeout must be positive".into());
    }
    let examples = inputs.binaries.join("examples");
    let owner = vec![
        "--source-native".into(),
        text(&inputs.source)?,
        "--artifact".into(),
        text(&inputs.artifact)?,
    ];
    let mut runtime = owner.clone();
    runtime.extend([
        "--binary".into(),
        text(&examples.join("plannerd_runtime_trace"))?,
    ]);
    let mut daemon = owner.clone();
    daemon.extend([
        "--python".into(),
        text(&inputs.python)?,
        "--binding".into(),
        text(&inputs.binding)?,
        "--binary".into(),
        text(&inputs.binaries.join("openpilot-plannerd"))?,
    ]);
    let mut checks = Vec::new();
    let mut add =
        |name: &str, script: &str, arguments: Vec<String>, output_option: &str| -> Result<()> {
            let mut argv = vec![
                text(&inputs.python)?,
                "-u".into(),
                "-P".into(),
                text(&inputs.root.join("rust/tools").join(script))?,
            ];
            argv.extend(arguments);
            let mut command = CommandSpec::new(argv, timeout);
            if name == "ipc" {
                command
                    .env
                    .insert("PLANNER_NATIVE_SCHEDULER".into(), text(&inputs.scheduler)?);
            }
            checks.push(Check {
                name: name.into(),
                command,
                output_option: output_option.into(),
                growth_bytes: growth[name],
                scheduler_calls: (name == "ipc").then_some(4 * 40),
            });
            Ok(())
        };
    add(
        "abi",
        "plannerd_acados_source.py",
        vec![
            "--baseline".into(),
            text(&inputs.source)?,
            "--artifact".into(),
            text(&inputs.artifact)?,
            "--trace".into(),
            text(&examples.join("plannerd_solver_trace"))?,
        ],
        "--evidence",
    )?;
    add(
        "policy",
        "plannerd_policy_source.py",
        vec![
            "--trace".into(),
            text(&examples.join("plannerd_policy_trace"))?,
        ],
        "--evidence",
    )?;
    for (name, script, trace) in [
        ("radar", "plannerd_radar_source.py", "radar"),
        ("gap", "plannerd_gap_source.py", "gap"),
    ] {
        add(
            name,
            script,
            vec![
                "--binary".into(),
                text(&examples.join(format!("plannerd_{trace}_trace")))?,
            ],
            "--output",
        )?;
    }
    let mut owner_trace = owner;
    owner_trace.extend([
        "--binary".into(),
        text(&examples.join("plannerd_owner_trace"))?,
    ]);
    add("owner", "plannerd_owner_source.py", owner_trace, "--output")?;
    for (name, extra) in [
        ("main", vec![]),
        ("fault", vec!["--faults", "--frames", "80"]),
        ("volkswagen", vec!["--brand", "volkswagen"]),
        ("stock-longitudinal", vec!["--stock-longitudinal"]),
        ("radar-off", vec!["--radar-mode", "0"]),
    ] {
        let mut arguments = runtime.clone();
        arguments.extend(extra.into_iter().map(str::to_owned));
        add(name, "plannerd_runtime_source.py", arguments, "--output")?;
    }
    add("ipc", "plannerd_ipc.py", daemon.clone(), "--output")?;
    add(
        "lifecycle",
        "check_plannerd_lifecycle.py",
        daemon,
        "--output",
    )?;
    Ok(checks)
}

fn digest(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

/// run_checks가 모두 성공한 뒤에만 호출한다. 원본의 실행물·binding·solver 식별을 유지한다.
pub fn write_receipt(
    inputs: &Inputs,
    checks: &[Check],
    output: &Path,
    runner: &mut dyn Runner,
) -> Result<Value> {
    let mut binaries = vec![
        inputs.binaries.join("openpilot-plannerd"),
        inputs.binding.clone(),
        inputs.artifact.join("manifest.json"),
    ];
    binaries.extend(
        ["solver", "policy", "radar", "gap", "owner", "runtime"].map(|name| {
            inputs
                .binaries
                .join("examples")
                .join(format!("plannerd_{name}_trace"))
        }),
    );
    let mut hashes = BTreeMap::new();
    for binary in binaries {
        hashes.insert(text(&binary)?, digest(&binary)?);
    }
    let mut command = CommandSpec::new(
        vec!["git".into(), "rev-parse".into(), "HEAD".into()],
        Duration::from_secs(10),
    );
    command.cwd = Some(inputs.root.clone());
    let source = runner.run(&command)?;
    if source.code != 0 {
        return Err("cannot record Planner source commit".into());
    }
    let source_commit = std::str::from_utf8(&source.stdout)?.trim();
    if source_commit.len() != 40 || !source_commit.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("invalid Planner source commit".into());
    }
    let receipt = json!({"status":"PASS", "architecture":std::env::consts::ARCH,
        "checks":checks.iter().map(|check| &check.name).collect::<Vec<_>>(), "source_commit":source_commit,
        "sha256":hashes, "scope":"exact source policies, main loops, native IPC and lifecycle on the stated host; no device acceptance"});
    write_json(&output.join("result.json"), &receipt)?;
    Ok(receipt)
}
