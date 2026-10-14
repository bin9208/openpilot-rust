use openpilot_dev_tools::{
    seal::digest,
    usbgpu_stage::{stage, Bundle},
};
use serde_json::{json, Value};
use std::{
    fs,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
};
const FIRMWARE: [&str; 8] = [
    "psp_14_0_2_sos.bin",
    "gc_12_0_0_pfp.bin",
    "gc_12_0_0_me.bin",
    "gc_12_0_0_mec.bin",
    "gc_12_0_0_rlc.bin",
    "gc_12_0_0_imu.bin",
    "sdma_7_0_0.bin",
    "smu_14_0_2.bin",
];
fn put(root: &Path, name: &str, bytes: &[u8]) {
    let path = root.join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}
struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    bundle: Bundle,
    manifest: Value,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root");
        let package = temp.path().join("companions");
        let selected = "a".repeat(64);
        let mut names = vec![
            "tinygrad-LICENSE".to_owned(),
            "probe-gfx1200.json".into(),
            "source-provenance.json".into(),
            format!("models/{selected}/model.hcq.json"),
            format!("models/{selected}/model.hcq-meta.json"),
        ];
        let mut hashes = serde_json::Map::new();
        for name in FIRMWARE {
            names.push(format!("firmware/{name}"));
            put(&package, &format!("firmware/{name}"), name.as_bytes());
            hashes.insert(
                name.into(),
                json!(digest(&package.join(format!("firmware/{name}"))).unwrap()),
            );
        }
        for camera in ["1344x760", "1928x1208"] {
            names.push(format!("warp-gfx1200-{camera}.json"));
            for name in [
                "graph.json",
                "weights.bin",
                "provenance.json",
                "kernel-0.bin",
            ] {
                names.push(format!("warp-qcom-a630-{camera}/{name}"));
            }
        }
        for name in &names {
            if !package.join(name).exists() {
                let bytes: &[u8] = if name.ends_with("/graph.json") {
                    br#"{"kernels":[{}]}"#
                } else {
                    b"owned fixture companion"
                };
                put(&package, name, bytes);
            }
        }
        put(
            &root,
            "rust/crates/usbgpu/assets/amd-metadata.json",
            &serde_json::to_vec(&json!({"firmware_hashes":hashes})).unwrap(),
        );
        let license = temp.path().join("LICENSE.amdgpu");
        fs::write(&license, "owned firmware license").unwrap();
        let files = names.iter().map(|name| json!({"path":name,"bytes":fs::metadata(package.join(name)).unwrap().len(),"sha256":digest(&package.join(name)).unwrap()})).collect::<Vec<_>>();
        let manifest = json!({"version":1,"model_sha256":selected,"files":files});
        let mut fixture = Self {
            _temp: temp,
            root,
            bundle: Bundle {
                path: package,
                manifest_sha256: String::new(),
                firmware_license: license,
                model: None,
            },
            manifest,
        };
        fixture.reseal();
        fixture
    }
    fn reseal(&mut self) {
        put(
            &self.bundle.path,
            "manifest.json",
            &serde_json::to_vec(&self.manifest).unwrap(),
        );
        self.bundle.manifest_sha256 = digest(&self.bundle.path.join("manifest.json")).unwrap();
    }
    fn target(&self) -> PathBuf {
        self.root.join("rust/bin/usbgpu-assets")
    }
    fn rejected(&self, expected: &str) {
        let error = stage(&self.root, &self.bundle).unwrap_err().to_string();
        assert!(error.contains(expected), "{error}");
    }
}
#[test]
fn stages_verified_files_licenses_and_repeats_without_replacing_them() {
    let fixture = Fixture::new();
    let summary = stage(&fixture.root, &fixture.bundle).unwrap();
    assert_eq!(summary["manifest_sha256"], fixture.bundle.manifest_sha256);
    assert_eq!(
        summary["files"],
        fixture.manifest["files"].as_array().unwrap().len() + 4
    );
    for entry in fixture.manifest["files"].as_array().unwrap() {
        assert_eq!(
            digest(&fixture.target().join(entry["path"].as_str().unwrap())).unwrap(),
            entry["sha256"]
        );
    }
    for (source, target) in [
        (fixture.bundle.firmware_license.clone(), "LICENSE.amdgpu"),
        (
            fixture.bundle.path.join("tinygrad-LICENSE"),
            "tinygrad-LICENSE",
        ),
        (
            fixture.bundle.path.join("source-provenance.json"),
            "build-provenance.json",
        ),
    ] {
        assert_eq!(
            fs::read(source).unwrap(),
            fs::read(fixture.root.join("rust/bin/licenses/usbgpu").join(target)).unwrap()
        );
    }
    let old = fs::File::open(fixture.target().join("manifest.json")).unwrap();
    assert_eq!(stage(&fixture.root, &fixture.bundle).unwrap(), summary);
    use std::os::unix::fs::MetadataExt;
    assert_eq!(
        old.metadata().unwrap().ino(),
        fs::metadata(fixture.target().join("manifest.json"))
            .unwrap()
            .ino()
    );
    assert!(!fixture.target().join("native-provider.json").exists());
    assert!(!fixture.target().join("installed.json").exists());
}
#[test]
fn manifest_identity_version_model_identity_and_duplicate_entries_fail_before_copy() {
    for case in [
        "identity",
        "version",
        "model",
        "duplicate",
        "size",
        "checksum",
        "count",
    ] {
        let mut fixture = Fixture::new();
        match case {
            "identity" => fixture.bundle.manifest_sha256 = "f".repeat(64),
            "version" => fixture.manifest["version"] = json!(2),
            "model" => fixture.manifest["model_sha256"] = json!("A".repeat(64)),
            "duplicate" => {
                let item = fixture.manifest["files"][0].clone();
                fixture.manifest["files"].as_array_mut().unwrap().push(item);
            }
            "size" => fixture.manifest["files"][0]["bytes"] = json!(999),
            "checksum" => fixture.manifest["files"][0]["sha256"] = json!("f".repeat(64)),
            "count" => {
                fixture.manifest["files"] = json!(vec![fixture.manifest["files"][0].clone(); 513])
            }
            _ => unreachable!(),
        }
        if case != "identity" {
            fixture.reseal();
        }
        if case == "count" {
            fixture.rejected("unsupported native companion manifest");
        } else {
            assert!(stage(&fixture.root, &fixture.bundle).is_err(), "{case}");
        }
        assert!(!fixture.target().exists(), "{case}");
    }
}
#[test]
fn missing_camera_kernel_firmware_pin_and_modified_companion_are_rejected() {
    for case in ["required", "kernel", "firmware", "changed"] {
        let mut fixture = Fixture::new();
        match case {
            "required" => {
                fixture.manifest["files"]
                    .as_array_mut()
                    .unwrap()
                    .retain(|entry| entry["path"] != "probe-gfx1200.json");
                fixture.reseal();
            }
            "kernel" => {
                fixture.manifest["files"]
                    .as_array_mut()
                    .unwrap()
                    .retain(|entry| entry["path"] != "warp-qcom-a630-1344x760/kernel-0.bin");
                fixture.reseal();
            }
            "firmware" => put(
                &fixture.root,
                "rust/crates/usbgpu/assets/amd-metadata.json",
                br#"{"firmware_hashes":{}}"#,
            ),
            "changed" => put(&fixture.bundle.path, "probe-gfx1200.json", b"mutated"),
            _ => unreachable!(),
        }
        assert!(stage(&fixture.root, &fixture.bundle).is_err(), "{case}");
        assert!(!fixture.target().exists());
    }
}
#[test]
fn unsafe_paths_and_source_or_destination_symlinks_are_rejected() {
    for case in ["../outside", "/outside", "", "source", "parent", "target"] {
        let mut fixture = Fixture::new();
        match case {
            "source" => {
                fs::remove_file(fixture.bundle.path.join("probe-gfx1200.json")).unwrap();
                symlink(
                    fixture.bundle.path.join("tinygrad-LICENSE"),
                    fixture.bundle.path.join("probe-gfx1200.json"),
                )
                .unwrap();
            }
            "parent" => {
                fs::create_dir_all(fixture.root.join("rust/bin")).unwrap();
                symlink(&fixture.bundle.path, fixture.target()).unwrap();
            }
            "target" => {
                fs::create_dir_all(fixture.target()).unwrap();
                symlink(
                    fixture.bundle.path.join("manifest.json"),
                    fixture.target().join("manifest.json"),
                )
                .unwrap();
            }
            path => {
                fixture.manifest["files"][0]["path"] = json!(path);
                fixture.reseal();
            }
        }
        assert!(stage(&fixture.root, &fixture.bundle).is_err(), "{case}");
    }
}
#[test]
fn destination_conflicts_and_unpinned_model_do_not_publish_companions() {
    let fixture = Fixture::new();
    put(&fixture.target(), "manifest.json", b"conflict");
    fixture.rejected("conflicting");
    assert!(!fixture.target().join("probe-gfx1200.json").exists());
    let mut fixture = Fixture::new();
    let model = fixture.bundle.path.join("wrong.pkl");
    fs::write(&model, b"wrong model").unwrap();
    fixture.bundle.model = Some(model);
    fixture.rejected("source-pinned");
    assert!(!fixture.target().exists());
}

#[test]
fn oversized_companions_are_rejected_before_staging() {
    let mut fixture = Fixture::new();
    let name = fixture.manifest["files"][0]["path"]
        .as_str()
        .unwrap()
        .to_owned();
    fs::OpenOptions::new()
        .write(true)
        .open(fixture.bundle.path.join(&name))
        .unwrap()
        .set_len((128 << 20) + 1)
        .unwrap();
    fixture.manifest["files"][0]["bytes"] = json!((128u64 << 20) + 1);
    fixture.manifest["files"][0]["sha256"] =
        json!(digest(&fixture.bundle.path.join(&name)).unwrap());
    fixture.reseal();
    fixture.rejected("oversized");
    assert!(!fixture.target().exists());
}

#[test]
fn native_cli_stages_without_python_or_any_executable_search_path() {
    let fixture = Fixture::new();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_stage-runtime-usbgpu"))
        .env_clear()
        .env("PATH", "")
        .arg("--root")
        .arg(&fixture.root)
        .arg("--package")
        .arg(&fixture.bundle.path)
        .arg("--expect-manifest-sha256")
        .arg(&fixture.bundle.manifest_sha256)
        .arg("--firmware-license")
        .arg(&fixture.bundle.firmware_license)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let summary: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(summary["manifest_sha256"], fixture.bundle.manifest_sha256);
    let invalid = std::process::Command::new(env!("CARGO_BIN_EXE_stage-runtime-usbgpu"))
        .arg("--root")
        .output()
        .unwrap();
    assert!(!invalid.status.success());
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("missing argument"));
}

#[test]
fn existing_license_parent_file_rejects_before_publishing_any_assets() {
    for parent in ["rust/bin/licenses", "rust/bin/licenses/usbgpu"] {
        let fixture = Fixture::new();
        put(&fixture.root, parent, b"existing unrelated file");
        assert!(stage(&fixture.root, &fixture.bundle).is_err());
        assert!(
            !fixture.target().exists(),
            "license-parent conflict at {parent} must not publish any USB assets"
        );
        assert_eq!(
            fs::read(fixture.root.join(parent)).unwrap(),
            b"existing unrelated file"
        );
    }
}
