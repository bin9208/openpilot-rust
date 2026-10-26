use super::{
    memory,
    plan::{self, Inputs},
    run_checks, run_prepared_checks, scheduler,
};
use crate::{qa_process::NativeRunner, seal::Result};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs::{self, OpenOptions},
    io::ErrorKind,
    path::{Path, PathBuf},
    time::Duration,
};

const HELP: &str = "check-plannerd-ci schedule\ncheck-plannerd-ci [memory [--asan-library PATH]] --root PATH --python PATH --bin-dir PATH --artifact PATH --source-native PATH --binding PATH --output PATH --timeout-seconds N --growth-mib NAME=MIB (all 12 operations, or abi/owner/fault in memory mode; --asan-library prepares sanitizer wrappers and writes the memory receipt)";

#[cfg(feature = "planner-ipc-native")]
const IPC_HELP: &str = concat!(
    "check-plannerd-ci ipc --root PATH --python PATH --binary PATH --artifact PATH ",
    "--binding PATH --source-native PATH --original-ipc-binding PATH --scheduler PATH ",
    "--compiler PATH --corpus PATH --corpus-audit PATH --corpus-audit-sha256 SHA256 ",
    "--output PATH --evidence PATH --timeout-seconds N --growth-mib MIB --peer-build-growth-mib MIB ",
    "(all options required once; absolute UTF-8 paths; positive decimal budgets)"
);

#[cfg(feature = "planner-ipc-native")]
const LIFECYCLE_HELP: &str = concat!(
    "check-plannerd-ci lifecycle --root PATH --python PATH --binary PATH --artifact PATH ",
    "--binding PATH --source-native PATH --original-ipc-binding PATH --corpus PATH --corpus-audit PATH ",
    "--output PATH --timeout-seconds N --growth-mib MIB (all options required once)"
);

#[cfg(feature = "planner-ipc-native")]
pub(super) fn parse_lifecycle_arguments(
    mut arguments: impl Iterator<Item = OsString>,
) -> Result<super::planner_ipc_native::lifecycle::Options> {
    let allowed = [
        "--root",
        "--python",
        "--binary",
        "--artifact",
        "--binding",
        "--source-native",
        "--original-ipc-binding",
        "--corpus",
        "--corpus-audit",
        "--output",
        "--timeout-seconds",
        "--growth-mib",
    ];
    let mut values = BTreeMap::<String, OsString>::new();
    while let Some(key) = arguments.next() {
        let name = key.to_str().ok_or(LIFECYCLE_HELP)?;
        if !allowed.contains(&name) {
            return Err(format!("unknown lifecycle option {name}").into());
        }
        let value = arguments
            .next()
            .ok_or_else(|| format!("missing value for {name}"))?;
        if values.insert(name.into(), value).is_some() {
            return Err(format!("duplicate lifecycle option {name}").into());
        }
    }
    let take = |name: &str, values: &mut BTreeMap<String, OsString>| -> Result<String> {
        values
            .remove(name)
            .ok_or_else(|| format!("missing lifecycle {name}"))?
            .into_string()
            .map_err(|_| format!("lifecycle {name} requires UTF-8").into())
    };
    let path = |name: &str, values: &mut BTreeMap<String, OsString>| -> Result<PathBuf> {
        let value = take(name, values)?;
        if value.is_empty() || value.contains('\0') || !Path::new(&value).is_absolute() {
            return Err(format!("lifecycle {name} requires an absolute UTF-8 path").into());
        }
        Ok(value.into())
    };
    let positive = |name: &str, values: &mut BTreeMap<String, OsString>| -> Result<u64> {
        let value = take(name, values)?;
        if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(format!("lifecycle {name} requires positive decimal").into());
        }
        let number = value.parse::<u64>()?;
        if number == 0 {
            return Err(format!("lifecycle {name} must be positive").into());
        }
        Ok(number)
    };
    let timeout = Duration::from_secs(positive("--timeout-seconds", &mut values)?);
    if std::time::Instant::now().checked_add(timeout).is_none() {
        return Err("lifecycle deadline range".into());
    }
    let growth_bytes = positive("--growth-mib", &mut values)?
        .checked_mul(1024 * 1024)
        .ok_or("lifecycle growth overflow")?;
    let options = super::planner_ipc_native::lifecycle::Options {
        root: path("--root", &mut values)?,
        python: path("--python", &mut values)?,
        binary: path("--binary", &mut values)?,
        artifact: path("--artifact", &mut values)?,
        binding: path("--binding", &mut values)?,
        source_native: path("--source-native", &mut values)?,
        original_ipc_binding: path("--original-ipc-binding", &mut values)?,
        corpus: path("--corpus", &mut values)?,
        corpus_audit: path("--corpus-audit", &mut values)?,
        output: path("--output", &mut values)?,
        timeout,
        growth_bytes,
    };
    if !values.is_empty() {
        return Err("unused lifecycle options".into());
    }
    Ok(options)
}

fn prepared_native_lifecycle(
    inputs: &Inputs,
    output: &Path,
    ipc: &plan::NativeIpcInputs,
    growth_bytes: u64,
) -> Result<plan::NativeLifecycleInputs> {
    #[cfg(not(feature = "planner-ipc-native"))]
    {
        let _ = (inputs, output, ipc, growth_bytes);
        Err("runtime lifecycle requires the planner-ipc-native feature".into())
    }
    #[cfg(feature = "planner-ipc-native")]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let job = PathBuf::from(std::env::var("CI_JOB_TEMP")?);
        let corpus = job.join("planner-lifecycle-original");
        let audit = corpus.join("audit.json");
        if output != job.join("planner-comparison")
            || inputs.root.as_path() != Path::new(&std::env::var("CI296_SOURCE")?)
            || Path::new(&std::env::var("PLANNER_LIFECYCLE_CORPUS")?) != corpus.as_path()
            || Path::new(&std::env::var("PLANNER_LIFECYCLE_CORPUS_AUDIT")?) != audit.as_path()
            || growth_bytes < 128 * 1024 * 1024
            || !growth_bytes.is_multiple_of(1024 * 1024)
        {
            return Err("prepared runtime lifecycle paths or operation growth differ".into());
        }
        let directory = fs::symlink_metadata(&corpus)?;
        let metadata = fs::symlink_metadata(&audit)?;
        let uid = rustix::process::getuid().as_raw();
        if corpus.canonicalize()? != corpus
            || !directory.is_dir()
            || directory.uid() != uid
            || directory.permissions().mode() & 0o777 != 0o700
            || audit.canonicalize()? != audit
            || !metadata.is_file()
            || metadata.uid() != uid
            || metadata.permissions().mode() & 0o777 != 0o600
            || metadata.len() > 1024 * 1024
        {
            return Err(
                "prepared runtime lifecycle requires canonical owned700/600 corpus/audit".into(),
            );
        }
        Ok(plan::NativeLifecycleInputs {
            original_ipc_binding: ipc.original_ipc_binding.clone(),
            corpus,
            corpus_audit: audit,
            growth_mib: growth_bytes / (1024 * 1024),
        })
    }
}

fn require_retired_controller_absent(root: &Path) -> Result<()> {
    for relative in [
        "rust/tools/check_plannerd_ci.py",
        "rust/tools/tests/test_plannerd_ci.py",
    ] {
        match fs::symlink_metadata(root.join(relative)) {
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!(
                    "cannot verify retired Planner controller path {relative}: {error}"
                )
                .into());
            }
            Ok(_) => {
                return Err(format!("retired Planner controller path exists: {relative}").into());
            }
        }
    }
    Ok(())
}

#[cfg(not(feature = "planner-ipc-native"))]
fn prepared_native_ipc(_: &Inputs, _: &Path) -> Result<plan::NativeIpcInputs> {
    Err("runtime IPC requires the planner-ipc-native feature".into())
}

#[cfg(feature = "planner-ipc-native")]
fn prepared_native_ipc(inputs: &Inputs, output: &Path) -> Result<plan::NativeIpcInputs> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let value = |key: &str| -> Result<String> {
        std::env::var(key).map_err(|_| format!("prepared runtime IPC requires {key}").into())
    };
    let (host, runner_arch, audit_sha, seconds) = match std::env::consts::ARCH {
        "x86_64" => (
            "x86_64-unknown-linux-gnu",
            "X64",
            "384bcef1bafc962019557d7b89ef5dac4c85ee2d1683906c1942458b04118e54",
            600,
        ),
        "aarch64" => (
            "aarch64-unknown-linux-gnu",
            "ARM64",
            "7dd3abad75e472a7241d16143b155dd3ad6e8bb826359bd0afedca1360689efc",
            1200,
        ),
        _ => return Err("runtime IPC requires a qualified native Linux host".into()),
    };
    for (key, expected) in [
        ("PLANNER_NATIVE_IPC_PROFILE", "planner-ipc-native-v1"),
        ("GITHUB_ACTIONS", "true"),
        ("GITHUB_REPOSITORY", "bin9208/openpilot-rust"),
        ("GITHUB_JOB", "planner-runtime"),
        ("RUNNER_OS", "Linux"),
        ("RUNNER_ARCH", runner_arch),
        ("NATIVE_TOOLS_HOST", host),
    ] {
        if value(key)? != expected {
            return Err(format!("prepared runtime IPC {key} differs").into());
        }
    }
    if std::env::consts::OS != "linux" {
        return Err("runtime IPC requires Linux process-group ownership".into());
    }
    if runner_arch == "ARM64" {
        for (key, expected) in [
            ("CI296_NATIVE_ARM", "1"),
            ("RUNNER_ENVIRONMENT", "github-hosted"),
            ("OPENPILOT_CI_EXECUTION_MODE", "native-host"),
            ("OPENPILOT_CI_HOST_ARCH", "aarch64"),
            ("OPENPILOT_CI_TARGET_ARCH", "aarch64"),
        ] {
            if value(key)? != expected {
                return Err(format!("native ARM IPC {key} differs").into());
            }
        }
    }
    let job = PathBuf::from(value("CI_JOB_TEMP")?);
    let name = job.file_name().and_then(|name| name.to_str()).unwrap_or("");
    if job.parent() != Some(Path::new("/tmp"))
        || !name.starts_with("openpilot-ci.")
        || name.len() != "openpilot-ci.".len() + 6
        || !name["openpilot-ci.".len()..]
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric())
    {
        return Err("prepared runtime IPC requires the owned job root".into());
    }
    let uid = rustix::process::geteuid().as_raw();
    let owned_dir = |path: &Path| -> Result<()> {
        let metadata = fs::symlink_metadata(path)?;
        if !metadata.is_dir()
            || metadata.uid() != uid
            || metadata.permissions().mode() & 0o777 != 0o700
            || path.canonicalize()? != path
        {
            return Err(
                "prepared runtime IPC requires canonical owned mode-700 directories".into(),
            );
        }
        Ok(())
    };
    let read = |path: &Path| -> Result<Vec<u8>> {
        let metadata = fs::symlink_metadata(path)?;
        if !metadata.is_file()
            || metadata.uid() != uid
            || metadata.len() > 1024 * 1024
            || path.canonicalize()? != path
        {
            return Err("prepared runtime IPC requires bounded owned regular metadata".into());
        }
        let mut bytes = Vec::new();
        fs::File::open(path)?
            .take(1024 * 1024 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > 1024 * 1024 {
            return Err("prepared runtime IPC metadata exceeded 1 MiB".into());
        }
        Ok(bytes)
    };
    owned_dir(&job)?;
    let owner = format!(
        "{}:{}:planner-runtime",
        value("GITHUB_RUN_ID")?,
        value("GITHUB_RUN_ATTEMPT")?
    );
    if String::from_utf8(read(&job.join("owner"))?)?.trim_end() != owner {
        return Err("prepared runtime IPC job owner differs".into());
    }
    let source = value("GITHUB_SHA")?;
    if source.len() != 40 || !source.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("prepared runtime IPC requires the current source SHA".into());
    }
    for (key, expected) in [
        ("CI296_SOURCE", &inputs.root),
        ("GITHUB_WORKSPACE", &inputs.root),
        ("RUNNER_TEMP", &job),
        ("PLANNER_PARAMS_BINDING", &inputs.binding),
    ] {
        if Path::new(&value(key)?) != expected {
            return Err(format!("prepared runtime IPC {key} path differs").into());
        }
    }
    if std::env::current_dir()? != inputs.root
        || inputs.python != job.join("venv/bin/python")
        || inputs.binaries != inputs.root.join("rust/target/debug")
        || inputs.artifact != job.join("planner-artifact")
        || inputs.source != job.join("planner-source")
        || output != job.join("planner-comparison")
    {
        return Err("prepared runtime IPC caller paths differ".into());
    }
    let corpus = job.join("planner-ipc-original");
    let audit = corpus.join("audit.json");
    if Path::new(&value("PLANNER_NATIVE_IPC_CORPUS")?) != corpus
        || Path::new(&value("PLANNER_NATIVE_IPC_AUDIT")?) != audit
        || value("PLANNER_NATIVE_IPC_AUDIT_SHA256")? != audit_sha
    {
        return Err("prepared runtime IPC stager paths or independent audit SHA differ".into());
    }
    owned_dir(&corpus)?;
    if format!("{:x}", Sha256::digest(read(&audit)?)) != audit_sha
        || read(&corpus.join("owner"))? != owner.as_bytes()
    {
        return Err("prepared runtime IPC corpus owner or audit differs".into());
    }
    let stage = job.join("proof/planner-ipc-stage");
    owned_dir(&stage)?;
    let status: serde_json::Value = serde_json::from_slice(&read(&stage.join("status.json"))?)?;
    if status["status"] != "STAGED_FOR_NATIVE_REVALIDATION"
        || status["owner"] != owner
        || status["source_commit"] != source
        || status["architecture"] != std::env::consts::ARCH
        || status["audit_sha256"] != audit_sha
        || status["root"].as_str() != corpus.to_str()
        || status["original_execution"] != false
        || status["native_revalidation_required"] != true
    {
        return Err("prepared runtime IPC stager source/owner receipt differs".into());
    }
    let proof = job.join("proof/planner-native-controller-523");
    owned_dir(&proof)?;
    if read(&proof.join("controller-source.txt"))? != format!("{source}:{host}\n").as_bytes()
        || read(&proof.join("controller-profile.txt"))?
            != b"ci-receipt-metadata-native,planner-ipc-native\n"
        || read(&proof.join("controller.sha256"))?
            != format!(
                "{}  {}\n",
                crate::seal::digest(&inputs.scheduler)?,
                inputs.scheduler.display()
            )
            .as_bytes()
    {
        return Err("prepared runtime IPC must reuse the initial qualified controller ELF".into());
    }
    let compiler =
        std::env::split_paths(&std::env::var_os("PATH").ok_or("IPC compiler PATH required")?)
            .map(|path| path.join("g++"))
            .find(|path| {
                fs::metadata(path).is_ok_and(|metadata| {
                    metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
                })
            })
            .ok_or("prepared runtime IPC requires the original g++ provider")?
            .canonicalize()?;
    Ok(plan::NativeIpcInputs {
        original_ipc_binding: super::planner_ipc_native::original_ipc_binding(
            &job.join("planner-original/ipc/msgq"),
        )?,
        compiler,
        corpus,
        corpus_audit: audit,
        corpus_audit_sha256: audit_sha.into(),
        timeout: Duration::from_secs(seconds),
        growth_mib: 512,
        peer_build_growth_mib: 128,
    })
}

pub fn cli() -> Result<()> {
    let mut arguments = std::env::args_os().skip(1).peekable();
    if arguments.peek().is_some_and(|arg| arg == "schedule") {
        arguments.next();
        if arguments.next().is_some() {
            return Err(HELP.into());
        }
        let input = std::io::stdin();
        let output = std::io::stdout();
        if let Some(path) = std::env::var_os("PLANNER_NATIVE_SCHEDULER_EVIDENCE") {
            let file = OpenOptions::new().create(true).append(true).open(path)?;
            return scheduler::serve_recorded(input.lock(), output.lock(), file);
        }
        return scheduler::serve(input.lock(), output.lock());
    }
    if arguments.peek().is_some_and(|arg| arg == "ipc") {
        arguments.next();
        #[cfg(not(feature = "planner-ipc-native"))]
        return Err("ipc requires the planner-ipc-native feature".into());
        #[cfg(feature = "planner-ipc-native")]
        {
            if arguments
                .peek()
                .is_some_and(|arg| arg == "--help" || arg == "-h")
            {
                arguments.next();
                if arguments.next().is_some() {
                    return Err(IPC_HELP.into());
                }
                println!("{IPC_HELP}");
                return Ok(());
            }
            let allowed = [
                "--root",
                "--python",
                "--binary",
                "--artifact",
                "--binding",
                "--source-native",
                "--original-ipc-binding",
                "--scheduler",
                "--compiler",
                "--corpus",
                "--corpus-audit",
                "--corpus-audit-sha256",
                "--output",
                "--evidence",
                "--timeout-seconds",
                "--growth-mib",
                "--peer-build-growth-mib",
            ];
            let mut values = BTreeMap::<String, OsString>::new();
            while let Some(key) = arguments.next() {
                let name = key.to_str().ok_or(IPC_HELP)?;
                if !allowed.contains(&name) {
                    return Err(format!("unknown ipc option {name}\n{IPC_HELP}").into());
                }
                let value = arguments
                    .next()
                    .ok_or_else(|| format!("missing value for {name}\n{IPC_HELP}"))?;
                if values.insert(name.to_owned(), value).is_some() {
                    return Err(format!("duplicate ipc option {name}").into());
                }
            }
            let take = |name: &str, values: &mut BTreeMap<String, OsString>| -> Result<String> {
                values
                    .remove(name)
                    .ok_or_else(|| format!("missing {name}\n{IPC_HELP}"))?
                    .into_string()
                    .map_err(|_| format!("ipc {name} requires UTF-8").into())
            };
            let path = |name: &str, values: &mut BTreeMap<String, OsString>| -> Result<PathBuf> {
                let value = take(name, values)?;
                if value.is_empty() || value.contains('\0') || !Path::new(&value).is_absolute() {
                    return Err(format!("ipc {name} requires an absolute UTF-8 path").into());
                }
                // venv와 C++ driver의 경로 철자를 보존한다. symlink canonicalize는 하지 않는다.
                Ok(PathBuf::from(value))
            };
            let positive = |name: &str, values: &mut BTreeMap<String, OsString>| -> Result<u64> {
                let value = take(name, values)?;
                if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
                    return Err(format!("ipc {name} requires a positive decimal integer").into());
                }
                let number = value
                    .parse::<u64>()
                    .map_err(|_| format!("ipc {name} is outside u64 range"))?;
                if number == 0 {
                    return Err(format!("ipc {name} must be positive").into());
                }
                Ok(number)
            };
            let timeout = Duration::from_secs(positive("--timeout-seconds", &mut values)?);
            if std::time::Instant::now().checked_add(timeout).is_none() {
                return Err("ipc timeout exceeds the monotonic clock range".into());
            }
            let growth_bytes = positive("--growth-mib", &mut values)?
                .checked_mul(1024 * 1024)
                .ok_or("ipc growth overflow")?;
            let peer_build_growth_bytes = positive("--peer-build-growth-mib", &mut values)?
                .checked_mul(1024 * 1024)
                .ok_or("ipc peer build growth overflow")?;
            let audit_sha256 = take("--corpus-audit-sha256", &mut values)?;
            if audit_sha256.len() != 64
                || !audit_sha256
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            {
                return Err(
                    "ipc corpus audit SHA must be 64 lowercase hexadecimal characters".into(),
                );
            }
            let options = super::planner_ipc_native::Options {
                root: path("--root", &mut values)?,
                python: path("--python", &mut values)?,
                binary: path("--binary", &mut values)?,
                artifact: path("--artifact", &mut values)?,
                binding: path("--binding", &mut values)?,
                source_native: path("--source-native", &mut values)?,
                original_ipc_binding: path("--original-ipc-binding", &mut values)?,
                scheduler: path("--scheduler", &mut values)?,
                compiler: path("--compiler", &mut values)?,
                corpus: path("--corpus", &mut values)?,
                corpus_audit: path("--corpus-audit", &mut values)?,
                corpus_audit_sha256: audit_sha256,
                output: path("--output", &mut values)?,
                evidence: path("--evidence", &mut values)?,
                timeout,
                growth_bytes,
                peer_build_growth_bytes,
            };
            if !values.is_empty() {
                return Err("unused ipc options".into());
            }
            let receipt = super::planner_ipc_native::run(&options)?;
            println!("{receipt}");
            return Ok(());
        }
    }
    if arguments.peek().is_some_and(|arg| arg == "lifecycle") {
        arguments.next();
        #[cfg(not(feature = "planner-ipc-native"))]
        return Err("lifecycle requires the planner-ipc-native feature".into());
        #[cfg(feature = "planner-ipc-native")]
        {
            if arguments
                .peek()
                .is_some_and(|arg| arg == "--help" || arg == "-h")
            {
                arguments.next();
                if arguments.next().is_some() {
                    return Err(LIFECYCLE_HELP.into());
                }
                println!("{LIFECYCLE_HELP}");
                return Ok(());
            }
            let options = parse_lifecycle_arguments(arguments)?;
            let receipt = super::planner_ipc_native::lifecycle::run(&options)?;
            println!("{receipt}");
            return Ok(());
        }
    }
    let memory = arguments.peek().is_some_and(|arg| arg == "memory");
    if memory {
        arguments.next();
    }
    let mut options = BTreeMap::new();
    let mut growth = BTreeMap::new();
    while let Some(key) = arguments.next() {
        if key == "--help" || key == "-h" {
            println!("{HELP}");
            return Ok(());
        }
        let value = arguments.next().ok_or(HELP)?;
        if key == "--growth-mib" {
            let (name, amount) = value.to_str().ok_or(HELP)?.split_once('=').ok_or(HELP)?;
            let bytes = amount
                .parse::<u64>()?
                .checked_mul(1024 * 1024)
                .ok_or("growth overflow")?;
            if growth.insert(name.to_owned(), bytes).is_some() {
                return Err("duplicate growth operation".into());
            }
        } else {
            let name = key.to_str().ok_or(HELP)?;
            if ![
                "--root",
                "--python",
                "--bin-dir",
                "--artifact",
                "--source-native",
                "--binding",
                "--output",
                "--timeout-seconds",
                "--asan-library",
            ]
            .contains(&name)
                || options.insert(name.to_owned(), value).is_some()
            {
                return Err(HELP.into());
            }
        }
    }
    let asan = options.remove("--asan-library");
    if asan.is_some() && !memory {
        return Err("--asan-library requires memory mode".into());
    }
    let mut take = |name: &str| -> Result<OsString> {
        options
            .remove(name)
            .ok_or_else(|| format!("missing {name}\n{HELP}").into())
    };
    let root = PathBuf::from(take("--root")?).canonicalize()?;
    let python = PathBuf::from(take("--python")?);
    // sys.executable처럼 가상환경 경로 철자를 보존한다. symlink canonicalize는 하지 않는다.
    let absolute = |path: OsString| -> Result<PathBuf> { Ok(std::env::current_dir()?.join(path)) };
    let inputs = Inputs {
        root,
        python: absolute(python.into_os_string())?,
        binaries: absolute(take("--bin-dir")?)?,
        artifact: absolute(take("--artifact")?)?,
        source: absolute(take("--source-native")?)?,
        binding: absolute(take("--binding")?)?,
        scheduler: std::env::current_exe()?,
        native_ipc: None,
        native_lifecycle: None,
    };
    let output = absolute(take("--output")?)?;
    let timeout = take("--timeout-seconds")?
        .to_str()
        .ok_or(HELP)?
        .parse::<u64>()?;
    let mut runner = NativeRunner;
    if let Some(asan) = asan {
        let receipt = memory::run(
            &inputs,
            &PathBuf::from(asan),
            &output,
            &growth,
            Duration::from_secs(timeout),
            &mut runner,
        )?;
        println!("{receipt}");
        return Ok(());
    }
    let mut checks = Vec::new();
    let checked = if memory {
        checks = plan::memory_checks(&inputs, &growth, Duration::from_secs(timeout))?;
        run_checks(&checks, &output, &inputs.root, &mut runner)
    } else {
        // 전체 growth12 검증과 기존 부재 guard는 첫 leaf 전에 수행한다.
        let steps = plan::runtime_plan(&inputs, &growth, Duration::from_secs(timeout))?;
        require_retired_controller_absent(&inputs.root)?;
        let mut prepared_ipc = None;
        let prepared = steps.into_iter().map(|step| {
            let check = if matches!(&step, plan::PlannedCheck::NativeIpc { .. }) {
                // ABI42 등 앞선 실패 뒤에는 이 준비·feature/profile 검사에 도달하지 않는다.
                let mut ipc_inputs = inputs.clone();
                let context = prepared_native_ipc(&inputs, &output)?;
                prepared_ipc = Some(context.clone());
                ipc_inputs.native_ipc = Some(context);
                step.complete(&ipc_inputs)?
            } else if let plan::PlannedCheck::NativeLifecycle { growth_bytes, .. } = &step {
                let mut lifecycle_inputs = inputs.clone();
                lifecycle_inputs.native_lifecycle = Some(prepared_native_lifecycle(
                    &inputs,
                    &output,
                    prepared_ipc
                        .as_ref()
                        .ok_or("native lifecycle requires the preceding prepared IPC")?,
                    *growth_bytes,
                )?);
                step.complete(&lifecycle_inputs)?
            } else {
                step.complete(&inputs)?
            };
            checks.push(check.clone());
            Ok(check)
        });
        run_prepared_checks(prepared, &output, &inputs.root, &mut runner)
    };
    // 첫 leaf 실패 뒤에도 복원 여부를 확인한다. commands.json의 실제 실패 행은 보존한다.
    if !memory {
        require_retired_controller_absent(&inputs.root)?;
    }
    checked?;
    if !memory {
        let receipt = plan::write_receipt(&inputs, &checks, &output, &mut runner)?;
        println!("{receipt}");
    }
    // --asan-library 없는 memory core는 원본 Python wrapper의 독립 비교 경로로 보존한다.
    Ok(())
}
