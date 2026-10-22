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
        if name.ends_with('/') {
            assert!(bytes.is_empty());
            zip.add_directory(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
        } else {
            zip.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(bytes).unwrap();
        }
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
fn directories_are_excluded_and_native_bytes_hashes_licenses_and_provenance_are_preserved() {
    let temp = tempfile::tempdir().unwrap();
    let mut members = MEMBERS.to_vec();
    members.extend([
        ("numpy.libs/nested/", b"".as_slice()),
        ("numpy-2.5.3.dist-info/licenses/", b"".as_slice()),
        ("numpy-2.5.3.dist-info/licenses/nested/", b"".as_slice()),
        ("unselected/", b"".as_slice()),
    ]);
    let (wheel, lock) = wheel(temp.path(), &members);
    let output = temp.path().join("stage");
    stage(&wheel, &output, &lock).unwrap();
    assert!(!output.join("nested").exists());
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(output.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["format"], 1);
    assert_eq!(manifest["numpy"], "2.5.3");
    assert_eq!(manifest["abi"], "scipy_dgesdd_64_");
    assert_eq!(manifest["library"], "libscipy_openblas64_fixture.so");
    assert_eq!(manifest["files"].as_array().unwrap().len(), 2);
    assert!(!output.join("numpy.libs").exists());
    assert_eq!(
        fs::read(output.join("libscipy_openblas64_fixture.so")).unwrap(),
        b"native openblas fixture"
    );
    assert_eq!(
        fs::read(output.join("libgfortran-fixture.so")).unwrap(),
        b"native fortran fixture"
    );
    let provenance: serde_json::Value =
        serde_json::from_slice(&fs::read(output.join("provenance.json")).unwrap()).unwrap();
    assert_eq!(provenance["sha256"], digest(&wheel).unwrap());
    assert_eq!(provenance["wheel"], "numpy-2.5.3-fixture.whl");
    assert_eq!(
        provenance["source"],
        "https://github.com/numpy/numpy/tree/v2.5.3"
    );
    assert_eq!(
        provenance["runtime"],
        "native shared libraries only; no Python imports"
    );
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

// Hosted-only provider check: download the exact ARM wheel from uv.lock first.
#[test]
#[ignore = "requires NUMPY_ARM_WHEEL pointing at the source-locked ARM archive"]
fn pinned_arm_wheel_retains_exactly_two_native_libraries_and_all_license_bytes() {
    use std::io::Read;
    let wheel = PathBuf::from(std::env::var_os("NUMPY_ARM_WHEEL").expect("NUMPY_ARM_WHEEL"));
    assert_eq!(
        digest(&wheel).unwrap(),
        "76c2c1e6bfa5c84adc6434dfbf013aa92096a7985221762c8f11fedfd20fff58"
    );
    let lock = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../uv.lock");
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("stage");
    stage(&wheel, &output, &lock).unwrap();
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(output.join("manifest.json")).unwrap()).unwrap();
    let mut expected_files = Vec::new();
    let mut licenses = 0;
    let mut archive = zip::ZipArchive::new(fs::File::open(&wheel).unwrap()).unwrap();
    for index in 0..archive.len() {
        let mut member = archive.by_index(index).unwrap();
        let name = member.name().unwrap().to_string();
        if member.is_dir() {
            continue;
        }
        let target = if name.starts_with("numpy.libs/") {
            let name = name.rsplit('/').next().unwrap();
            expected_files.push(serde_json::json!({
                "name": name,
                "sha256": digest(&output.join(name)).unwrap(),
            }));
            output.join(name)
        } else if let Some((_, license)) = name.split_once(".dist-info/licenses/") {
            licenses += 1;
            output.join("licenses").join(license)
        } else {
            continue;
        };
        let mut expected = Vec::new();
        member.read_to_end(&mut expected).unwrap();
        assert_eq!(fs::read(target).unwrap(), expected, "{name}");
    }
    assert_eq!(expected_files.len(), 2);
    assert!(licenses > 0);
    assert_eq!(manifest["files"], serde_json::Value::Array(expected_files));
    assert!(!output.join("numpy.libs").exists());
    let provenance: serde_json::Value =
        serde_json::from_slice(&fs::read(output.join("provenance.json")).unwrap()).unwrap();
    assert_eq!(provenance["sha256"], digest(&wheel).unwrap());
    assert_eq!(
        provenance["wheel"],
        wheel.file_name().unwrap().to_str().unwrap()
    );
}
