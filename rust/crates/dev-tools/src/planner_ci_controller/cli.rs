use super::{
    memory,
    plan::{self, Inputs},
    run_checks, scheduler,
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
    let checks = if memory {
        plan::memory_checks(&inputs, &growth, Duration::from_secs(timeout))?
    } else {
        plan::runtime_checks(&inputs, &growth, Duration::from_secs(timeout))?
    };
    if !memory {
        require_retired_controller_absent(&inputs.root)?;
    }
    let checked = run_checks(&checks, &output, &inputs.root, &mut runner);
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
