//! 기존 12개 실제 비교 leaf와 PASS provenance. IPC는 같은 ELF의 직접 child다.
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
pub const MEMORY_CHECK_NAMES: [&str; 3] = ["abi", "owner", "fault"];

#[derive(Clone, Debug)]
pub struct NativeIpcInputs {
    pub original_ipc_binding: PathBuf,
    pub compiler: PathBuf,
    pub corpus: PathBuf,
    pub corpus_audit: PathBuf,
    pub corpus_audit_sha256: String,
    pub timeout: Duration,
    pub growth_mib: u64,
    pub peer_build_growth_mib: u64,
}

#[derive(Clone, Debug)]
pub struct NativeLifecycleInputs {
    pub original_ipc_binding: PathBuf,
    pub corpus: PathBuf,
    pub corpus_audit: PathBuf,
    pub growth_mib: u64,
}

#[derive(Clone, Debug)]
pub struct Inputs {
    pub root: PathBuf,
    pub python: PathBuf,
    pub binaries: PathBuf,
    pub artifact: PathBuf,
    pub source: PathBuf,
    pub binding: PathBuf,
    pub scheduler: PathBuf,
    pub native_ipc: Option<NativeIpcInputs>,
    pub native_lifecycle: Option<NativeLifecycleInputs>,
}

fn text(path: &Path) -> Result<String> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| "Planner paths must be UTF-8".into())
}

/// 미해결 IPC는 argv가 아니다. 실행 iterator가 이 단계에 도달한 뒤에만 완성한다.
pub(crate) enum PlannedCheck {
    Complete(Box<Check>),
    NativeIpc {
        growth_bytes: u64,
        timeout: Duration,
    },
    NativeLifecycle {
        growth_bytes: u64,
        timeout: Duration,
    },
}

impl PlannedCheck {
    pub(crate) fn complete(self, inputs: &Inputs) -> Result<Check> {
        match self {
            Self::Complete(check) => Ok(*check),
            Self::NativeIpc {
                growth_bytes,
                timeout,
            } => native_ipc_check(inputs, growth_bytes, timeout),
            Self::NativeLifecycle {
                growth_bytes,
                timeout,
            } => native_lifecycle_check(inputs, growth_bytes, timeout),
        }
    }
}

pub(crate) fn runtime_plan(
    inputs: &Inputs,
    growth: &BTreeMap<String, u64>,
    timeout: Duration,
) -> Result<Vec<PlannedCheck>> {
    selected_checks(inputs, growth, timeout, &CHECK_NAMES)
}

pub fn runtime_checks(
    inputs: &Inputs,
    growth: &BTreeMap<String, u64>,
    timeout: Duration,
) -> Result<Vec<Check>> {
    runtime_plan(inputs, growth, timeout)?
        .into_iter()
        .map(|step| step.complete(inputs))
        .collect()
}

pub fn memory_checks(
    inputs: &Inputs,
    growth: &BTreeMap<String, u64>,
    timeout: Duration,
) -> Result<Vec<Check>> {
    selected_checks(inputs, growth, timeout, &MEMORY_CHECK_NAMES)?
        .into_iter()
        .map(|step| step.complete(inputs))
        .collect()
}

fn selected_checks(
    inputs: &Inputs,
    growth: &BTreeMap<String, u64>,
    timeout: Duration,
    names: &[&str],
) -> Result<Vec<PlannedCheck>> {
    if growth.len() != names.len() || names.iter().any(|name| !growth.contains_key(*name)) {
        return Err(format!(
            "growth admission must name every selected Planner operation exactly once: {}",
            names.join(",")
        )
        .into());
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
    let mut checks = Vec::new();
    let mut add =
        |name: &str, script: &str, arguments: Vec<String>, output_option: &str| -> Result<()> {
            let Some(growth_bytes) = growth.get(name) else {
                return Ok(());
            };
            if name == "ipc" {
                checks.push(PlannedCheck::NativeIpc {
                    growth_bytes: *growth_bytes,
                    timeout,
                });
                return Ok(());
            }
            if name == "lifecycle" {
                checks.push(PlannedCheck::NativeLifecycle {
                    growth_bytes: *growth_bytes,
                    timeout,
                });
                return Ok(());
            }
            let mut argv = vec![
                text(&inputs.python)?,
                "-u".into(),
                "-P".into(),
                text(&inputs.root.join("rust/tools").join(script))?,
            ];
            argv.extend(arguments);
            let command = CommandSpec::new(argv, timeout);
            checks.push(PlannedCheck::Complete(Box::new(Check {
                name: name.into(),
                command,
                output_option: output_option.into(),
                growth_bytes: *growth_bytes,
                scheduler_calls: None,
            })));
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
    add("ipc", "", Vec::new(), "--output")?;
    add("lifecycle", "", Vec::new(), "--output")?;
    Ok(checks)
}

fn native_lifecycle_check(inputs: &Inputs, growth_bytes: u64, timeout: Duration) -> Result<Check> {
    let lifecycle = inputs
        .native_lifecycle
        .as_ref()
        .ok_or("prepared native lifecycle context is required")?;
    if [
        &lifecycle.original_ipc_binding,
        &lifecycle.corpus,
        &lifecycle.corpus_audit,
    ]
    .iter()
    .any(|path| !path.is_absolute())
        || lifecycle.growth_mib == 0
        || lifecycle.growth_mib.checked_mul(1024 * 1024).is_none()
        || timeout.is_zero()
        || timeout.subsec_nanos() != 0
    {
        return Err("invalid prepared native lifecycle paths or budgets".into());
    }
    let argv = vec![
        text(&inputs.scheduler)?,
        "lifecycle".into(),
        "--root".into(),
        text(&inputs.root)?,
        "--python".into(),
        text(&inputs.python)?,
        "--binary".into(),
        text(&inputs.binaries.join("openpilot-plannerd"))?,
        "--artifact".into(),
        text(&inputs.artifact)?,
        "--binding".into(),
        text(&inputs.binding)?,
        "--source-native".into(),
        text(&inputs.source)?,
        "--original-ipc-binding".into(),
        text(&lifecycle.original_ipc_binding)?,
        "--corpus".into(),
        text(&lifecycle.corpus)?,
        "--corpus-audit".into(),
        text(&lifecycle.corpus_audit)?,
        "--timeout-seconds".into(),
        timeout.as_secs().to_string(),
        "--growth-mib".into(),
        lifecycle.growth_mib.to_string(),
    ];
    Ok(Check {
        name: "lifecycle".into(),
        command: CommandSpec::new(argv, timeout),
        output_option: "--output".into(),
        growth_bytes,
        scheduler_calls: None,
    })
}

fn native_ipc_check(inputs: &Inputs, growth_bytes: u64, timeout: Duration) -> Result<Check> {
    let ipc = inputs
        .native_ipc
        .as_ref()
        .ok_or("prepared native IPC context is required")?;
    if [
        &ipc.original_ipc_binding,
        &ipc.compiler,
        &ipc.corpus,
        &ipc.corpus_audit,
    ]
    .into_iter()
    .any(|path| !path.is_absolute())
        || ipc.timeout.as_secs() == 0
        || ipc.timeout.subsec_nanos() != 0
        || ipc.timeout > timeout
        || ipc.growth_mib == 0
        || ipc.peer_build_growth_mib == 0
        || ipc.corpus_audit_sha256.len() != 64
        || !ipc
            .corpus_audit_sha256
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("invalid prepared native IPC paths, SHA or budgets".into());
    }
    let argv = vec![
        text(&inputs.scheduler)?,
        "ipc".into(),
        "--root".into(),
        text(&inputs.root)?,
        "--python".into(),
        text(&inputs.python)?,
        "--binary".into(),
        text(&inputs.binaries.join("openpilot-plannerd"))?,
        "--artifact".into(),
        text(&inputs.artifact)?,
        "--binding".into(),
        text(&inputs.binding)?,
        "--source-native".into(),
        text(&inputs.source)?,
        "--original-ipc-binding".into(),
        text(&ipc.original_ipc_binding)?,
        "--scheduler".into(),
        text(&inputs.scheduler)?,
        "--compiler".into(),
        text(&ipc.compiler)?,
        "--corpus".into(),
        text(&ipc.corpus)?,
        "--corpus-audit".into(),
        text(&ipc.corpus_audit)?,
        "--corpus-audit-sha256".into(),
        ipc.corpus_audit_sha256.clone(),
        "--timeout-seconds".into(),
        ipc.timeout.as_secs().to_string(),
        "--growth-mib".into(),
        ipc.growth_mib.to_string(),
        "--peer-build-growth-mib".into(),
        ipc.peer_build_growth_mib.to_string(),
    ];
    let mut command = CommandSpec::new(argv, timeout);
    command
        .env
        .insert("PLANNER_NATIVE_SCHEDULER".into(), text(&inputs.scheduler)?);
    Ok(Check {
        name: "ipc".into(),
        command,
        output_option: "--output".into(),
        growth_bytes,
        scheduler_calls: Some(4 * 40),
    })
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
