#![cfg(unix)]
use std::{
    fs,
    os::unix::fs::{symlink, PermissionsExt},
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};
struct Fixture {
    root: PathBuf,
    source: PathBuf,
    cache: PathBuf,
    bin: PathBuf,
    warm: bool,
    install_status: i32,
}
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "service-bootstrap-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let source = root.join("source tree");
        let bin = root.join("tools");
        let cache = root.join("external cache");
        fs::create_dir_all(source.join("tools/scripts")).unwrap();
        fs::create_dir_all(source.join("rust")).unwrap();
        fs::create_dir(&bin).unwrap();
        fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../tools/scripts/service_ports"),
            source.join("tools/scripts/service_ports"),
        )
        .unwrap();
        for name in ["bash", "awk", "dirname", "env", "mkdir"] {
            let found = Command::new("/bin/sh")
                .args(["-c", &format!("command -v {name}")])
                .output()
                .unwrap();
            symlink(
                String::from_utf8(found.stdout).unwrap().trim(),
                bin.join(name),
            )
            .unwrap();
        }
        for (name,body) in [
   ("uname","#!/bin/bash\ncase \"$1\" in -s) printf '%s\\n' \"$FAKE_OS\" ;; -m) printf '%s\\n' \"$FAKE_ARCH\" ;; *) exit 99 ;; esac\n"),
   ("df","#!/bin/bash\nprintf 'df:%s\\n' \"$*\" >> \"$TRACE\"\nprintf 'Filesystem 1024-blocks Used Available Capacity Mounted on\\n'\nprintf '/dev/disk with spaces 999999999 1 %s 1%% /mount with spaces\\n' \"$FREE_KIB\"\n"),
   ("rustup","#!/bin/bash\nprintf 'rustup:%s\\n' \"$*\" >> \"$TRACE\"\nif [ \"$1\" = run ]; then exit \"$TOOLCHAIN_STATUS\"; fi\nprintf 'rustup stdout\\n'\nexit \"${RUSTUP_STATUS:-0}\"\n"),
   ("cargo",r#"#!/bin/bash
set -eu
printf 'cargo:%s\n' "$*" >> "$TRACE"
test -z "${CARGO_BUILD_TARGET+x}${RUSTFLAGS+x}${CARGO_ENCODED_RUSTFLAGS+x}${RUSTC_WRAPPER+x}${RUSTC_WORKSPACE_WRAPPER+x}"
test "$CARGO_INCREMENTAL" = 0
test "$CARGO_BUILD_JOBS" = 2
test "$CARGO_PROFILE_DEV_DEBUG" = 0
printf 'cargo stdout\n'; printf 'cargo stderr\n' >&2
if [ "${CARGO_STATUS:-0}" != 0 ]; then exit "$CARGO_STATUS"; fi
cache= host=
while (($#)); do case "$1" in --target) host=$2; shift ;; --target-dir) cache=$2; shift ;; esac; shift; done
mkdir -p "$cache/$host/debug"
printf '#!/bin/bash\nprintf "native service rows\\n"\nprintf "native selected\\n" >&2\nprintf "native:%%s\\n" "$*" >> "$TRACE"\n' > "$cache/$host/debug/list-service-ports"
chmod 755 "$cache/$host/debug/list-service-ports"
"#),
  ]{let path=bin.join(name);fs::write(&path,body).unwrap();fs::set_permissions(path,fs::Permissions::from_mode(0o755)).unwrap();}
        // Fake cargo needs chmod; all supplied tools are non-Python.
        let found = Command::new("/bin/sh")
            .args(["-c", "command -v chmod"])
            .output()
            .unwrap();
        symlink(
            String::from_utf8(found.stdout).unwrap().trim(),
            bin.join("chmod"),
        )
        .unwrap();
        Self {
            root,
            source,
            cache,
            bin,
            warm: false,
            install_status: 0,
        }
    }
    fn run(
        &self,
        os: &str,
        arch: &str,
        free: &str,
        cargo_status: &str,
        cache: &Path,
        override_binary: Option<&Path>,
    ) -> Output {
        let mut command = Command::new("/bin/bash");
        command
            .arg(self.source.join("tools/scripts/service_ports"))
            .arg("kept argument")
            .env("PATH", &self.bin)
            .env("FAKE_OS", os)
            .env("FAKE_ARCH", arch)
            .env("FREE_KIB", free)
            .env("CARGO_STATUS", cargo_status)
            .env("TOOLCHAIN_STATUS", if self.warm { "0" } else { "1" })
            .env("RUSTUP_STATUS", self.install_status.to_string())
            .env("TRACE", self.root.join("trace"))
            .env("OPENPILOT_SERVICE_TOOLS_CACHE", cache)
            .env("RUSTUP_HOME", self.root.join("toolchain"))
            .env("CARGO_BUILD_TARGET", "bad-cross-target")
            .env("RUSTFLAGS", "bad-cross-flags")
            .env("CARGO_ENCODED_RUSTFLAGS", "bad-cross-flags")
            .env("RUSTC_WRAPPER", "bad-wrapper")
            .env("RUSTC_WORKSPACE_WRAPPER", "bad-wrapper");
        command.env_remove("OPENPILOT_SERVICE_PORTS");
        if let Some(binary) = override_binary {
            command.env("OPENPILOT_SERVICE_PORTS", binary);
        }
        command.output().unwrap()
    }
    fn trace(&self) -> String {
        fs::read_to_string(self.root.join("trace")).unwrap_or_default()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
#[test]
fn portable_hosts_build_guarded_pinned_binary_with_clean_cross_flags_and_only_rows_on_stdout() {
    for (os, arch, host) in [
        ("Linux", "x86_64", "x86_64-unknown-linux-gnu"),
        ("Darwin", "arm64", "aarch64-apple-darwin"),
        ("Darwin", "x86_64", "x86_64-apple-darwin"),
    ] {
        let fixture = Fixture::new();
        let result = fixture.run(os, arch, "28311552", "0", &fixture.cache, None);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(result.stdout, b"native service rows\n");
        let stderr = String::from_utf8(result.stderr).unwrap();
        for log in [
            "rustup stdout",
            "cargo stdout",
            "cargo stderr",
            "native selected",
        ] {
            assert!(stderr.contains(log), "{stderr}");
        }
        let trace = fixture.trace();
        let rows = trace.lines().collect::<Vec<_>>();
        assert_eq!(rows.len(), 7, "{trace}");
        assert!(rows[0].starts_with("df:-Pk "));
        assert_eq!(rows[1], "rustup:run 1.94.0 rustc --version");
        assert!(rows[2].starts_with("df:-Pk "));
        assert_eq!(rows[3], "rustup:toolchain install 1.94.0 --profile minimal");
        assert!(rows[4].starts_with("df:-Pk "));
        assert!(rows[5].contains("+1.94.0 build --manifest-path"));
        assert!(rows[5].contains("--locked -p openpilot-service-tools --bin list-service-ports"));
        assert!(rows[5].contains(&format!("--target {host} --target-dir")));
        assert_eq!(rows[6], "native:kept argument");
    }
}
#[test]
fn low_disk_prevents_install_build_and_cache_creation() {
    let fixture = Fixture::new();
    let output = fixture.run("Darwin", "arm64", "28311551", "0", &fixture.cache, None);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(!fixture.cache.exists());
    let trace = fixture.trace();
    assert_eq!(trace.lines().count(), 1);
    assert!(trace.starts_with("df:"));
}
#[test]
fn build_failure_keeps_build_output_on_stderr_without_selecting_binary() {
    let fixture = Fixture::new();
    let output = fixture.run("Linux", "x86_64", "28311552", "23", &fixture.cache, None);
    assert_eq!(output.status.code(), Some(23));
    assert!(output.stdout.is_empty());
    assert!(!fixture.trace().contains("native:"));
    assert!(String::from_utf8(output.stderr)
        .unwrap()
        .contains("cargo stdout"));
}
#[test]
fn cache_inside_source_is_rejected_before_install_or_mkdir() {
    let fixture = Fixture::new();
    let cache = fixture.source.join("must not create");
    let output = fixture.run("Darwin", "x86_64", "28311552", "0", &cache, None);
    assert!(!output.status.success());
    assert!(!cache.exists());
    assert!(!fixture.trace().contains("rustup:"));
    assert!(!fixture.trace().contains("cargo:"));
}
#[test]
fn explicit_binary_override_runs_without_guard_toolchain_or_python() {
    let fixture = Fixture::new();
    let binary = fixture.root.join("selected");
    fs::write(
        &binary,
        "#!/bin/bash\nprintf 'selected:%s\\n' \"$1\"\nexit 29\n",
    )
    .unwrap();
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).unwrap();
    let output = fixture.run(
        "unsupported",
        "unsupported",
        "0",
        "99",
        &fixture.cache,
        Some(&binary),
    );
    assert_eq!(output.status.code(), Some(29));
    assert_eq!(output.stdout, b"selected:kept argument\n");
    assert!(output.stderr.is_empty());
    assert!(fixture.trace().is_empty());
}

#[test]
fn warm_toolchain_probe_skips_installer_and_keeps_guard_before_build() {
    let mut fixture = Fixture::new();
    fixture.warm = true;
    let output = fixture.run("Darwin", "arm64", "28311552", "0", &fixture.cache, None);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let trace = fixture.trace();
    let rows = trace.lines().collect::<Vec<_>>();
    assert_eq!(rows.len(), 5, "{trace}");
    assert!(rows[0].starts_with("df:"));
    assert_eq!(rows[1], "rustup:run 1.94.0 rustc --version");
    assert!(rows[2].starts_with("df:"));
    assert!(rows[3].starts_with("cargo:"));
    assert!(!trace.contains("toolchain install"));
}
#[test]
fn failed_toolchain_install_stops_before_cargo_and_binary() {
    let mut fixture = Fixture::new();
    fixture.install_status = 31;
    let output = fixture.run("Darwin", "arm64", "28311552", "0", &fixture.cache, None);
    assert_eq!(output.status.code(), Some(31));
    assert!(output.stdout.is_empty());
    let trace = fixture.trace();
    assert!(trace.contains("toolchain install"));
    assert!(!trace.contains("cargo:"));
    assert!(!trace.contains("native:"));
}
#[test]
fn symlink_alias_into_source_is_rejected_before_any_install_or_directory_creation() {
    let fixture = Fixture::new();
    let alias = fixture.root.join("source alias");
    symlink(&fixture.source, &alias).unwrap();
    let cache = alias.join("alias cache must not exist");
    let output = fixture.run("Darwin", "arm64", "28311552", "0", &cache, None);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(!fixture.source.join("alias cache must not exist").exists());
    assert!(!fixture.trace().contains("rustup:"));
    assert!(!fixture.trace().contains("cargo:"));
}
#[test]
fn missing_component_then_parent_into_source_is_rejected_without_creating_either_path() {
    let fixture = Fixture::new();
    let cache = fixture
        .root
        .join("missing/../source tree/lexical cache must not exist");
    let output = fixture.run("Linux", "x86_64", "28311552", "0", &cache, None);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(!fixture.root.join("missing").exists());
    assert!(!fixture.source.join("lexical cache must not exist").exists());
    assert!(!fixture.trace().contains("rustup:"));
    assert!(!fixture.trace().contains("cargo:"));
}
#[test]
fn missing_parent_then_existing_alias_is_resolved_before_source_cache_creation() {
    let fixture = Fixture::new();
    let alias = fixture.root.join("alias after parent");
    symlink(&fixture.source, &alias).unwrap();
    let cache = fixture
        .root
        .join("missing/../alias after parent/combined cache must not exist");
    let output = fixture.run("Darwin", "arm64", "28311552", "0", &cache, None);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(!fixture
        .source
        .join("combined cache must not exist")
        .exists());
    assert!(!fixture.root.join("missing").exists());
    assert!(!fixture.trace().contains("rustup:"));
    assert!(!fixture.trace().contains("cargo:"));
}
#[test]
fn nested_alias_then_parent_uses_physical_target_parent_before_source_rejection() {
    // Independent Path.resolve(strict=False) capture in fixtures/cache-nested-alias.json:
    // missing/../aliases/nested/../nested parent cache -> $ROOT/source tree/nested parent cache.
    let fixture = Fixture::new();
    fs::create_dir(fixture.source.join("inside")).unwrap();
    fs::create_dir(fixture.root.join("aliases")).unwrap();
    symlink("../source tree/inside", fixture.root.join("aliases/nested")).unwrap();
    let cache = fixture
        .root
        .join("missing/../aliases/nested/../nested parent cache");
    let output = fixture.run("Darwin", "arm64", "28311552", "0", &cache, None);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(!fixture.source.join("nested parent cache").exists());
    assert!(!fixture.root.join("missing").exists());
    assert!(!fixture.trace().contains("rustup:"));
    assert!(!fixture.trace().contains("cargo:"));
}
