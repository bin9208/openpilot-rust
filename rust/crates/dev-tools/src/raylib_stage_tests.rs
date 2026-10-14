//! Parked until the coordinated #326 Cargo window; no production pin overrides.
use super::*;
use crate::qa_process::{Captured, CommandSpec, Runner};
use std::{
    collections::BTreeMap,
    io::Write,
    os::unix::fs::{symlink, MetadataExt},
};

fn fixture(root: &Path) -> (Options, Pin) {
    let wheel = root.join("fixture.whl");
    write_wheel(&wheel, &[]);
    let pin = Pin {
        arch: "x86_64".into(),
        url: "https://fixture.invalid/reviewed.whl".into(),
        sha256: crate::seal::digest(&wheel).unwrap(),
        size: fs::metadata(&wheel).unwrap().len(),
        archive: "libraylib_desktop.a".into(),
        machine: 62,
    };
    let options = Options {
        arch: "x86_64".into(),
        output: root.join("output"),
        shared: false,
        cxx: "fixture-cxx".into(),
        wheel_file: Some(wheel),
        library_directories: vec![root.join("gnu libs")],
    };
    (options, pin)
}
fn write_wheel(path: &Path, extra: &[(&str, &[u8])]) {
    let mut wheel = zip::ZipWriter::new(fs::File::create(path).unwrap());
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, bytes) in [
        (
            "root/raylib/install/lib/libraylib_desktop.a",
            b"archive".as_slice(),
        ),
        (
            "root/raylib/install/include/raylib.h",
            b"raylib header + license",
        ),
        (
            "root/raylib/install/include/rlgl.h",
            b"rlgl header + license",
        ),
        (
            "root/raylib/install/include/raymath.h",
            b"raymath header + license",
        ),
    ]
    .into_iter()
    .chain(extra.iter().copied())
    {
        wheel.start_file(name, options).unwrap();
        wheel.write_all(bytes).unwrap();
    }
    wheel.finish().unwrap();
}
#[derive(Default)]
struct Compiler {
    commands: Vec<Vec<String>>,
    fail: bool,
    malformed: bool,
    mutate_existing: Option<PathBuf>,
}
impl Runner for Compiler {
    fn run(&mut self, command: &CommandSpec) -> Result<Captured> {
        self.commands.push(command.argv.clone());
        assert!(command.timeout <= std::time::Duration::from_secs(150));
        if let Some(root) = &self.mutate_existing {
            fs::write(root.join("include/raylib.h"), b"externally changed")?;
        }
        if !self.fail {
            let mut raw = vec![0_u8; 64];
            raw[..6].copy_from_slice(b"\x7fELF\x02\x01");
            raw[16..18].copy_from_slice(&3_u16.to_le_bytes());
            raw[18..20].copy_from_slice(&62_u16.to_le_bytes());
            if self.malformed {
                raw[0] = 0;
            }
            fs::write(command.argv.last().unwrap(), raw)?;
        }
        Ok(Captured {
            code: i32::from(self.fail),
            stdout: Vec::new(),
            stderr: b"compiler fixture".to_vec(),
        })
    }
}
fn snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn visit(base: &Path, current: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(current).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(base, &path, files);
            } else {
                files.insert(
                    path.strip_prefix(base).unwrap().to_owned(),
                    fs::read(path).unwrap(),
                );
            }
        }
    }
    let mut files = BTreeMap::new();
    visit(root, root, &mut files);
    files
}
fn clean(parent: &Path) {
    assert!(!fs::read_dir(parent).unwrap().any(|entry| entry
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".raylib-")));
}
#[test]
fn archive_and_headers_stage_exactly_and_repeat_reuses_generation() {
    let temp = tempfile::tempdir().unwrap();
    let (options, pin) = fixture(temp.path());
    let mut runner = Compiler::default();
    let root = stage_with(&options, &pin, &mut runner).unwrap();
    assert_eq!(root, options.output.canonicalize().unwrap());
    assert_eq!(
        fs::read(root.join("lib/libraylib_desktop.a")).unwrap(),
        b"archive"
    );
    assert_eq!(
        fs::read_to_string(root.join("SOURCE_SHA256")).unwrap(),
        format!("{}\n", pin.sha256)
    );
    let before = snapshot(&root);
    let inode = root.metadata().unwrap().ino();
    stage_with(&options, &pin, &mut runner).unwrap();
    assert_eq!(snapshot(&root), before);
    assert_eq!(root.metadata().unwrap().ino(), inode);
    assert!(runner.commands.is_empty());
    clean(temp.path());
}
#[test]
fn augmentation_is_atomic_and_native_shared_repeats_retain_verified_plugin() {
    let temp = tempfile::tempdir().unwrap();
    let (mut options, pin) = fixture(temp.path());
    let mut runner = Compiler::default();
    stage_with(&options, &pin, &mut runner).unwrap();
    let inode = options.output.metadata().unwrap().ino();
    options.shared = true;
    stage_with(&options, &pin, &mut runner).unwrap();
    assert_ne!(options.output.metadata().unwrap().ino(), inode);
    let before = snapshot(&options.output);
    let inode = options.output.metadata().unwrap().ino();
    let proof: serde_json::Value =
        serde_json::from_slice(&before[Path::new("plugin-build.json")]).unwrap();
    assert_eq!(
        proof["plugin_sha256"],
        crate::seal::digest(&options.output.join("lib/libopenpilot-raylib.so")).unwrap()
    );
    assert_eq!(
        proof["command"],
        serde_json::to_value(&runner.commands[0]).unwrap()
    );
    stage_with(&options, &pin, &mut runner).unwrap();
    options.shared = false;
    stage_with(&options, &pin, &mut runner).unwrap();
    assert_eq!(runner.commands.len(), 1);
    assert_eq!(snapshot(&options.output), before);
    assert_eq!(options.output.metadata().unwrap().ino(), inode);
    clean(temp.path());
}
#[test]
fn failed_or_malformed_compiler_keeps_existing_generation_untouched() {
    for malformed in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let (mut options, pin) = fixture(temp.path());
        stage_with(&options, &pin, &mut Compiler::default()).unwrap();
        let before = snapshot(&options.output);
        let inode = options.output.metadata().unwrap().ino();
        options.shared = true;
        let mut runner = Compiler {
            fail: !malformed,
            malformed,
            ..Default::default()
        };
        assert!(stage_with(&options, &pin, &mut runner).is_err());
        assert_eq!(snapshot(&options.output), before);
        assert_eq!(options.output.metadata().unwrap().ino(), inode);
        clean(temp.path());
    }
}
#[test]
fn existing_conflicts_symlinks_extra_files_and_changed_proof_are_rejected() {
    for kind in 0..5 {
        let temp = tempfile::tempdir().unwrap();
        let (mut options, pin) = fixture(temp.path());
        options.shared = true;
        stage_with(&options, &pin, &mut Compiler::default()).unwrap();
        match kind {
            0 => fs::write(options.output.join("include/raylib.h"), b"changed").unwrap(),
            1 => fs::write(options.output.join("unmanaged"), b"keep").unwrap(),
            2 => fs::write(options.output.join("SOURCE_SHA256"), b"wrong\n").unwrap(),
            3 => fs::write(
                options.output.join("lib/libopenpilot-raylib.so"),
                b"changed",
            )
            .unwrap(),
            _ => {
                fs::remove_file(options.output.join("include/rlgl.h")).unwrap();
                symlink("raylib.h", options.output.join("include/rlgl.h")).unwrap();
            }
        }
        let before = snapshot(&options.output);
        assert!(stage_with(&options, &pin, &mut Compiler::default()).is_err());
        assert_eq!(snapshot(&options.output), before);
        clean(temp.path());
    }
}
#[test]
fn legacy_shared_requires_explicit_relink_before_nonshared_reuse() {
    let temp = tempfile::tempdir().unwrap();
    let (mut options, pin) = fixture(temp.path());
    options.shared = true;
    stage_with(&options, &pin, &mut Compiler::default()).unwrap();
    let path = options.output.join("plugin-build.json");
    let mut proof: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    proof.as_object_mut().unwrap().remove("plugin_sha256");
    fs::write(&path, serde_json::to_vec_pretty(&proof).unwrap()).unwrap();
    let before = snapshot(&options.output);
    options.shared = false;
    assert!(stage_with(&options, &pin, &mut Compiler::default())
        .unwrap_err()
        .to_string()
        .contains("--shared"));
    assert_eq!(snapshot(&options.output), before);
    options.shared = true;
    stage_with(&options, &pin, &mut Compiler::default()).unwrap();
    let proof: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert!(proof.get("plugin_sha256").is_some());
    clean(temp.path());
}
#[test]
fn wheel_identity_and_duplicate_selected_members_fail_without_output() {
    for duplicate in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let (options, mut pin) = fixture(temp.path());
        if duplicate {
            write_wheel(
                options.wheel_file.as_ref().unwrap(),
                &[("other/raylib/install/include/raylib.h", b"duplicate")],
            );
            pin.size = fs::metadata(options.wheel_file.as_ref().unwrap())
                .unwrap()
                .len();
            pin.sha256 = crate::seal::digest(options.wheel_file.as_ref().unwrap()).unwrap();
        } else {
            pin.sha256 = "0".repeat(64);
        }
        assert!(stage_with(&options, &pin, &mut Compiler::default()).is_err());
        assert!(!options.output.exists());
        clean(temp.path());
    }
}

#[test]
fn final_identity_recheck_prevents_overwriting_external_changes_during_link() {
    let temp = tempfile::tempdir().unwrap();
    let (mut options, pin) = fixture(temp.path());
    stage_with(&options, &pin, &mut Compiler::default()).unwrap();
    let mut expected = snapshot(&options.output);
    expected.insert(
        PathBuf::from("include/raylib.h"),
        b"externally changed".to_vec(),
    );
    let inode = options.output.metadata().unwrap().ino();
    options.shared = true;
    let mut runner = Compiler {
        mutate_existing: Some(options.output.clone()),
        ..Default::default()
    };
    assert!(stage_with(&options, &pin, &mut runner).is_err());
    assert_eq!(snapshot(&options.output), expected);
    assert_eq!(options.output.metadata().unwrap().ino(), inode);
    clean(temp.path());
}
#[test]
fn oversized_wheel_and_hostile_zip_metadata_fail_before_publication() {
    for scenario in ["sparse", "entry-count", "missing", "crc"] {
        let temp = tempfile::tempdir().unwrap();
        let (options, mut pin) = fixture(temp.path());
        let path = options.wheel_file.as_ref().unwrap();
        if scenario == "sparse" {
            fs::OpenOptions::new()
                .write(true)
                .open(path)
                .unwrap()
                .set_len(1 << 40)
                .unwrap();
        } else {
            let mut bytes = fs::read(path).unwrap();
            if scenario == "entry-count" {
                let offset = bytes
                    .windows(4)
                    .rposition(|value| value == b"PK\x05\x06")
                    .unwrap();
                bytes[offset + 8..offset + 12].fill(0xff);
            } else if scenario == "missing" {
                let old = b"raymath.h";
                for offset in (0..=bytes.len() - old.len()).rev() {
                    if bytes[offset..offset + old.len()] == *old {
                        bytes[offset + old.len() - 1] = b'x';
                    }
                }
            } else {
                let marker = b"raylib header + license";
                let offset = bytes
                    .windows(marker.len())
                    .position(|value| value == marker)
                    .unwrap();
                bytes[offset] ^= 1;
            }
            fs::write(path, bytes).unwrap();
            pin.sha256 = crate::seal::digest(path).unwrap();
        }
        assert!(
            stage_with(&options, &pin, &mut Compiler::default()).is_err(),
            "{scenario}"
        );
        assert!(!options.output.exists());
        clean(temp.path());
    }
}
#[test]
fn fixed_architecture_pins_match_reviewed_uv_lock_without_python_imports() {
    let lock: toml::Value = include_str!("../../../../uv.lock").parse().unwrap();
    let package = lock["package"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["name"].as_str() == Some("comma-deps-raylib"))
        .unwrap();
    assert_eq!(package["version"].as_str().unwrap(), "6.0.0.1.post103");
    for arch in ["x86_64", "aarch64"] {
        let pin = pin_for(arch).unwrap();
        let wheel = package["wheels"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["url"].as_str() == Some(&pin.url))
            .unwrap();
        assert_eq!(
            wheel["hash"].as_str().unwrap(),
            format!("sha256:{}", pin.sha256)
        );
        assert_eq!(
            wheel["size"].as_integer().unwrap(),
            i64::try_from(pin.size).unwrap()
        );
    }
}

#[test]
fn compiler_flags_and_link_order_match_unchanged_original_for_both_architectures() {
    for (arch, expected) in [
        (
            "x86_64",
            include_str!("../tests/fixtures/raylib/x86_64-compiler-command.json"),
        ),
        (
            "aarch64",
            include_str!("../tests/fixtures/raylib/aarch64-compiler-command.json"),
        ),
    ] {
        let options = Options {
            arch: arch.into(),
            output: PathBuf::from("/output"),
            shared: true,
            cxx: "fixture-cxx".into(),
            wheel_file: None,
            library_directories: vec![PathBuf::from("/fixture/gnu libs")],
        };
        let root = Path::new("/owned generation");
        let command = compiler_command(&options, &pin_for(arch).unwrap(), root).unwrap();
        let normalized: Vec<_> = command
            .argv
            .iter()
            .map(|arg| arg.replace("/owned generation", "$OWNED_ROOT"))
            .collect();
        assert_eq!(
            normalized,
            serde_json::from_str::<Vec<String>>(expected).unwrap()
        );
        assert!(command.timeout <= std::time::Duration::from_secs(150));
    }
    assert_eq!(
        CONTRACT_SOURCE,
        include_str!("../tests/fixtures/raylib/plugin-contract.txt")
    );
}

#[test]
fn download_has_exact_size_deadline_and_uses_owned_temporary_file() {
    struct Download {
        source: PathBuf,
        calls: usize,
    }
    impl Runner for Download {
        fn run(&mut self, command: &CommandSpec) -> Result<Captured> {
            self.calls += 1;
            assert_eq!(command.argv[0], "curl");
            assert!(command.timeout <= std::time::Duration::from_secs(150));
            assert!(command.argv.iter().any(|arg| arg == "--max-filesize"));
            assert!(command.argv.iter().any(|arg| arg == "=https"));
            let index = command
                .argv
                .iter()
                .position(|arg| arg == "--output")
                .unwrap();
            assert_eq!(command.argv[index + 1], "-");
            assert!(command.stdout_limit <= 24 * 1024 * 1024);
            Ok(Captured {
                code: 0,
                stdout: fs::read(&self.source)?,
                stderr: Vec::new(),
            })
        }
    }
    let temp = tempfile::tempdir().unwrap();
    let (mut options, pin) = fixture(temp.path());
    let mut download = Download {
        source: options.wheel_file.take().unwrap(),
        calls: 0,
    };
    stage_with(&options, &pin, &mut download).unwrap();
    assert_eq!(download.calls, 1);
    assert!(options.output.join("SOURCE_SHA256").is_file());
    assert_eq!(snapshot(&options.output).len(), 5);
    clean(temp.path());
}

#[test]
fn unsafe_selected_paths_and_symlink_modes_are_rejected_before_extracting() {
    for symbolic in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let (options, mut pin) = fixture(temp.path());
        let path = options.wheel_file.as_ref().unwrap();
        let mut bytes = fs::read(path).unwrap();
        if symbolic {
            let offset = bytes
                .windows(4)
                .position(|value| value == b"PK\x01\x02")
                .unwrap();
            bytes[offset + 38..offset + 42].copy_from_slice(&(0o120777_u32 << 16).to_le_bytes());
        } else {
            for offset in (0..=bytes.len() - 5).rev() {
                if bytes[offset..offset + 5] == *b"root/" {
                    bytes[offset..offset + 5].copy_from_slice(b"../x/");
                }
            }
        }
        fs::write(path, bytes).unwrap();
        pin.sha256 = crate::seal::digest(path).unwrap();
        assert!(stage_with(&options, &pin, &mut Compiler::default()).is_err());
        assert!(!options.output.exists());
        clean(temp.path());
    }
}

#[test]
fn existing_directory_entry_count_is_bounded_before_indexing() {
    let temp = tempfile::tempdir().unwrap();
    let (options, pin) = fixture(temp.path());
    fs::create_dir(&options.output).unwrap();
    for index in 0..17 {
        fs::write(options.output.join(format!("unmanaged-{index}")), b"kept").unwrap();
    }
    let before = snapshot(&options.output);
    let error = stage_with(&options, &pin, &mut Compiler::default()).unwrap_err();
    assert!(
        error.to_string().contains("directory entry limit"),
        "{error}"
    );
    assert_eq!(snapshot(&options.output), before);
    clean(temp.path());
}
