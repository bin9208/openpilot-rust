//! Build the model logging link-fault peer without replacing the checkout's
//! production modeld. The owned target is rebuilt plain after the fault link.
use crate::{
    qa_process::{CaptureError, Captured, CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    time::Duration,
};

const MAX_BINARY: u64 = 512_000_000;
const MAX_SOURCE: u64 = 1_000_000;
const BUILD_GROWTH: u64 = 12 * 1024 * 1024 * 1024;
const SHIM_GROWTH: u64 = 128 * 1024 * 1024;

pub struct Options {
    pub root: PathBuf,
    pub output: PathBuf,
    pub cargo: PathBuf,
    pub compiler: PathBuf,
}

fn require(ok: bool, message: &str) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(message.into())
    }
}

fn digest(path: &Path, limit: u64) -> Result<String> {
    let descriptor = rustix::fs::open(
        path,
        rustix::fs::OFlags::RDONLY
            | rustix::fs::OFlags::NONBLOCK
            | rustix::fs::OFlags::NOFOLLOW
            | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )?;
    let mut file = File::from(descriptor);
    let metadata = file.metadata()?;
    let path_metadata = fs::symlink_metadata(path)?;
    require(
        metadata.is_file()
            && path_metadata.is_file()
            && (metadata.dev(), metadata.ino(), metadata.len())
                == (
                    path_metadata.dev(),
                    path_metadata.ino(),
                    path_metadata.len(),
                )
            && metadata.len() <= limit,
        "invalid fault build input",
    )?;
    let mut hash = Sha256::new();
    let mut count = 0_u64;
    let mut chunk = [0_u8; 64 * 1024];
    while count <= limit {
        let length = chunk.len().min((limit + 1 - count) as usize);
        let size = file.read(&mut chunk[..length])?;
        if size == 0 {
            break;
        }
        hash.update(&chunk[..size]);
        count += size as u64;
    }
    require(
        count == metadata.len()
            && fs::symlink_metadata(path).is_ok_and(|end| {
                (end.dev(), end.ino(), end.len())
                    == (metadata.dev(), metadata.ino(), metadata.len())
            }),
        "fault build input changed while reading",
    )?;
    Ok(format!("{:x}", hash.finalize()))
}

fn include_directory(target: &Path) -> Result<PathBuf> {
    let mut matches = Vec::new();
    for entry in fs::read_dir(target.join("debug/build"))? {
        let entry = entry?;
        if entry.file_name().to_string_lossy().starts_with("zmq-sys-") {
            let include = entry.path().join("out/source/include");
            if include.join("zmq.h").is_file() {
                matches.push(include);
            }
        }
    }
    require(
        matches.len() == 1,
        "fault build requires one owned zmq-sys header tree",
    )?;
    Ok(matches.remove(0))
}

fn cargo_spec(options: &Options, target: &Path, fault: Option<&Path>) -> CommandSpec {
    let mut argv = vec![
        options.cargo.as_os_str().to_owned(),
        if fault.is_some() { "rustc" } else { "build" }.into(),
        "--manifest-path".into(),
        options.root.join("rust/Cargo.toml").into_os_string(),
        "-p".into(),
        "openpilot-driving-modeld".into(),
        "--bin".into(),
        "openpilot-driving-modeld".into(),
        "--locked".into(),
        "--target-dir".into(),
        target.as_os_str().to_owned(),
    ];
    if let Some(shim) = fault {
        let mut link = OsString::from("link-arg=");
        link.push(shim.as_os_str());
        argv.extend([
            "--".into(),
            "-C".into(),
            link,
            "-C".into(),
            "link-arg=-Wl,--wrap=zmq_msg_send".into(),
        ]);
    }
    let mut spec = CommandSpec::new_os(argv, Duration::from_secs(30 * 60));
    spec.stdout_limit = 8 * 1024 * 1024;
    spec.stderr_limit = 8 * 1024 * 1024;
    spec.cwd = Some(options.root.clone());
    spec.set_os_env(BTreeMap::from([(
        OsString::from("CARGO_INCREMENTAL"),
        OsString::from("0"),
    )]));
    spec
}

fn clean_spec(options: &Options, target: &Path) -> CommandSpec {
    let mut spec = CommandSpec::new_os(
        vec![
            options.cargo.as_os_str().to_owned(),
            "clean".into(),
            "--manifest-path".into(),
            options.root.join("rust/Cargo.toml").into_os_string(),
            "-p".into(),
            "openpilot-driving-modeld".into(),
            "--target-dir".into(),
            target.as_os_str().to_owned(),
        ],
        Duration::from_secs(5 * 60),
    );
    spec.stdout_limit = 8 * 1024 * 1024;
    spec.stderr_limit = 8 * 1024 * 1024;
    spec.cwd = Some(options.root.clone());
    spec
}

fn compile_spec(options: &Options, include: &Path, source: &Path, shim: &Path) -> CommandSpec {
    let mut include_arg = OsString::from("-I");
    include_arg.push(include.as_os_str());
    let mut spec = CommandSpec::new_os(
        vec![
            options.compiler.as_os_str().to_owned(),
            "-std=c++17".into(),
            "-O2".into(),
            include_arg,
            "-c".into(),
            source.as_os_str().to_owned(),
            "-o".into(),
            shim.as_os_str().to_owned(),
        ],
        Duration::from_secs(2 * 60),
    );
    spec.stdout_limit = 8 * 1024 * 1024;
    spec.stderr_limit = 8 * 1024 * 1024;
    spec.cwd = Some(options.root.clone());
    spec
}

fn step(runner: &mut impl Runner, spec: &CommandSpec, output: &Path, name: &str) -> Result<()> {
    let result = runner.run(spec);
    let (stdout, stderr) = match &result {
        Ok(Captured { stdout, stderr, .. }) => (stdout.as_slice(), stderr.as_slice()),
        Err(error) => match error.downcast_ref::<CaptureError>() {
            Some(CaptureError::Timeout { stdout, stderr, .. }) => {
                (stdout.as_slice(), stderr.as_slice())
            }
            _ => (&[][..], &[][..]),
        },
    };
    fs::write(output.join(format!("{name}.stdout")), stdout)?;
    fs::write(output.join(format!("{name}.stderr")), stderr)?;
    if let Err(error) = &result {
        if !matches!(
            error.downcast_ref::<CaptureError>(),
            Some(CaptureError::Timeout { .. })
        ) {
            fs::write(output.join(format!("{name}.error")), format!("{error}\n"))?;
        }
    }
    let Captured { code, .. } = result?;
    require(code == 0, &format!("fault build {name} exited {code}"))
}

fn copy_fault(source: &Path, destination: &Path) -> Result<()> {
    let before = digest(source, MAX_BINARY)?;
    let descriptor = rustix::fs::open(
        source,
        rustix::fs::OFlags::RDONLY
            | rustix::fs::OFlags::NONBLOCK
            | rustix::fs::OFlags::NOFOLLOW
            | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )?;
    let mut from = File::from(descriptor);
    let metadata = from.metadata()?;
    let path_metadata = fs::symlink_metadata(source)?;
    require(
        metadata.is_file()
            && path_metadata.is_file()
            && (metadata.dev(), metadata.ino(), metadata.len())
                == (
                    path_metadata.dev(),
                    path_metadata.ino(),
                    path_metadata.len(),
                )
            && metadata.len() <= MAX_BINARY,
        "invalid fault-linked binary",
    )?;
    let mut to = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)?;
    let bytes = std::io::copy(&mut Read::by_ref(&mut from).take(MAX_BINARY + 1), &mut to)?;
    require(
        bytes == metadata.len(),
        "fault-linked binary changed while copying",
    )?;
    to.set_permissions(fs::Permissions::from_mode(0o755))?;
    to.sync_all()?;
    require(
        digest(source, MAX_BINARY)? == before && digest(destination, MAX_BINARY)? == before,
        "fault-linked binary changed during copy",
    )?;
    Ok(())
}

pub fn run_with(runner: &mut impl Runner, options: &Options) -> Result<Value> {
    run_with_space(runner, options, |destination, growth| {
        let space = crate::disk_space::inspect(destination, growth)?;
        println!("{}", serde_json::to_string(&space)?);
        space.require()
    })
}

fn run_with_space(
    runner: &mut impl Runner,
    options: &Options,
    mut space: impl FnMut(&Path, u64) -> Result<()>,
) -> Result<Value> {
    require(
        options.root.is_absolute()
            && options.output.is_absolute()
            && options.cargo.is_absolute()
            && options.compiler.is_absolute(),
        "fault build paths must be absolute",
    )?;
    fs::create_dir(&options.output)?;
    let source = options.root.join("rust/tools/model_logging_fault.cc");
    let source_sha = digest(&source, MAX_SOURCE)?;
    let manifest = options.root.join("rust/Cargo.toml");
    let lock = options.root.join("rust/Cargo.lock");
    let manifest_sha = digest(&manifest, MAX_SOURCE)?;
    let lock_sha = digest(&lock, MAX_SOURCE)?;
    let target = options.output.join("target");
    let shim = options.output.join("model_logging_fault.o");
    let plain = target.join("debug/openpilot-driving-modeld");
    space(&target, BUILD_GROWTH)?;
    step(
        runner,
        &cargo_spec(options, &target, None),
        &options.output,
        "plain-before",
    )?;
    let plain_before = digest(&plain, MAX_BINARY)?;
    let include = include_directory(&target)?;
    let header = include.join("zmq.h");
    let header_sha = digest(&header, MAX_SOURCE)?;
    space(&shim, SHIM_GROWTH)?;
    step(
        runner,
        &compile_spec(options, &include, &source, &shim),
        &options.output,
        "shim",
    )?;
    let shim_sha = digest(&shim, MAX_BINARY)?;
    // Force a fresh plain package link after using altered rustc link arguments.
    // Always attempt to clean the private package, even when fault linking failed.
    // Reserve the prospective fault relink *and* a subsequent plain rebuild.
    // This cannot guarantee disk against concurrent consumers, so cleanup
    // below still runs even if a later reservation or copy fails.
    let fault_result = space(&target, BUILD_GROWTH * 2).and_then(|()| {
        step(
            runner,
            &cargo_spec(options, &target, Some(&shim)),
            &options.output,
            "fault-link",
        )
    });
    let fault = options.output.join("openpilot-driving-modeld-fault");
    let fault_result = fault_result.and_then(|()| {
        space(&fault, MAX_BINARY + BUILD_GROWTH)?;
        copy_fault(&plain, &fault)
    });
    let restoration = step(
        runner,
        &clean_spec(options, &target),
        &options.output,
        "plain-clean",
    )
    .and_then(|()| {
        space(&target, BUILD_GROWTH)?;
        step(
            runner,
            &cargo_spec(options, &target, None),
            &options.output,
            "plain-after",
        )
    });
    restoration?;
    fault_result?;
    require(
        digest(&plain, MAX_BINARY)? == plain_before,
        "plain modeld was not restored",
    )?;
    let fault_sha = digest(&fault, MAX_BINARY)?;
    require(
        fault_sha != plain_before,
        "fault link did not change modeld",
    )?;
    require(
        digest(&source, MAX_SOURCE)? == source_sha
            && digest(&manifest, MAX_SOURCE)? == manifest_sha
            && digest(&lock, MAX_SOURCE)? == lock_sha
            && digest(&header, MAX_SOURCE)? == header_sha,
        "fault source changed during build",
    )?;
    let report = json!({"result":"pass", "scope":"private-target link wrapper and plain restoration",
        "source_sha256":source_sha, "shim_sha256":shim_sha, "fault_sha256":fault_sha,
        "plain_sha256":plain_before, "fault_binary":fault, "cargo_manifest_sha256":manifest_sha,
        "cargo_lock_sha256":lock_sha, "zmq_header_sha256":header_sha});
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(options.output.join("fault-build.json"))?;
    file.write_all(format!("{}\n", serde_json::to_string_pretty(&report)?).as_bytes())?;
    file.sync_all()?;
    Ok(report)
}

pub fn run(options: &Options) -> Result<Value> {
    run_with(&mut NativeRunner, options)
}

pub fn cli() -> Result<()> {
    let mut root = None;
    let mut output = None;
    let mut cargo = None;
    let mut compiler = None;
    let mut args = std::env::args_os().skip(1);
    while let Some(key) = args.next() {
        let value = args.next().ok_or("missing fault build option value")?;
        let slot = match key.to_str() {
            Some("--root") => &mut root,
            Some("--output") => &mut output,
            Some("--cargo") => &mut cargo,
            Some("--compiler") => &mut compiler,
            _ => return Err("unknown fault build option".into()),
        };
        let _ = slot.replace(PathBuf::from(value));
    }
    let options = Options {
        root: root.ok_or("missing --root")?,
        output: output.ok_or("missing --output")?,
        cargo: cargo.ok_or("missing --cargo")?,
        compiler: compiler.ok_or("missing --compiler")?,
    };
    println!("{}", serde_json::to_string(&run(&options)?)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{digest, run_with_space, Options};
    use crate::{
        qa_process::{Captured, CommandSpec, Runner},
        seal::Result,
    };
    use std::{ffi::OsStr, fs, os::unix::fs::symlink, path::Path};

    #[test]
    fn final_digest_is_standard_sha256_and_rejects_source_alias() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        fs::write(&source, b"abc").unwrap();
        assert_eq!(
            digest(&source, 3).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let alias = root.path().join("alias");
        symlink(&source, &alias).unwrap();
        assert!(digest(&alias, 3).is_err());
    }

    struct BuildPeer {
        steps: Vec<String>,
        fault_fails: bool,
        restoration_stale: bool,
        source_drift: bool,
    }
    impl Runner for BuildPeer {
        fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
            assert_eq!(spec.stdout_limit, 8 * 1024 * 1024);
            assert_eq!(spec.stderr_limit, 8 * 1024 * 1024);
            let argv = spec.effective_argv();
            let target_index = argv
                .iter()
                .position(|value| value.as_os_str() == OsStr::new("--target-dir"));
            let command = if target_index.is_some() {
                argv[1].to_string_lossy().into_owned()
            } else {
                "c++".into()
            };
            let restoring = self.steps.iter().any(|step| step == "clean");
            self.steps.push(command.clone());
            if let Some(index) = target_index {
                let target = Path::new(argv[index + 1].as_os_str());
                let binary = target.join("debug/openpilot-driving-modeld");
                fs::create_dir_all(binary.parent().unwrap())?;
                match command.as_str() {
                    "build" if !restoring => {
                        fs::write(&binary, b"plain")?;
                        let include = target.join("debug/build/zmq-sys-test/out/source/include");
                        fs::create_dir_all(&include)?;
                        fs::write(include.join("zmq.h"), b"header")?;
                    }
                    "rustc" if !self.fault_fails => fs::write(&binary, b"fault")?,
                    "clean" => {
                        let _ = fs::remove_file(&binary);
                    }
                    "build" => fs::write(
                        &binary,
                        if self.restoration_stale {
                            b"fault"
                        } else {
                            b"plain"
                        },
                    )?,
                    _ => {}
                }
                if command == "build" && restoring && self.source_drift {
                    fs::write(
                        spec.cwd
                            .as_ref()
                            .unwrap()
                            .join("rust/tools/model_logging_fault.cc"),
                        b"changed wrapper",
                    )?;
                }
            } else {
                let shim = argv.last().unwrap();
                fs::write(Path::new(shim.as_os_str()), b"shim")?;
            }
            Ok(Captured {
                code: i32::from(command == "rustc" && self.fault_fails),
                stdout: b"build stdout".to_vec(),
                stderr: b"build stderr".to_vec(),
            })
        }
    }

    fn options(root: &Path) -> Options {
        let source = root.join("rust/tools");
        fs::create_dir_all(&source).unwrap();
        let production = root.join("rust/target/debug");
        fs::create_dir_all(&production).unwrap();
        fs::write(production.join("openpilot-driving-modeld"), b"production").unwrap();
        fs::write(source.join("model_logging_fault.cc"), b"unchanged wrapper").unwrap();
        fs::write(root.join("rust/Cargo.toml"), b"manifest").unwrap();
        fs::write(root.join("rust/Cargo.lock"), b"lock").unwrap();
        Options {
            root: root.into(),
            output: root.join("output"),
            cargo: root.join("cargo"),
            compiler: root.join("c++"),
        }
    }

    #[test]
    fn private_build_restores_plain_target_and_keeps_fault_binary() {
        let root = tempfile::tempdir().unwrap();
        let options = options(root.path());
        let mut peer = BuildPeer {
            steps: Vec::new(),
            fault_fails: false,
            restoration_stale: false,
            source_drift: false,
        };
        let report = run_with_space(&mut peer, &options, |_, _| Ok(())).unwrap();
        assert_eq!(
            peer.steps.iter().map(String::as_str).collect::<Vec<_>>(),
            vec!["build", "c++", "rustc", "clean", "build"]
        );
        assert_eq!(report["result"], "pass");
        assert_eq!(
            fs::read(options.output.join("target/debug/openpilot-driving-modeld")).unwrap(),
            b"plain"
        );
        assert_eq!(
            fs::read(options.output.join("openpilot-driving-modeld-fault")).unwrap(),
            b"fault"
        );
        assert!(options.output.join("fault-build.json").is_file());
        assert_eq!(
            fs::read(
                options
                    .root
                    .join("rust/target/debug/openpilot-driving-modeld")
            )
            .unwrap(),
            b"production"
        );
        assert_eq!(
            fs::read(options.output.join("fault-link.stderr")).unwrap(),
            b"build stderr"
        );
    }

    #[test]
    fn failed_fault_link_still_restores_and_cannot_publish_receipt() {
        let root = tempfile::tempdir().unwrap();
        let options = options(root.path());
        let mut peer = BuildPeer {
            steps: Vec::new(),
            fault_fails: true,
            restoration_stale: false,
            source_drift: false,
        };
        let error = run_with_space(&mut peer, &options, |_, _| Ok(()))
            .unwrap_err()
            .to_string();
        assert!(error.contains("fault build fault-link exited 1"));
        assert_eq!(
            peer.steps.iter().map(String::as_str).collect::<Vec<_>>(),
            vec!["build", "c++", "rustc", "clean", "build"]
        );
        assert_eq!(
            fs::read(options.output.join("target/debug/openpilot-driving-modeld")).unwrap(),
            b"plain"
        );
        assert!(!options.output.join("fault-build.json").exists());
        assert_eq!(
            fs::read(
                options
                    .root
                    .join("rust/target/debug/openpilot-driving-modeld")
            )
            .unwrap(),
            b"production"
        );
    }

    #[test]
    fn stale_plain_rebuild_cannot_claim_restoration() {
        let root = tempfile::tempdir().unwrap();
        let options = options(root.path());
        let mut peer = BuildPeer {
            steps: Vec::new(),
            fault_fails: false,
            restoration_stale: true,
            source_drift: false,
        };
        let error = run_with_space(&mut peer, &options, |_, _| Ok(()))
            .unwrap_err()
            .to_string();
        assert!(error.contains("plain modeld was not restored"));
        assert!(!options.output.join("fault-build.json").exists());
        assert_eq!(
            fs::read(options.output.join("openpilot-driving-modeld-fault")).unwrap(),
            b"fault"
        );
    }

    #[test]
    fn changed_wrapper_cannot_publish_build_receipt() {
        let root = tempfile::tempdir().unwrap();
        let options = options(root.path());
        let mut peer = BuildPeer {
            steps: Vec::new(),
            fault_fails: false,
            restoration_stale: false,
            source_drift: true,
        };
        let error = run_with_space(&mut peer, &options, |_, _| Ok(()))
            .unwrap_err()
            .to_string();
        assert!(error.contains("fault source changed during build"));
        assert_eq!(peer.steps.len(), 5);
        assert!(!options.output.join("fault-build.json").exists());
    }

    #[test]
    fn copy_reservation_failure_still_restores_private_plain_binary() {
        let root = tempfile::tempdir().unwrap();
        let options = options(root.path());
        let mut peer = BuildPeer {
            steps: Vec::new(),
            fault_fails: false,
            restoration_stale: false,
            source_drift: false,
        };
        let mut checks = 0;
        let error = run_with_space(&mut peer, &options, |_, _| {
            checks += 1;
            if checks == 4 {
                Err("copy capacity vanished".into())
            } else {
                Ok(())
            }
        })
        .unwrap_err()
        .to_string();
        assert!(error.contains("copy capacity vanished"));
        assert_eq!(checks, 5, "restoration reserve was not checked");
        assert_eq!(
            peer.steps.iter().map(String::as_str).collect::<Vec<_>>(),
            vec!["build", "c++", "rustc", "clean", "build"]
        );
        assert_eq!(
            fs::read(options.output.join("target/debug/openpilot-driving-modeld")).unwrap(),
            b"plain"
        );
        assert_eq!(
            fs::read(
                options
                    .root
                    .join("rust/target/debug/openpilot-driving-modeld")
            )
            .unwrap(),
            b"production"
        );
        assert!(!options.output.join("fault-build.json").exists());
    }

    #[test]
    fn restoration_reservation_failure_still_runs_package_clean() {
        let root = tempfile::tempdir().unwrap();
        let options = options(root.path());
        let mut peer = BuildPeer {
            steps: Vec::new(),
            fault_fails: false,
            restoration_stale: false,
            source_drift: false,
        };
        let mut checks = 0;
        let error = run_with_space(&mut peer, &options, |_, _| {
            checks += 1;
            if checks == 5 {
                Err("restore capacity vanished".into())
            } else {
                Ok(())
            }
        })
        .unwrap_err()
        .to_string();
        assert!(error.contains("restore capacity vanished"));
        assert_eq!(
            peer.steps.iter().map(String::as_str).collect::<Vec<_>>(),
            vec!["build", "c++", "rustc", "clean"]
        );
        assert!(!options
            .output
            .join("target/debug/openpilot-driving-modeld")
            .exists());
        assert_eq!(
            fs::read(
                options
                    .root
                    .join("rust/target/debug/openpilot-driving-modeld")
            )
            .unwrap(),
            b"production"
        );
        assert!(!options.output.join("fault-build.json").exists());
    }
}
