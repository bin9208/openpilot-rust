use openpilot_dev_tools::{seal::digest, torque_stage::stage};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

fn wheel(root: &Path, members: &[(&str, &[u8])]) -> (PathBuf, PathBuf) {
    let wheel = root.join("numpy-2.5.3-fixture.whl");
    let mut zip = zip::ZipWriter::new(fs::File::create(&wheel).unwrap());
    for (name, bytes) in members {
        zip.start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap();
    let lock = root.join("uv.lock");
    fs::write(&lock, format!("[[package]]\nname='numpy'\nversion='2.5.3'\nwheels=[{{url='https://example.test/numpy-2.5.3-fixture.whl',hash='sha256:{}'}}]\n",digest(&wheel).unwrap())).unwrap();
    (wheel, lock)
}
const MEMBERS: &[(&str, &[u8])] = &[
    ("numpy/__init__.py", b"must not be staged"),
    ("numpy.libs/", b""),
    (
        "numpy.libs/libscipy_openblas64_fixture.so",
        b"native openblas fixture",
    ),
    (
        "numpy.libs/libgfortran-fixture.so",
        b"native fortran fixture",
    ),
    (
        "numpy-2.5.3.dist-info/licenses/LICENSE.txt",
        b"original numpy license",
    ),
    (
        "numpy-2.5.3.dist-info/licenses/nested/NOTICE",
        b"native provider notices",
    ),
];

#[test]
fn exact_native_library_manifest_and_nested_licenses_match_original_contract() {
    let temp = tempfile::tempdir().unwrap();
    let (wheel, lock) = wheel(temp.path(), MEMBERS);
    let output = temp.path().join("stage");
    stage(&wheel, &output, &lock).unwrap();
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(output.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["format"], 1);
    assert_eq!(manifest["numpy"], "2.5.3");
    assert_eq!(manifest["abi"], "scipy_dgesdd_64_");
    assert_eq!(manifest["library"], "libscipy_openblas64_fixture.so");
    assert_eq!(manifest["files"].as_array().unwrap().len(), 3);
    assert_eq!(fs::read(output.join("numpy.libs")).unwrap(), b"");
    for file in manifest["files"].as_array().unwrap() {
        assert_eq!(
            file["sha256"],
            digest(&output.join(file["name"].as_str().unwrap())).unwrap()
        );
    }
    assert_eq!(
        fs::read(output.join("licenses/nested/NOTICE")).unwrap(),
        b"native provider notices"
    );
    assert!(!output.join("numpy").exists());
    assert!(stage(&wheel, &output, &lock).is_err());
}

#[test]
fn changed_archive_version_filename_or_duplicate_lock_package_never_publish() {
    for change in [
        "archive",
        "version",
        "filename",
        "duplicate",
        "size",
        "wheel-duplicate",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let (wheel, lock) = wheel(temp.path(), MEMBERS);
        let original = fs::read_to_string(&lock).unwrap();
        match change {
            "archive" => fs::write(&wheel, b"changed wheel").unwrap(),
            "version" => fs::write(
                &lock,
                original.replace("version='2.5.3'", "version='2.5.4'"),
            )
            .unwrap(),
            "filename" => fs::write(&lock, original.replace("fixture.whl", "other.whl")).unwrap(),
            "size" => fs::write(&lock, original.replace("wheels=[{", "wheels=[{size=1,")).unwrap(),
            "wheel-duplicate" => {
                let mut data: toml::Value = original.parse().unwrap();
                let wheels = data["package"][0]["wheels"].as_array_mut().unwrap();
                wheels.push(wheels[0].clone());
                fs::write(&lock, toml::to_string(&data).unwrap()).unwrap();
            }
            _ => fs::write(&lock, format!("{original}{original}")).unwrap(),
        }
        let output = temp.path().join("stage");
        assert!(stage(&wheel, &output, &lock).is_err(), "accepted {change}");
        assert!(!output.exists());
    }
}

#[test]
fn symlink_archive_members_and_output_symlinks_are_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let (wheel, lock) = wheel(temp.path(), MEMBERS);
    let old = digest(&wheel).unwrap();
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&wheel)
        .unwrap();
    let mut zip = zip::ZipWriter::new_append(file).unwrap();
    zip.add_symlink(
        "numpy.libs/external",
        "/etc/passwd",
        zip::write::SimpleFileOptions::default(),
    )
    .unwrap();
    zip.finish().unwrap();
    fs::write(
        &lock,
        fs::read_to_string(&lock)
            .unwrap()
            .replace(&old, &digest(&wheel).unwrap()),
    )
    .unwrap();
    let output = temp.path().join("stage");
    assert!(stage(&wheel, &output, &lock).is_err());
    assert!(!output.exists());
    std::os::unix::fs::symlink(temp.path().join("absent"), &output).unwrap();
    assert!(stage(&wheel, &output, &lock).is_err());
    assert!(output.is_symlink());
}

#[test]
fn cli_stages_without_interpreter_or_source_working_directory() {
    let temp = tempfile::tempdir().unwrap();
    let (wheel, lock) = wheel(temp.path(), MEMBERS);
    let output = temp.path().join("stage");
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_stage-torque-numerics"))
        .current_dir(temp.path())
        .arg("--wheel")
        .arg(&wheel)
        .arg("--output")
        .arg(&output)
        .arg("--lock")
        .arg(&lock)
        .env("PATH", "")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let stdout: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    let provenance: serde_json::Value =
        serde_json::from_slice(&fs::read(output.join("provenance.json")).unwrap()).unwrap();
    assert_eq!(stdout, provenance);
}

#[test]
fn unsafe_paths_flattening_collisions_and_missing_or_multiple_openblas_fail_atomically() {
    for (extra, remove_library) in [
        ("numpy.libs/../escaped", false),
        ("numpy-2.5.3.dist-info/licenses/../../escaped", false),
        ("numpy.libs/nested/libgfortran-fixture.so", false),
        ("numpy.libs/libscipy_openblas64_second.so", false),
        ("numpy.libs/dependency.so", true),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let mut members = MEMBERS.to_vec();
        if remove_library {
            members.retain(|(name, _)| !name.contains("libscipy_openblas"));
        }
        members.push((extra, b"extra"));
        let (wheel, lock) = wheel(temp.path(), &members);
        let output = temp.path().join("stage");
        assert!(stage(&wheel, &output, &lock).is_err(), "accepted {extra}");
        assert!(!output.exists());
        assert!(!temp.path().join("escaped").exists());
    }
}
