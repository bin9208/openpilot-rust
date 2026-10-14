use openpilot_dev_tools::{
    elf_audit::{audit, elf},
    package::disk_guard,
};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
struct Fixture {
    temp: tempfile::TempDir,
}
impl Fixture {
    fn new() -> Self {
        Self {
            temp: tempfile::tempdir().unwrap(),
        }
    }
    fn root(&self) -> &Path {
        self.temp.path()
    }
    fn library(
        &self,
        dir: &str,
        name: &str,
        source: &str,
        links: &[&str],
        version: Option<&str>,
    ) -> PathBuf {
        let directory = self.root().join(dir);
        fs::create_dir_all(&directory).unwrap();
        let code = directory.join(format!("{name}.c"));
        fs::write(&code, source).unwrap();
        let output = directory.join(name);
        disk_guard(&directory, 16 * 1024 * 1024).unwrap();
        let mut command = Command::new(std::env::var_os("CC").unwrap_or_else(|| "gcc".into()));
        command
            .args(["-shared", "-nostdlib", "-fPIC"])
            .arg(&code)
            .arg("-o")
            .arg(&output)
            .arg(format!("-Wl,-soname,{name}"));
        if let Some(version) = version {
            let script = directory.join(format!("{name}.map"));
            fs::write(
                &script,
                format!("{version} {{ global: supplied; other; local: *; }};\n"),
            )
            .unwrap();
            command.arg(format!("-Wl,--version-script={}", script.display()));
        }
        if !links.is_empty() {
            command
                .arg("-Wl,--no-as-needed")
                .arg(format!("-L{}", directory.display()));
            for link in links {
                command.arg(format!("-l:{link}"));
            }
        }
        let result = command.output().unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        output
    }
    fn provider(&self, dir: &str, version: &str, symbol: &str) -> PathBuf {
        self.library(
            dir,
            "libprovider.so",
            &format!("int {symbol}(void) {{ return 7; }}"),
            &[],
            Some(version),
        )
    }
    fn consumer(&self, dir: &str) -> PathBuf {
        self.library(
            dir,
            "consumer.so",
            "extern int supplied(void); int consume(void) { return supplied(); }",
            &["libprovider.so"],
            None,
        )
    }
}
#[test]
fn real_readelf_resolves_versions_default_aliases_and_ordered_provider_paths() {
    let f = Fixture::new();
    let good = f.provider("good", "PROVIDER_1", "supplied");
    let root = f.consumer("good");
    f.provider("bad", "PROVIDER_2", "supplied");
    let info = elf(&good).unwrap();
    assert!(info.exports.contains("supplied@PROVIDER_1"));
    assert!(info.exports.contains("supplied"));
    assert!(info.definitions.iter().any(|v| v == "PROVIDER_1"));
    let consumer = elf(&root).unwrap();
    assert_eq!(consumer.needed, ["libprovider.so"]);
    assert!(consumer.imports.contains("supplied@PROVIDER_1"));
    assert_eq!(consumer.needs["libprovider.so"], ["PROVIDER_1"]);
    let good_report = audit(
        std::slice::from_ref(&root),
        &[f.root().join("good"), f.root().join("bad")],
        62,
    )
    .unwrap();
    assert_eq!(good_report["compatible"], true);
    assert_eq!(good_report["objects"].as_array().unwrap().len(), 2);
    let bad_report = audit(&[root], &[f.root().join("bad"), f.root().join("good")], 62).unwrap();
    assert_eq!(bad_report["compatible"], false);
    assert_eq!(
        bad_report["roots"][0]["failures"][0]["missing_versions"]["libprovider.so"][0],
        "PROVIDER_1"
    );
}
#[test]
fn wrong_architecture_is_rejected_for_roots_and_every_provider() {
    let f = Fixture::new();
    let provider = f.provider("good", "PROVIDER_1", "supplied");
    let root = f.consumer("good");
    assert!(
        audit(std::slice::from_ref(&root), &[f.root().join("good")], 183)
            .unwrap_err()
            .to_string()
            .contains("unexpected ELF architecture")
    );
    let mut data = fs::read(&provider).unwrap();
    data[18..20].copy_from_slice(&183u16.to_le_bytes());
    fs::write(provider, data).unwrap();
    assert!(audit(&[root], &[f.root().join("good")], 62)
        .unwrap_err()
        .to_string()
        .contains("libprovider.so"));
}
#[test]
fn missing_provider_and_strong_symbol_fail_closed() {
    let f = Fixture::new();
    f.provider("good", "PROVIDER_1", "supplied");
    let root = f.consumer("good");
    assert!(
        audit(std::slice::from_ref(&root), &[f.root().join("missing")], 62)
            .unwrap_err()
            .to_string()
            .contains("missing provider libprovider.so")
    );
    f.provider("missing-symbol", "PROVIDER_1", "other");
    let report = audit(&[root], &[f.root().join("missing-symbol")], 62).unwrap();
    assert_eq!(report["compatible"], false);
    let failure = &report["roots"][0]["failures"][0];
    assert_eq!(failure["missing_versions"], serde_json::json!({}));
    assert_eq!(
        failure["missing_strong_symbols"],
        serde_json::json!(["supplied@PROVIDER_1"])
    );
}
#[test]
fn unrelated_root_exports_cannot_satisfy_a_missing_dependency() {
    let f = Fixture::new();
    let bad = f.library(
        "roots",
        "bad.so",
        "extern int required(void); int bad(void) { return required(); }",
        &[],
        None,
    );
    let unrelated = f.library(
        "roots",
        "unrelated.so",
        "int required(void) { return 42; }",
        &[],
        None,
    );
    let report = audit(&[bad, unrelated], &[], 62).unwrap();
    assert_eq!(report["compatible"], false);
    assert_eq!(report["roots"][0]["compatible"], false);
    assert_eq!(report["roots"][0]["closure"].as_array().unwrap().len(), 1);
    assert_eq!(
        report["roots"][0]["failures"][0]["missing_strong_symbols"],
        serde_json::json!(["required"])
    );
    assert_eq!(report["roots"][1]["compatible"], true);
}
#[test]
fn weak_undefined_symbols_do_not_require_a_provider() {
    let f = Fixture::new();
    let root=f.library("weak","weak.so","extern int optional(void) __attribute__((weak)); int root(void) { return optional ? optional() : 0; }",&[],None);
    let info = elf(&root).unwrap();
    assert!(!info.imports.contains("optional"));
    assert_eq!(audit(&[root], &[], 62).unwrap()["compatible"], true);
}
#[test]
fn dependency_cycles_terminate_and_preserve_both_objects() {
    let f = Fixture::new();
    f.library("cycle", "liba.so", "int a(void) { return 7; }", &[], None);
    f.library(
        "cycle",
        "libb.so",
        "extern int a(void); int b(void) { return a(); }",
        &["liba.so"],
        None,
    );
    let root = f.library(
        "cycle",
        "liba.so",
        "extern int b(void); int a(void) { return b(); }",
        &["libb.so"],
        None,
    );
    let report = audit(&[root], &[f.root().join("cycle")], 62).unwrap();
    assert_eq!(report["compatible"], true);
    assert_eq!(report["objects"].as_array().unwrap().len(), 2);
    assert_eq!(report["roots"][0]["closure"].as_array().unwrap().len(), 2);
}
#[test]
fn native_cli_keeps_aarch64_architecture_gate() {
    let f = Fixture::new();
    let root = f.library(
        "cli",
        "owned.so",
        "int owned(void) { return 1; }",
        &[],
        None,
    );
    let output = f.root().join("report.json");
    let result = Command::new(env!("CARGO_BIN_EXE_runtime-elf-audit"))
        .arg("--root")
        .arg(root)
        .arg("--library")
        .arg(f.root().join("cli"))
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("unexpected ELF architecture"));
    assert!(!output.exists());
}
