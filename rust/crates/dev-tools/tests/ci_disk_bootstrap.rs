//! Native counterparts of all thirteen test_native_disk_guard.py cases.
use openpilot_dev_tools::{
    ci_policy::parse,
    qa_process::{CommandSpec, NativeRunner, Runner},
};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::{symlink, PermissionsExt},
    path::{Path, PathBuf},
    time::Duration,
};
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}
fn bootstrap() -> PathBuf {
    root().join("rust/tools/check-disk-space")
}
fn executable(path: &Path, body: &str) {
    fs::write(path, format!("#!/bin/bash\n{body}")).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}
struct Fixture {
    _temp: tempfile::TempDir,
    path: PathBuf,
    env: BTreeMap<String, String>,
    log: PathBuf,
}
impl Fixture {
    fn new(available: &str) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().to_owned();
        let commands = path.join("commands");
        fs::create_dir(&commands).unwrap();
        let log = path.join("commands.log");
        executable(&commands.join("df"),&format!("set -eu\ncase \"$*\" in *'operation space'*) printf 'Avail\\n%s\\n' \"${{GUARD_OPERATION_FREE:-{available}}}\";; *) printf 'Avail\\n{available}\\n';; esac\n"));
        executable(
            &commands.join("rustup"),
            "set -eu\nprintf 'rustup %s\\n' \"$*\" >> \"$GUARD_COMMAND_LOG\"\n",
        );
        executable(
            &commands.join("cargo"),
            r#"set -eu
test -z "${RUSTFLAGS+x}${CARGO_BUILD_TARGET+x}"
printf 'cargo %s\n' "$*" >> "$GUARD_COMMAND_LOG"
test -z "${GUARD_BUILD_FAIL:-}" || exit 17
sleep 0.1
target=''
host=''
while [ "$#" -gt 0 ]; do
 case "$1" in
 --target-dir) target=$2; shift ;;
 --target) host=$2; shift ;;
 esac
 shift
done
mkdir -p "$target/$host/debug"
printf '#!/bin/bash\nprintf "native %%s\\n" "$*" >> "$GUARD_COMMAND_LOG"\n' > "$target/$host/debug/disk-space-guard"
chmod +x "$target/$host/debug/disk-space-guard"
"#,
        );
        let runner = path.join("runner");
        fs::create_dir(&runner).unwrap();
        fs::set_permissions(&runner, fs::Permissions::from_mode(0o700)).unwrap();
        let env = BTreeMap::from([
            (
                "PATH".into(),
                format!("{}:{}", commands.display(), std::env::var("PATH").unwrap()),
            ),
            ("RUNNER_TEMP".into(), runner.display().to_string()),
            ("GUARD_COMMAND_LOG".into(), log.display().to_string()),
            (
                "CARGO_TARGET_DIR".into(),
                path.join("unrelated-target").display().to_string(),
            ),
            ("RUSTFLAGS".into(), "-Zsanitizer=address".into()),
            (
                "CARGO_BUILD_TARGET".into(),
                "aarch64-unknown-linux-gnu".into(),
            ),
        ]);
        Self {
            _temp: temp,
            path,
            env,
            log,
        }
    }
    fn invoke(&self) -> openpilot_dev_tools::qa_process::Captured {
        let script = self
            .env
            .get("GUARD_BOOTSTRAP")
            .cloned()
            .unwrap_or_else(|| bootstrap().display().to_string());
        self.invoke_args(
            &script,
            &[
                "--path",
                self.env
                    .get("GUARD_OPERATION_PATH")
                    .map(String::as_str)
                    .unwrap_or("/path with spaces/new"),
                "--growth-mib",
                "128",
            ],
        )
    }
    fn invoke_args(
        &self,
        script: &str,
        args: &[&str],
    ) -> openpilot_dev_tools::qa_process::Captured {
        let mut spec = CommandSpec::new(
            std::iter::once("bash".into())
                .chain(std::iter::once(script.into()))
                .chain(args.iter().map(|v| (*v).into()))
                .collect(),
            Duration::from_secs(3),
        );
        spec.env = self.env.clone();
        if !self.env.contains_key("RUNNER_TEMP") {
            spec.argv = std::iter::once("env".into())
                .chain(["-u".into(), "RUNNER_TEMP".into()])
                .chain(spec.argv)
                .collect();
        }
        NativeRunner.run(&spec).unwrap()
    }
    fn lines(&self) -> Vec<String> {
        fs::read_to_string(&self.log)
            .unwrap()
            .lines()
            .map(str::to_owned)
            .collect()
    }
    fn cache(&self) -> PathBuf {
        PathBuf::from(&self.env["RUNNER_TEMP"]).join("openpilot-disk-guard")
    }
    fn binary(&self) -> PathBuf {
        fs::read_dir(self.cache())
            .unwrap()
            .filter_map(Result::ok)
            .map(|v| v.path().join("disk-space-guard"))
            .find(|p| p.is_file())
            .unwrap()
    }
}
fn success(output: &openpilot_dev_tools::qa_process::Captured) {
    assert_eq!(
        output.code,
        0,
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
#[test]
fn capacity_failure_precedes_rustup_and_cargo() {
    let f = Fixture::new(&(25 * 1024u64.pow(3) + 1024u64.pow(3) - 1).to_string());
    let r = f.invoke();
    assert_ne!(r.code, 0);
    assert!(String::from_utf8_lossy(&r.stderr).contains("recover at least 35 GiB"));
    assert!(!f.log.exists());
}
#[test]
fn cold_bootstrap_builds_only_host_guard_and_survives_target_cleanup() {
    let mut f = Fixture::new("999999999999");
    success(&f.invoke());
    let first = f.lines();
    assert!(first[0].starts_with("rustup toolchain install 1.94.0"));
    for needle in ["cargo +1.94.0 build", "-p openpilot-disk-space", "--locked"] {
        assert!(first[1].contains(needle));
    }
    assert_eq!(
        first[2],
        "native --path /path with spaces/new --growth-mib 128"
    );
    let _ = fs::remove_dir_all(&f.env["CARGO_TARGET_DIR"]);
    f.env
        .get_mut("CARGO_TARGET_DIR")
        .unwrap()
        .push_str("-changed");
    success(&f.invoke());
    let mut expected = first.clone();
    expected.push(first[2].clone());
    assert_eq!(f.lines(), expected);
}
fn hash(lines: &[String]) -> String {
    format!("{:x}", Sha256::digest(lines.join("\n").as_bytes()))
}
#[test]
fn workflow_guards_use_absolute_native_bootstrap_and_keep_all_growths() {
    let text = fs::read_to_string(root().join(".github/workflows/rust.yml")).unwrap();
    assert!(!text.contains("card_ci_space.py"));
    let marker = "\"$GITHUB_WORKSPACE/rust/tools/check-disk-space\"";
    assert_eq!(text.matches(marker).count(), 198);
    let workflow = parse(&text).unwrap();
    let jobs = workflow["jobs"].as_object().unwrap();
    let scripts = |job: &serde_json::Value| {
        job["steps"]
            .as_array()
            .map(|steps| {
                steps
                    .iter()
                    .map(|s| s["run"].as_str().unwrap_or("").to_owned())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    };
    assert_eq!(
        jobs.values()
            .filter(|job| scripts(job).iter().any(|s| s.contains("check-disk-space")))
            .count(),
        26
    );
    assert_eq!(
        jobs.values()
            .filter(|job| scripts(job).iter().any(|s| {
                let lines = s.lines().collect::<Vec<_>>();
                lines.iter().enumerate().any(|(i, line)| {
                    line.contains("check-disk-space")
                        && (i + 1 == lines.len()
                            || !lines[i + 1].contains("--bin stage-startup-ui-raylib"))
                })
            }))
            .count(),
        25
    );
    let lines = text.lines().collect::<Vec<_>>();
    let mut arguments = vec![];
    let mut raylib = vec![];
    let mut logging_builder = vec![];
    let mut native_architecture = vec![];
    let mut encoder_builder = vec![];
    let mut model_source = vec![];
    let mut translation_checker = vec![];
    let mut diagnostic_checkers = vec![];
    let mut artifact_list = vec![];
    let mut miri_hashes = vec![];
    let mut message_state = vec![];
    let mut egl_reference = vec![];
    let mut command_checkers = vec![];
    let mut params_reference = vec![];
    let mut proc_stat_probe = vec![];
    let mut llvm_provider = vec![];
    for (i, line) in lines.iter().enumerate() {
        if let Some((_, tail)) = line.split_once(marker) {
            if i + 1 < lines.len() && lines[i + 1].contains("--bin stage-startup-ui-raylib") {
                raylib.push(tail.to_owned());
            } else if i + 1 < lines.len()
                && lines[i + 1].contains("--bin build-native-logging-reference")
            {
                logging_builder.push(tail.to_owned());
            } else if i + 1 < lines.len() && lines[i + 1].contains("--test native_build_closure") {
                native_architecture.push(tail.to_owned());
            } else if i + 1 < lines.len() && lines[i + 1].contains("--bin build-encoder-source") {
                encoder_builder.push(tail.to_owned());
            } else if i + 1 < lines.len() && lines[i + 1].contains("git lfs pull --include=openpilot/selfdrive/modeld/models/driving_supercombo.onnx") {
                model_source.push(tail.to_owned());
            } else if i + 1 < lines.len() && lines[i + 1].contains("--bin check-ui-translations") {
                translation_checker.push(tail.to_owned());
            } else if i + 1 < lines.len() && lines[i + 1].contains("--bin check-runtime-diagnostics") {
                diagnostic_checkers.push(tail.to_owned());
            } else if i + 1 < lines.len() && lines[i + 1].contains("--bin list-cargo-tests") {
                artifact_list.push(tail.to_owned());
            } else if i + 1 < lines.len() && lines[i + 1].contains("--bin hash-miri-core") {
                miri_hashes.push(tail.to_owned());
            } else if i + 1 < lines.len() && lines[i + 1].contains("--bin check-message-state") {
                message_state.push(tail.to_owned());
            } else if i + 1 < lines.len()
                && lines[i + 1].contains("--features egl-reference --test ui_egl_reference")
            {
                egl_reference.push(tail.to_owned());
            } else if i + 1 < lines.len() && lines[i + 1].contains("--bin check-command-reference") {
                command_checkers.push(tail.to_owned());
            } else if i + 1 < lines.len() && lines[i + 1].contains("--bin check-params-reference") {
                params_reference.push(tail.to_owned());
            } else if i + 1 < lines.len() && lines[i + 1].contains("rustc --edition=2021 tools/proc_stat_allocation_probe.rs") {
                proc_stat_probe.push(tail.to_owned());
            } else if i + 1 < lines.len() && lines[i + 1].contains("--output \"$RUNNER_TEMP/llvm18.deb\"") {
                llvm_provider.push(tail.to_owned());
            } else {
                arguments.push(tail.to_owned());
            }
        }
    }
    assert_eq!(arguments.len(), 179);
    assert_eq!(
        llvm_provider,
        vec![" --path \"$RUNNER_TEMP\" --growth-mib 256"]
    );
    assert_eq!(
        proc_stat_probe,
        vec![" --path \"$GITHUB_WORKSPACE\" --growth-mib 64"]
    );
    assert_eq!(
        miri_hashes,
        vec![" --path \"$GITHUB_WORKSPACE\" --growth-mib 512"]
    );
    assert_eq!(
        params_reference,
        vec![" --path \"$GITHUB_WORKSPACE\" --growth-mib 512"]
    );
    assert_eq!(
        command_checkers,
        vec![" --path \"$GITHUB_WORKSPACE\" --growth-mib 512"]
    );
    assert_eq!(
        egl_reference,
        vec![" --path \"$GITHUB_WORKSPACE\" --growth-mib 512"]
    );
    assert_eq!(
        message_state,
        vec![" --path \"$GITHUB_WORKSPACE\" --growth-mib 512"]
    );
    assert_eq!(
        artifact_list,
        vec![" --path \"$GITHUB_WORKSPACE\" --growth-mib 512"]
    );
    assert_eq!(
        diagnostic_checkers,
        vec![" --path \"$GITHUB_WORKSPACE\" --growth-mib 512"]
    );
    assert_eq!(
        translation_checker,
        vec![" --path \"$GITHUB_WORKSPACE\" --growth-mib 2048"]
    );
    assert_eq!(
        model_source,
        vec![" --path \"$GITHUB_WORKSPACE\" --growth-mib 128"]
    );
    assert_eq!(
        encoder_builder,
        vec![" --path \"$GITHUB_WORKSPACE\" --growth-mib 2048"]
    );
    assert_eq!(
        native_architecture,
        vec![" --path \"$GITHUB_WORKSPACE\" --growth-mib 1024"]
    );
    // Retain the exact original 179 guards/hash below, plus admission for the
    // new logging builder. A new build must not replace an inherited reserve.
    assert_eq!(
        logging_builder,
        vec![" --path \"$GITHUB_WORKSPACE\" --growth-mib 2048"]
    );
    assert_eq!(
        raylib,
        vec![" --path \"$GITHUB_WORKSPACE\" --growth-mib 2048"; 5]
    );
    let native = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l.contains("--bin stage-startup-ui-raylib"))
        .collect::<Vec<_>>();
    assert_eq!(native.len(), 5);
    let native_args = native
        .iter()
        .map(|(_, line)| {
            line.split_once("--bin stage-startup-ui-raylib -- ")
                .unwrap()
                .1
                .to_owned()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        hash(&native_args),
        "2efc90425a3b4e1cecaabbc410c6d13649b89ad4fbc3a9d722b33b510694b396"
    );
    for (i, line) in native {
        assert!(line.contains("cargo +1.94.0 run"));
        assert!(lines[i - 1].contains(marker));
    }
    assert!(!text.contains("stage_startup_ui_raylib.py"));
    assert_eq!(
        hash(&arguments),
        "952f3b9fc8820e701aae55f88368236acddd3da2af55465448aa00eddaf9fe55"
    );
    let template = fs::read_to_string(root().join("rust/crates/carrot-man/ci-jobs.yml")).unwrap();
    assert!(!template.contains("card_ci_space.py"));
    assert_eq!(template.matches(marker).count(), 9);
    let args = template
        .lines()
        .filter_map(|l| l.split_once(marker).map(|(_, s)| s.to_owned()))
        .collect::<Vec<_>>();
    assert_eq!(
        hash(&args),
        "471b43e7b994adca394eb5a342a5cdbaca211fec1b306dc219cc9c0f767a03b7"
    );
}
fn find_named(path: &Path, name: &str) -> Vec<PathBuf> {
    let mut found = vec![];
    for entry in fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.file_name().unwrap() == name {
            found.push(path.clone());
        }
        if path.is_dir() {
            found.extend(find_named(&path, name));
        }
    }
    found
}
#[test]
fn failed_build_never_publishes_guard() {
    let mut f = Fixture::new("999999999999");
    f.env.insert("GUARD_BUILD_FAIL".into(), "yes".into());
    assert_ne!(f.invoke().code, 0);
    assert!(find_named(Path::new(&f.env["RUNNER_TEMP"]), "disk-space-guard").is_empty());
    f.env.remove("GUARD_BUILD_FAIL");
    success(&f.invoke());
    assert_eq!(
        f.lines().iter().filter(|l| l.starts_with("cargo ")).count(),
        2
    );
}
#[test]
fn parallel_cold_calls_publish_once() {
    let f = Fixture::new("999999999999");
    std::thread::scope(|s| {
        let a = s.spawn(|| f.invoke());
        let b = s.spawn(|| f.invoke());
        success(&a.join().unwrap());
        success(&b.join().unwrap());
    });
    let lines = f.lines();
    assert_eq!(lines.iter().filter(|l| l.starts_with("cargo ")).count(), 1);
    assert_eq!(lines.iter().filter(|l| l.starts_with("native ")).count(), 2);
}
#[test]
fn requested_operation_filesystem_is_checked_before_toolchain_work() {
    let mut f = Fixture::new("999999999999");
    let operation = f.path.join("operation space");
    fs::create_dir(&operation).unwrap();
    f.env.insert(
        "GUARD_OPERATION_PATH".into(),
        operation.join("new output").display().to_string(),
    );
    f.env.insert(
        "GUARD_OPERATION_FREE".into(),
        (25 * 1024u64.pow(3) + 128 * 1024u64.pow(2) - 1).to_string(),
    );
    let result = f.invoke();
    assert_ne!(result.code, 0);
    assert!(String::from_utf8_lossy(&result.stderr).contains("recover at least 35 GiB"));
    assert!(!f.log.exists());
}
#[test]
fn arm_runner_bootstraps_native_host_even_with_cross_environment() {
    let mut f = Fixture::new("999999999999");
    executable(&f.path.join("commands/uname"), "printf 'aarch64\\n'\n");
    f.env.insert(
        "CARGO_BUILD_TARGET".into(),
        "x86_64-unknown-linux-gnu".into(),
    );
    success(&f.invoke());
    assert!(fs::read_to_string(&f.log)
        .unwrap()
        .contains("--target aarch64-unknown-linux-gnu"));
}
#[test]
fn bad_df_output_fails_closed_before_install() {
    let f = Fixture::new("not-a-number");
    assert_ne!(f.invoke().code, 0);
    assert!(!f.log.exists());
}
#[test]
fn invalid_cli_never_installs_or_compiles() {
    let f = Fixture::new("999999999999");
    for args in [
        &["--path", ".", "--path", ".", "--growth-mib", "0"][..],
        &["--path", ".", "--growth-mib", "0", "--growth-mib", "0"],
        &["--growth-mib", "0"],
        &["--path", ".", "--growth-mib", "0", "--unknown"],
    ] {
        let result = f.invoke_args(bootstrap().to_str().unwrap(), args);
        assert_ne!(result.code, 0, "{args:?}");
        assert!(result.stdout.is_empty());
        assert!(!f.log.exists());
    }
}
fn copy_tree(source: &Path, target: &Path) {
    fs::create_dir_all(target).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let to = target.join(entry.file_name());
        if entry.path().is_dir() {
            copy_tree(&entry.path(), &to);
        } else {
            fs::copy(entry.path(), to).unwrap();
        }
    }
}
#[test]
fn changed_source_invalidates_cached_executable() {
    let mut f = Fixture::new("999999999999");
    let repo = f.path.join("repository");
    fs::create_dir_all(repo.join("rust/tools")).unwrap();
    fs::copy(bootstrap(), repo.join("rust/tools/check-disk-space")).unwrap();
    for name in ["Cargo.toml", "Cargo.lock"] {
        fs::copy(root().join("rust").join(name), repo.join("rust").join(name)).unwrap();
    }
    copy_tree(
        &root().join("rust/crates/disk-space"),
        &repo.join("rust/crates/disk-space"),
    );
    f.env.insert(
        "GUARD_BOOTSTRAP".into(),
        repo.join("rust/tools/check-disk-space")
            .display()
            .to_string(),
    );
    success(&f.invoke());
    for name in [
        "rust/crates/disk-space/src/lib.rs",
        "rust/Cargo.lock",
        "rust/Cargo.toml",
    ] {
        let path = repo.join(name);
        let mut text = fs::read_to_string(&path).unwrap();
        text.push('\n');
        fs::write(path, text).unwrap();
        success(&f.invoke());
    }
    assert_eq!(
        f.lines().iter().filter(|l| l.starts_with("cargo ")).count(),
        4
    );
}
#[test]
fn interrupted_publication_exposes_no_partial_executable() {
    let f = Fixture::new("999999999999");
    let copier = f.path.join("commands/cp");
    executable(&copier,"for argument do destination=$argument; done\nprintf 'partial executable' > \"$destination\"\nexit 17\n");
    assert_ne!(f.invoke().code, 0);
    for entry in fs::read_dir(f.cache()).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            assert!(!path.join("disk-space-guard").exists());
            assert!(!fs::read_dir(path).unwrap().any(|v| v
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".publish.")));
        }
    }
    assert!(!f.lines().iter().any(|l| l.starts_with("native ")));
    fs::remove_file(copier).unwrap();
    success(&f.invoke());
}
#[test]
fn cache_rejects_unsafe_entries_without_executing_or_building() {
    for kind in [
        "base-mode",
        "base-symlink",
        "lock-fifo",
        "lock-symlink",
        "binary-mode",
        "binary-symlink",
        "key-mode",
        "lock-mode",
        "base-owner",
    ] {
        let f = Fixture::new("999999999999");
        let base = f.cache();
        if kind.starts_with("binary") || kind == "key-mode" {
            success(&f.invoke());
            let binary = f.binary();
            if kind == "key-mode" {
                fs::set_permissions(binary.parent().unwrap(), fs::Permissions::from_mode(0o777))
                    .unwrap();
            } else if kind == "binary-mode" {
                fs::set_permissions(binary, fs::Permissions::from_mode(0o777)).unwrap();
            } else {
                let target = f.path.join("sentinel");
                fs::rename(&binary, &target).unwrap();
                symlink(target, binary).unwrap();
            }
            fs::remove_file(&f.log).unwrap();
        } else if kind == "base-symlink" {
            let target = f.path.join("sentinel");
            fs::create_dir(&target).unwrap();
            symlink(target, &base).unwrap();
        } else {
            fs::create_dir(&base).unwrap();
            fs::set_permissions(&base, fs::Permissions::from_mode(0o700)).unwrap();
            match kind {
                "base-mode" => {
                    fs::set_permissions(&base, fs::Permissions::from_mode(0o777)).unwrap()
                }
                "lock-mode" => {
                    let lock = base.join("bootstrap.lock");
                    fs::write(&lock, "untouched").unwrap();
                    fs::set_permissions(lock, fs::Permissions::from_mode(0o666)).unwrap();
                }
                "base-owner" => executable(
                    &f.path.join("commands/stat"),
                    "printf '%s 700\\n' \"$(( $(id -u) + 1 ))\"\n",
                ),
                "lock-fifo" => {
                    let result = std::process::Command::new("mkfifo")
                        .arg(base.join("bootstrap.lock"))
                        .status()
                        .unwrap();
                    assert!(result.success());
                }
                _ => {
                    let target = f.path.join("sentinel");
                    fs::write(&target, "untouched").unwrap();
                    symlink(target, base.join("bootstrap.lock")).unwrap();
                }
            }
        }
        let result = f.invoke_args(
            bootstrap().to_str().unwrap(),
            &["--path", ".", "--growth-mib", "0"],
        );
        assert_ne!(result.code, 0, "{kind}");
        assert!(!f.log.exists(), "{kind}");
        if kind == "lock-symlink" {
            assert_eq!(
                fs::read_to_string(f.path.join("sentinel")).unwrap(),
                "untouched"
            );
        }
    }
}
#[test]
fn local_cache_uses_private_user_cache_and_secure_modes() {
    let mut f = Fixture::new("999999999999");
    f.env.remove("RUNNER_TEMP");
    f.env.insert(
        "XDG_CACHE_HOME".into(),
        f.path.join("user-cache").display().to_string(),
    );
    success(&f.invoke());
    let base = f.path.join("user-cache/openpilot-disk-guard");
    let binary = find_named(&base, "disk-space-guard")
        .into_iter()
        .find(|p| p.parent().unwrap().parent().unwrap() == base)
        .unwrap();
    for path in [&base, binary.parent().unwrap(), &binary] {
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o700
        );
    }
    success(&f.invoke());
    assert_eq!(
        f.lines().iter().filter(|l| l.starts_with("cargo ")).count(),
        1
    );
}
