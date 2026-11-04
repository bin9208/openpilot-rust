use openpilot_dev_tools::{
    elf_audit::{audit, audit_with_load_owner, elf},
    package::disk_guard,
};
use std::{
    collections::BTreeSet,
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
        self.library_with_soname(dir, name, source, links, version, name)
    }
    fn library_with_soname(
        &self,
        dir: &str,
        name: &str,
        source: &str,
        links: &[&str],
        version: Option<&str>,
        soname: &str,
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
            .arg(format!("-Wl,-soname,{soname}"));
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
fn real_readelf_origin_paths_keep_object_local_versions_and_strong_symbols() {
    for token in ["$ORIGIN", "${ORIGIN}"] {
        let f = Fixture::new();
        let needed = format!("{token}/../lib/libprovider.so");
        let mut providers = Vec::new();
        let mut roots = Vec::new();
        for (dir, version) in [("first/lib", "FIRST_1"), ("second/lib", "SECOND_1")] {
            providers.push(f.library_with_soname(
                dir,
                "libprovider.so",
                "int supplied(void) { return 7; }",
                &[],
                Some(version),
                &needed,
            ));
            let root = f.consumer(dir);
            let info = elf(&root).unwrap();
            assert_eq!(info.needed.as_slice(), std::slice::from_ref(&needed));
            assert_eq!(info.needs[&needed], [version]);
            roots.push(root);
        }
        let libraries = [f.root().join("second/lib"), f.root().join("first/lib")];
        let report = audit(&roots, &libraries, 62).unwrap();
        assert_eq!(report["compatible"], true);
        for i in 0..2 {
            let closure = report["roots"][i]["closure"].as_array().unwrap();
            assert_eq!(closure.len(), 2);
            assert!(closure.contains(&serde_json::json!(providers[i].canonicalize().unwrap())));
            assert!(!closure.contains(&serde_json::json!(providers[1 - i].canonicalize().unwrap())));
        }
        f.library_with_soname(
            "first/lib",
            "libprovider.so",
            "int supplied(void) { return 7; }",
            &[],
            Some("WRONG_1"),
            &needed,
        );
        let report = audit(&roots, &libraries, 62).unwrap();
        assert_eq!(report["roots"][0]["compatible"], false);
        assert_eq!(report["roots"][1]["compatible"], true);
        assert_eq!(
            report["roots"][0]["failures"][0]["missing_versions"][needed.as_str()][0],
            "FIRST_1"
        );
        f.library_with_soname(
            "first/lib",
            "libprovider.so",
            "int other(void) { return 7; }",
            &[],
            Some("FIRST_1"),
            &needed,
        );
        let report = audit(&roots, &libraries, 62).unwrap();
        assert_eq!(report["roots"][1]["compatible"], true);
        let failure = &report["roots"][0]["failures"][0];
        assert_eq!(failure["missing_versions"], serde_json::json!({}));
        assert_eq!(
            failure["missing_strong_symbols"],
            serde_json::json!(["supplied@FIRST_1"])
        );
        let mut bytes = fs::read(&providers[0]).unwrap();
        bytes[18..20].copy_from_slice(&183u16.to_le_bytes());
        fs::write(&providers[0], bytes).unwrap();
        assert!(audit(&roots, &libraries, 62)
            .unwrap_err()
            .to_string()
            .contains("unexpected ELF architecture"));
    }
}
#[test]
fn origin_paths_do_not_search_other_folders_or_escape_the_explicit_boundary() {
    let f = Fixture::new();
    let needed = "$ORIGIN/libprovider.so";
    let provider = f.library_with_soname(
        "owned",
        "libprovider.so",
        "int supplied(void) { return 7; }",
        &[],
        Some("PROVIDER_1"),
        needed,
    );
    let root = f.consumer("owned");
    let other = f.provider("other", "PROVIDER_1", "supplied");
    assert!(
        audit(std::slice::from_ref(&root), &[f.root().join("other")], 62)
            .unwrap_err()
            .to_string()
            .contains("outside explicit library directories")
    );
    fs::remove_file(&provider).unwrap();
    let libraries = [f.root().join("other"), f.root().join("owned")];
    assert!(audit(std::slice::from_ref(&root), &libraries, 62)
        .unwrap_err()
        .to_string()
        .contains("missing provider $ORIGIN/libprovider.so"));
    std::os::unix::fs::symlink(other, provider).unwrap();
    assert!(
        audit(std::slice::from_ref(&root), &[f.root().join("owned")], 62)
            .unwrap_err()
            .to_string()
            .contains("outside explicit library directories")
    );
    assert_eq!(audit(&[root], &libraries, 62).unwrap()["compatible"], true);
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
fn explicit_load_owner_exposes_only_its_verified_startup_closure_to_plugins() {
    let f = Fixture::new();
    let dependency = f.provider("startup", "OWNER_1", "supplied");
    let owner = f.library(
        "startup",
        "owner.so",
        "extern int supplied(void); int owner_export(void) { return supplied(); }",
        &["libprovider.so"],
        None,
    );
    let roots: Vec<_> = ["first.so", "second.so", "third.so"]
        .into_iter()
        .map(|name| {
            f.library(
                "plugins",
                name,
                "extern int owner_export(void); extern int supplied(void); int plugin(void) { return owner_export()+supplied(); }",
                &[],
                None,
            )
        })
        .collect();
    let libraries = [f.root().join("startup")];
    let independent_roots: Vec<_> = std::iter::once(owner.clone())
        .chain(roots.iter().cloned())
        .collect();
    let independent = audit(&independent_roots, &libraries, 62).unwrap();
    assert!(independent.get("load_owner").is_none());
    assert_eq!(independent["roots"][0]["compatible"], true);
    assert!(independent["roots"].as_array().unwrap()[1..]
        .iter()
        .all(|row| row["compatible"] == false));
    let report = audit_with_load_owner(&roots, &libraries, 62, &owner).unwrap();
    assert_eq!(report["compatible"], true);
    assert_eq!(
        report["load_owner"],
        serde_json::json!(owner.canonicalize().unwrap())
    );
    assert_eq!(report["roots"][0]["root"], report["load_owner"]);
    let rows = report["roots"].as_array().unwrap();
    assert_eq!(rows.len(), 4);
    let actual: BTreeSet<_> = rows
        .iter()
        .map(|row| PathBuf::from(row["root"].as_str().unwrap()))
        .collect();
    let expected: BTreeSet<_> = independent_roots
        .iter()
        .map(|p| p.canonicalize().unwrap())
        .collect();
    assert_eq!(actual, expected);
    assert_eq!(report["roots"][0]["closure"].as_array().unwrap().len(), 2);
    for row in &rows[1..] {
        let closure = row["closure"].as_array().unwrap();
        assert_eq!(closure.len(), 3);
        assert!(closure.contains(&report["load_owner"]));
        assert!(closure.contains(&serde_json::json!(dependency.canonicalize().unwrap())));
    }
    f.provider("startup", "WRONG_1", "supplied");
    let report = audit_with_load_owner(&roots, &libraries, 62, &owner).unwrap();
    assert_eq!(report["compatible"], false);
    assert_eq!(
        report["roots"][0]["failures"][0]["missing_versions"]["libprovider.so"][0],
        "OWNER_1"
    );
}
#[test]
fn explicit_load_owner_does_not_merge_unrelated_plugin_exports() {
    let f = Fixture::new();
    let owner = f.library(
        "scope",
        "owner.so",
        "int owned(void) { return 1; }",
        &[],
        None,
    );
    let plugin = f.library(
        "scope",
        "plugin.so",
        "extern int foreign_only(void); int plugin(void) { return foreign_only(); }",
        &[],
        None,
    );
    let foreign = f.library(
        "scope",
        "foreign.so",
        "int foreign_only(void) { return 2; }",
        &[],
        None,
    );
    let report = audit_with_load_owner(&[plugin, foreign], &[], 62, &owner).unwrap();
    assert_eq!(report["compatible"], false);
    assert_eq!(report["roots"][0]["compatible"], true);
    assert_eq!(report["roots"][2]["compatible"], true);
    assert_eq!(
        report["roots"][1]["failures"][0]["missing_strong_symbols"],
        serde_json::json!(["foreign_only"])
    );
}
#[test]
fn plugins_cannot_rescue_a_bad_owner_and_the_cli_checks_owner_architecture() {
    let f = Fixture::new();
    let owner = f.library(
        "scope",
        "owner.so",
        "extern int plugin_only(void); int owned(void) { return plugin_only(); }",
        &[],
        None,
    );
    let plugin = f.library(
        "scope",
        "plugin.so",
        "int plugin_only(void) { return 2; }",
        &[],
        None,
    );
    let report = audit_with_load_owner(std::slice::from_ref(&plugin), &[], 62, &owner).unwrap();
    assert_eq!(report["compatible"], false);
    assert_eq!(report["roots"][0]["compatible"], false);
    assert_eq!(
        report["roots"][0]["failures"][0]["missing_strong_symbols"],
        serde_json::json!(["plugin_only"])
    );
    assert_eq!(report["roots"][0]["closure"].as_array().unwrap().len(), 1);
    let output = f.root().join("owner-report.json");
    let result = Command::new(env!("CARGO_BIN_EXE_runtime-elf-audit"))
        .arg("--load-owner")
        .arg(&owner)
        .arg("--root")
        .arg(plugin)
        .arg("--library")
        .arg(f.root().join("scope"))
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert!(!result.status.success());
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(stderr.contains("unexpected ELF architecture"), "{stderr}");
    assert!(stderr.contains(owner.to_str().unwrap()), "{stderr}");
    assert!(!output.exists());
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
