//! SCons host-tool bootstrap boundaries. Controlled rustup/Cargo are process
//! fixtures only; these tests make no claims about generated DBC content.
#![cfg(unix)]

use openpilot_dev_tools::qa_process::{Captured, CommandSpec, NativeRunner, Runner};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    os::unix::fs::{symlink, PermissionsExt},
    path::{Path, PathBuf},
    time::Duration,
};

fn source_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}
fn executable(path: &Path, text: &str) {
    fs::write(path, format!("#!/bin/bash\nset -eu\n{text}\n")).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}
fn system_command(name: &str) -> PathBuf {
    std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .map(|directory| directory.join(name))
        .find(|path| {
            path.metadata().is_ok_and(|metadata| {
                metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
            })
        })
        .map(|path| fs::canonicalize(path).unwrap())
        .unwrap_or_else(|| panic!("required process-boundary test command absent: {name}"))
}

struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    commands: PathBuf,
    cwd: PathBuf,
    log: PathBuf,
    env: BTreeMap<String, String>,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("source root");
        let commands = temp.path().join("commands");
        let cwd = temp.path().join("unrelated working directory");
        for path in [root.join("rust/tools"), commands.clone(), cwd.clone()] {
            fs::create_dir_all(path).unwrap();
        }
        fs::copy(
            source_root().join("rust/tools/build-dbc-generator"),
            root.join("rust/tools/build-dbc-generator"),
        )
        .unwrap();
        // Tight PATH: absence/installation tests cannot find the host's rustup
        // or Cargo and accidentally invoke a real compiler.
        for command in [
            "bash", "dirname", "awk", "sed", "env", "mkdir", "chmod", "mktemp", "cp", "cmp", "mv",
            "rm",
        ] {
            symlink(system_command(command), commands.join(command)).unwrap();
        }
        let log = temp.path().join("process.log");
        executable(
            &commands.join("df"),
            r#"
printf 'df:%s\n' "$*" >> "$DBC_TEST_LOG"
printf 'Filesystem 1024-blocks Used Available Capacity Mounted on\nfixture 50000000 1 %s 1%% /\n' "${DBC_TEST_FREE_KIB:-40000000}"
"#,
        );
        executable(
            &commands.join("rustup"),
            r#"
printf 'rustup:%s\n' "$*" >> "$DBC_TEST_LOG"
if [[ "$1" == toolchain ]]; then
    [[ "$*" == 'toolchain install 1.94.0 --profile minimal' ]]
    test "${DBC_TEST_INSTALL_FAIL:-0}" = 0 || exit 19
    printf installed > "$DBC_TEST_INSTALLED"
    exit 0
fi
[[ "$1" == run && "$2" == 1.94.0 ]]
shift 2
if [[ "$1" == rustc ]]; then
    [[ "$*" == 'rustc -vV' ]]
    if [[ "${DBC_TEST_NEEDS_INSTALL:-0}" == 1 && ! -f "$DBC_TEST_INSTALLED" ]]; then exit 1; fi
    if [[ "${DBC_TEST_NO_HOST:-0}" != 1 ]]; then printf 'rustc 1.94.0\nhost: x86_64-test-native\n'; fi
    exit 0
fi
[[ "$1" == cargo ]]
exec "$@"
"#,
        );
        executable(
            &commands.join("cargo"),
            r#"
printf 'cargo:%s\n' "$*" >> "$DBC_TEST_LOG"
test -z "${RUSTFLAGS+x}${CARGO_ENCODED_RUSTFLAGS+x}${CARGO_BUILD_TARGET+x}${RUSTC_WRAPPER+x}${RUSTC_WORKSPACE_WRAPPER+x}${CC+x}${CXX+x}${CFLAGS+x}${CXXFLAGS+x}${CPPFLAGS+x}${LDFLAGS+x}" || exit 90
[[ "$CARGO_INCREMENTAL" == 0 && "$CARGO_BUILD_JOBS" == 2 && "$CARGO_PROFILE_DEV_DEBUG" == 0 ]] || exit 91
printf 'bounded:jobs=%s incremental=%s debug=%s\n' "$CARGO_BUILD_JOBS" "$CARGO_INCREMENTAL" "$CARGO_PROFILE_DEV_DEBUG" >> "$DBC_TEST_LOG"
test "${DBC_TEST_CARGO_FAIL:-0}" = 0 || exit 37
target= host= manifest=
while (($#)); do
    case "$1" in
        --target-dir) target=$2; shift ;;
        --target) host=$2; shift ;;
        --manifest-path) manifest=$2; shift ;;
    esac
    shift
done
[[ "$manifest" == "$DBC_TEST_ROOT/rust/Cargo.toml" && "$host" == x86_64-test-native ]]
mkdir -p "$target/$host/debug"
printf '#!/bin/sh\nexit 0\n' > "$target/$host/debug/dbc-generator"
chmod +x "$target/$host/debug/dbc-generator"
"#,
        );
        let mut env = BTreeMap::from([
            ("PATH".into(), commands.display().to_string()),
            ("DBC_TEST_ROOT".into(), root.display().to_string()),
            ("DBC_TEST_LOG".into(), log.display().to_string()),
            (
                "DBC_TEST_INSTALLED".into(),
                temp.path().join("installed").display().to_string(),
            ),
            ("CARGO_TARGET_DIR".into(), "relative target".into()),
            (
                "CARGO_HOME".into(),
                temp.path().join("cargo home").display().to_string(),
            ),
            (
                "RUSTUP_HOME".into(),
                temp.path().join("rustup home").display().to_string(),
            ),
        ]);
        for key in [
            "RUSTFLAGS",
            "CARGO_ENCODED_RUSTFLAGS",
            "CARGO_BUILD_TARGET",
            "RUSTC_WRAPPER",
            "RUSTC_WORKSPACE_WRAPPER",
            "CC",
            "CXX",
            "CFLAGS",
            "CXXFLAGS",
            "CPPFLAGS",
            "LDFLAGS",
        ] {
            env.insert(key.into(), "cross-environment-must-not-reach-cargo".into());
        }
        Self {
            _temp: temp,
            root,
            commands,
            cwd,
            log,
            env,
        }
    }
    fn invoke(&self, argv: Vec<OsString>, extra: &[(&str, &str)]) -> Captured {
        let mut spec = CommandSpec::new_os(argv, Duration::from_secs(5));
        spec.cwd = Some(self.cwd.clone());
        spec.env = self.env.clone();
        for (key, value) in extra {
            spec.env.insert((*key).into(), (*value).into());
        }
        NativeRunner.run(&spec).unwrap()
    }
    fn bootstrap(&self, extra: &[(&str, &str)]) -> Captured {
        self.invoke(
            vec![self
                .root
                .join("rust/tools/build-dbc-generator")
                .into_os_string()],
            extra,
        )
    }
    fn lines(&self) -> Vec<String> {
        fs::read_to_string(&self.log)
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }
    fn binary(&self) -> PathBuf {
        self.root.join("relative target/debug/dbc-generator")
    }
}
fn success(output: &Captured) {
    assert_eq!(
        output.code,
        0,
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn relative_target_is_repository_rooted_and_publishes_absolute_native_host_output() {
    let f = Fixture::new();
    let output = f.bootstrap(&[]);
    success(&output);
    assert_eq!(
        output.stdout,
        format!("{}\n", f.binary().display()).as_bytes()
    );
    assert!(f.binary().is_file());
    assert!(!f.cwd.join("relative target").exists());
    let lines = f.lines();
    assert_eq!(
        lines
            .iter()
            .filter(|line| line.starts_with("cargo:"))
            .count(),
        1
    );
    let cargo = lines
        .iter()
        .find(|line| line.starts_with("cargo:"))
        .unwrap();
    assert!(cargo.contains(
        "-p openpilot-dev-tools --features dbc-generator-native --bin dbc-generator --locked"
    ));
    assert!(cargo.contains("--target x86_64-test-native --target-dir"));
    assert!(lines
        .iter()
        .any(|line| line == "bounded:jobs=2 incremental=0 debug=0"));
}

#[test]
fn low_disk_refuses_before_any_rustup_or_cargo_process() {
    let f = Fixture::new();
    let output = f.bootstrap(&[("DBC_TEST_FREE_KIB", "1")]);
    assert_ne!(output.code, 0);
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("operation requires 2048 MiB growth"));
    assert!(f.lines().iter().all(|line| line.starts_with("df:")));
    assert!(!f.binary().exists());
}

#[test]
fn absent_rustup_is_actionable_and_never_attempts_cargo() {
    let f = Fixture::new();
    fs::remove_file(f.commands.join("rustup")).unwrap();
    let output = f.bootstrap(&[]);
    assert_ne!(output.code, 0);
    assert!(String::from_utf8_lossy(&output.stderr).contains("install rustup"));
    assert!(f.lines().iter().all(|line| line.starts_with("df:")));
}

#[test]
fn missing_pinned_toolchain_is_installed_only_after_space_checks() {
    let f = Fixture::new();
    let output = f.bootstrap(&[("DBC_TEST_NEEDS_INSTALL", "1")]);
    success(&output);
    assert_eq!(
        output.stdout,
        format!("{}\n", f.binary().display()).as_bytes()
    );
    let lines = f.lines();
    let install = lines
        .iter()
        .position(|line| line == "rustup:toolchain install 1.94.0 --profile minimal")
        .unwrap();
    assert_eq!(
        lines[..install]
            .iter()
            .filter(|line| line.starts_with("df:"))
            .count(),
        3
    );
    assert!(lines[install + 1..]
        .iter()
        .any(|line| line.starts_with("cargo:")));
    let failed = Fixture::new();
    let output = failed.bootstrap(&[
        ("DBC_TEST_NEEDS_INSTALL", "1"),
        ("DBC_TEST_INSTALL_FAIL", "1"),
    ]);
    assert_eq!(output.code, 19);
    assert!(!failed.lines().iter().any(|line| line.starts_with("cargo:")));
}

#[test]
fn cargo_failure_propagates_without_publishing_a_host_path() {
    let f = Fixture::new();
    let output = f.bootstrap(&[("DBC_TEST_CARGO_FAIL", "1")]);
    assert_eq!(output.code, 37);
    assert!(output.stdout.is_empty());
    assert!(!f.binary().exists());
}

#[test]
fn absent_native_host_fails_before_cargo_instead_of_guessing_a_cross_target() {
    let f = Fixture::new();
    let output = f.bootstrap(&[("DBC_TEST_NO_HOST", "1")]);
    assert_ne!(output.code, 0);
    assert!(String::from_utf8_lossy(&output.stderr).contains("missing native host triple"));
    assert!(!f.lines().iter().any(|line| line.starts_with("cargo:")));
}

#[test]
fn publication_is_exact_executable_and_atomic_for_existing_inode_readers() {
    use std::io::Read;
    let f = Fixture::new();
    let stable = f.binary();
    fs::create_dir_all(stable.parent().unwrap()).unwrap();
    let old = b"old generator inode remains usable";
    fs::write(&stable, old).unwrap();
    let mut held = fs::File::open(&stable).unwrap();
    let output = f.bootstrap(&[]);
    success(&output);
    assert_eq!(output.stdout, format!("{}\n", stable.display()).as_bytes());
    let host = f
        .root
        .join("relative target/x86_64-test-native/debug/dbc-generator");
    assert_eq!(fs::read(&stable).unwrap(), fs::read(host).unwrap());
    assert_eq!(
        fs::metadata(&stable).unwrap().permissions().mode() & 0o777,
        0o755
    );
    let mut retained = Vec::new();
    held.read_to_end(&mut retained).unwrap();
    assert_eq!(
        retained, old,
        "publication must replace, not overwrite an active inode"
    );
    assert!(fs::read_dir(stable.parent().unwrap())
        .unwrap()
        .all(|entry| !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".dbc-generator.")));
}

#[test]
fn exact_operation_growth_replaces_the_withdrawn_fixed_floor() {
    for free_kib in [2048 * 1024 - 1, 2048 * 1024] {
        let f = Fixture::new();
        let value = free_kib.to_string();
        let output = f.bootstrap(&[("DBC_TEST_FREE_KIB", &value)]);
        assert_eq!(output.code == 0, free_kib == 2048 * 1024);
        if free_kib < 2048 * 1024 {
            assert!(f.lines().iter().all(|line| line.starts_with("df:")));
            assert!(!f.binary().exists());
        } else {
            assert!(f.binary().exists());
        }
    }
}
