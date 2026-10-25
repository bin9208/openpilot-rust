use super::{
    memory,
    plan::{self, Inputs},
    run_checks, scheduler,
};
use crate::{qa_process::NativeRunner, seal::Result};
use std::{collections::BTreeMap, ffi::OsString, fs::OpenOptions, path::PathBuf, time::Duration};

const HELP: &str = "check-plannerd-ci schedule\ncheck-plannerd-ci [memory [--asan-library PATH]] --root PATH --python PATH --bin-dir PATH --artifact PATH --source-native PATH --binding PATH --output PATH --timeout-seconds N --growth-mib NAME=MIB (all 12 operations, or abi/owner/fault in memory mode; --asan-library prepares sanitizer wrappers and writes the memory receipt)";

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
    run_checks(&checks, &output, &inputs.root, &mut runner)?;
    if !memory {
        let receipt = plan::write_receipt(&inputs, &checks, &output, &mut runner)?;
        println!("{receipt}");
    }
    // --asan-library 없는 memory core는 원본 Python wrapper의 독립 비교 경로로 보존한다.
    Ok(())
}
