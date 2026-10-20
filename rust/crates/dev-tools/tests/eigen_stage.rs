#![cfg(target_os = "linux")]
use openpilot_dev_tools::{eigen_stage, seal::digest};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

fn required(name: &str) -> PathBuf {
    PathBuf::from(
        std::env::var_os(name).unwrap_or_else(|| panic!("cloud Eigen gate requires {name}")),
    )
}
fn tree(root: &Path) -> BTreeMap<String, String> {
    fn visit(root: &Path, current: &Path, files: &mut BTreeMap<String, String>) {
        for entry in fs::read_dir(current).unwrap() {
            let path = entry.unwrap().path();
            let metadata = fs::symlink_metadata(&path).unwrap();
            assert!(!metadata.file_type().is_symlink());
            if metadata.is_dir() {
                visit(root, &path, files);
            } else {
                assert!(metadata.is_file());
                files.insert(
                    path.strip_prefix(root)
                        .unwrap()
                        .to_str()
                        .unwrap()
                        .replace('\\', "/"),
                    digest(&path).unwrap(),
                );
            }
        }
    }
    let mut files = BTreeMap::new();
    visit(root, root, &mut files);
    files
}

#[test]
fn pinned_wheel_stages_exact_header_tree_and_never_clobbers() {
    let wheel = required("EIGEN_WHEEL");
    let architecture =
        std::env::var("EIGEN_ARCHITECTURE").expect("cloud Eigen architecture required");
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("native");
    let manifest = eigen_stage::stage(&wheel, &output, &architecture).unwrap();
    assert_eq!(manifest.files.len(), 690);
    assert_eq!(
        manifest.files.iter().map(|file| file.bytes).sum::<u64>(),
        10_130_800
    );
    assert_eq!(manifest.version, "3.4.0");
    let installed = tree(&output.join("install"));
    assert_eq!(installed.len(), 690);
    assert!(installed.keys().all(|path| !path.ends_with(".py")));
    let expected = manifest
        .files
        .iter()
        .map(|file| (file.path.clone(), file.sha256.clone()))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(installed, expected);
    let saved = fs::read(output.join("manifest.json")).unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&saved).unwrap()["files"]
            .as_array()
            .unwrap()
            .len(),
        690
    );
    assert!(eigen_stage::stage(&wheel, &output, &architecture).is_err());
    assert_eq!(fs::read(output.join("manifest.json")).unwrap(), saved);

    let altered = temp.path().join(wheel.file_name().unwrap());
    let mut bytes = fs::read(&wheel).unwrap();
    bytes[0] ^= 1;
    fs::write(&altered, bytes).unwrap();
    let rejected = temp.path().join("rejected");
    assert!(eigen_stage::stage(&altered, &rejected, &architecture).is_err());
    assert!(!rejected.exists());
    assert!(eigen_stage::stage(&wheel, &rejected, "wrong-architecture").is_err());

    let link_dir = temp.path().join("linked");
    fs::create_dir(&link_dir).unwrap();
    let linked_wheel = link_dir.join(wheel.file_name().unwrap());
    std::os::unix::fs::symlink(&wheel, &linked_wheel).unwrap();
    assert!(eigen_stage::stage(&linked_wheel, &rejected, &architecture).is_err());
    assert!(!rejected.exists());
}

#[test]
fn fresh_original_python_shim_install_matches_native_every_header_byte() {
    let wheel = required("EIGEN_WHEEL");
    let architecture =
        std::env::var("EIGEN_ARCHITECTURE").expect("cloud Eigen architecture required");
    let original = required("EIGEN_ORIGINAL_INCLUDE");
    let native = required("EIGEN_NATIVE_INCLUDE");
    assert!(original.is_absolute() && native.is_absolute());
    assert_ne!(
        fs::canonicalize(&original).unwrap(),
        fs::canonicalize(&native).unwrap()
    );
    assert_eq!(tree(&original).len(), 690);
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("independent-native");
    eigen_stage::stage(&wheel, &output, &architecture).unwrap();
    assert_ne!(
        fs::canonicalize(&original).unwrap(),
        fs::canonicalize(output.join("install")).unwrap()
    );
    let generated = tree(&output.join("install"));
    let original_files = tree(&original);
    let native_files = tree(&native);
    assert_eq!(generated, original_files);
    assert_eq!(generated, native_files);
    let evidence = required("EIGEN_EVIDENCE");
    fs::create_dir(&evidence).unwrap();
    for (name, files) in [("original", original_files), ("native", native_files)] {
        fs::write(
            evidence.join(format!("{name}-files.json")),
            serde_json::to_vec_pretty(&files).unwrap(),
        )
        .unwrap();
    }
    fs::write(
        evidence.join("comparison.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "architecture": architecture,
            "wheel_sha256": digest(&wheel).unwrap(),
            "original_include": original,
            "native_include": native,
            "files": generated.len(),
            "all_sha256_equal": true
        }))
        .unwrap(),
    )
    .unwrap();
}
