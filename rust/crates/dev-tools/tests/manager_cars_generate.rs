#![cfg(target_os = "linux")]

use openpilot_dev_tools::{
    manager_cars_generate::BRANDS,
    qa_process::{CommandSpec, NativeRunner, Runner},
};
use sha2::{Digest, Sha256};
use std::{
    ffi::OsString,
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    os::unix::{
        ffi::OsStrExt,
        fs::{symlink, MetadataExt},
    },
    path::{Path, PathBuf},
    time::Duration,
};

// Original-only bytes from run 39739972934, artifact 12408992459, checked-out
// source 94758a721355264319d370eec8fac894d2ff045c. The seven native and
// installed files were independently compared with these original exports.
// Artifact ZIP SHA-256: ef424c1d4bf53973f91645c532167cb4c4a082632cfa8636a0122e51f92c28ad.
const ORIGINAL_BYTES: [(&str, &str); 7] = [
    (
        "hyundai",
        "74aa2a26b4a79538ebb570739ff68b19ab639af91fe0ee8a7acf065254d1ab05",
    ),
    (
        "gm",
        "761280717a2bd88b98d19468b29e16f2243076276cebf6e88986c233311c5b95",
    ),
    (
        "toyota",
        "3e3e4d4f34a3a2d449f30002a978dfb2c2354ce4a216144bf702923ebd948429",
    ),
    (
        "mazda",
        "2a077c3e3dc380b59a85acbeb5bb2082efc860cfab79f403fc6d95b831720bdb",
    ),
    (
        "ford",
        "b435fced262e55234a7f438b042d1ca15e4afe86cdc0137717320da49f6be3fd",
    ),
    (
        "volkswagen",
        "56ab9c05ccf1de1575294743275803cc563d33d6415de0e212b0c04169c18984",
    ),
    (
        "tesla",
        "a0fa227252ecef1cfff4de4c8e006a6ef6388942740e26c96d14c6b6238834c4",
    ),
];
const ORIGINAL_STDOUT: &[u8] = b"hyundai: 138 source car documentation names\ngm: 34 source car documentation names\ntoyota: 66 source car documentation names\nmazda: 6 source car documentation names\nford: 27 source car documentation names\nvolkswagen: 65 source car documentation names\ntesla: 5 source car documentation names\n";
const ORIGINAL_SOURCES: [(&str, &str); 9] = [
    (
        "opendbc/car/__init__.py",
        "f56a23742d37395f5869f9c63a367375e038e2b47966efaed7a9679307c01e5e",
    ),
    (
        "opendbc/car/docs_definitions.py",
        "00801f31f5a245a27be1bb38865cad2270468f2a226bbba36ddb0a60ec4100ac",
    ),
    (
        "opendbc/car/hyundai/values.py",
        "53a8c9f184653d1dc48c3fcd1b05ac189323d54eaeb41de8302290faec63bfa8",
    ),
    (
        "opendbc/car/gm/values.py",
        "228d20a54736a99113d1968e121d1a6951ab0a7a9686a7e7ecac052a68b2a7bf",
    ),
    (
        "opendbc/car/toyota/values.py",
        "dbe9030ee5509f202989d7749d77cc62fdcd20ab07a55391b0a516d154145070",
    ),
    (
        "opendbc/car/mazda/values.py",
        "6eb75330447d850d76604916a1c213e3b95199012b1c4fbf037e3bf07cb0390a",
    ),
    (
        "opendbc/car/ford/values.py",
        "816861eb35d11d0ccd154dc1ed0b42cc76aebde49848c8cf06494039fbca38e9",
    ),
    (
        "opendbc/car/volkswagen/values.py",
        "ee5a875a5c7ef292710c1d9017d557cd6e9da2d5b08056bfdf9cc226f7951dea",
    ),
    (
        "opendbc/car/tesla/values.py",
        "6027321c0be5847494f7062f0b2be0a09595d5b94c4d482a01f176545fde7bb4",
    ),
];

type Identity = (u64, u64, u64, u32, i64, i64, i64, i64);

fn identity(meta: &fs::Metadata) -> Identity {
    (
        meta.dev(),
        meta.ino(),
        meta.len(),
        meta.mode(),
        meta.mtime(),
        meta.mtime_nsec(),
        meta.ctime(),
        meta.ctime_nsec(),
    )
}

struct HeldInput {
    path: PathBuf,
    file: File,
    identity: Identity,
    digest: String,
}

impl HeldInput {
    fn open(path: PathBuf, elf: bool) -> Self {
        let before = fs::symlink_metadata(&path).unwrap();
        assert!(before.is_file() && !before.file_type().is_symlink());
        if elf {
            assert_ne!(before.mode() & 0o111, 0, "ELF is not executable");
        }
        assert!(
            before.len()
                <= if elf {
                    512 * 1024 * 1024
                } else {
                    2 * 1024 * 1024
                }
        );
        let file = File::from(
            rustix::fs::open(
                &path,
                rustix::fs::OFlags::RDONLY
                    | rustix::fs::OFlags::NOFOLLOW
                    | rustix::fs::OFlags::CLOEXEC,
                rustix::fs::Mode::empty(),
            )
            .unwrap(),
        );
        assert_eq!(identity(&before), identity(&file.metadata().unwrap()));
        if elf {
            let mut header = [0; 20];
            file.try_clone().unwrap().read_exact(&mut header).unwrap();
            assert_eq!(&header[..4], b"\x7fELF");
            assert_eq!(header[4], 2);
            assert_eq!(header[5], 1, "ELF is not little endian");
            let machine = match std::env::consts::ARCH {
                "x86_64" => 62,
                "aarch64" => 183,
                arch => panic!("unsupported manager generator host {arch}"),
            };
            assert_eq!(u16::from_le_bytes([header[18], header[19]]), machine);
        }
        let mut held = Self {
            path,
            file,
            identity: identity(&before),
            digest: String::new(),
        };
        held.digest = sha(&held.bytes());
        held.verify();
        held
    }

    fn bytes(&self) -> Vec<u8> {
        let mut reader = self.file.try_clone().unwrap();
        reader.seek(SeekFrom::Start(0)).unwrap();
        let mut bytes = Vec::new();
        reader
            .take(self.identity.2 + 1)
            .read_to_end(&mut bytes)
            .unwrap();
        assert_eq!(bytes.len() as u64, self.identity.2);
        bytes
    }

    fn verify(&self) {
        assert_eq!(identity(&self.file.metadata().unwrap()), self.identity);
        assert_eq!(
            identity(&fs::symlink_metadata(&self.path).unwrap()),
            self.identity
        );
        assert_eq!(sha(&self.bytes()), self.digest);
    }
}

fn required(name: &str) -> PathBuf {
    PathBuf::from(std::env::var_os(name).unwrap_or_else(|| panic!("{name} required")))
}

fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[derive(Clone, Copy)]
struct Inputs<'a> {
    binary: &'a Path,
    root: &'a Path,
    evidence: &'a Path,
}

fn invoke(inputs: Inputs<'_>, output: &Path, extra: &[OsString], name: &str, code: i32) -> Vec<u8> {
    let mut argv = vec![
        inputs.binary.as_os_str().to_os_string(),
        output.as_os_str().to_os_string(),
    ];
    argv.extend_from_slice(extra);
    let mut command = CommandSpec::new_os(argv, Duration::from_secs(30));
    command.cwd = Some(inputs.root.to_path_buf());
    command.env_remove = vec!["PYTHONPATH".into(), "PYTHONHOME".into()];
    let mut runner = NativeRunner;
    let captured = runner.run(&command).unwrap();
    fs::write(
        inputs.evidence.join(format!("{name}.stdout")),
        &captured.stdout,
    )
    .unwrap();
    fs::write(
        inputs.evidence.join(format!("{name}.stderr")),
        &captured.stderr,
    )
    .unwrap();
    fs::write(
        inputs.evidence.join(format!("{name}.status")),
        format!("{}\n", captured.code),
    )
    .unwrap();
    assert_eq!(captured.code, code, "{name} failed");
    captured.stdout
}

fn raw_trace_path(encoded: &str) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut pos = 0;
    while pos < encoded.len() {
        if encoded.as_bytes()[pos] == b'\\' {
            assert!(pos + 4 <= encoded.len(), "short escaped exec path");
            assert_eq!(&encoded.as_bytes()[pos..pos + 2], b"\\x");
            bytes.push(u8::from_str_radix(&encoded[pos + 2..pos + 4], 16).unwrap());
            pos += 4;
        } else {
            assert!(encoded.as_bytes()[pos].is_ascii());
            bytes.push(encoded.as_bytes()[pos]);
            pos += 1;
        }
    }
    bytes
}

fn audit_trace_text(text: &str, binary: &Path) -> usize {
    let mut attempts = 0;
    for line in text.lines().filter(|line| line.contains("execve")) {
        assert!(
            !line.contains("execveat") && !line.contains("..."),
            "{line}"
        );
        let encoded = line
            .strip_prefix("execve(\"")
            .unwrap_or_else(|| panic!("opaque exec: {line}"));
        let path = encoded.split_once('"').unwrap().0;
        assert_eq!(raw_trace_path(path), binary.as_os_str().as_bytes());
        assert!(line.ends_with(" = 0"), "failed or incomplete exec: {line}");
        attempts += 1;
    }
    attempts
}

#[test]
fn manager_car_generator_trace_parser_rejects_unknown_failed_and_incomplete_execs() {
    let binary = Path::new("/native/generate-manager-cars");
    let good = "execve(\"\\x2f\\x6e\\x61\\x74\\x69\\x76\\x65\\x2f\\x67\\x65\\x6e\\x65\\x72\\x61\\x74\\x65\\x2d\\x6d\\x61\\x6e\\x61\\x67\\x65\\x72\\x2d\\x63\\x61\\x72\\x73\", [\"generate-manager-cars\"], 0x7fff) = 0\n";
    assert_eq!(audit_trace_text(good, binary), 1);
    for bad in [
        good.replace(" = 0", " = -1 ENOENT"),
        good.replace(" = 0", " <unfinished ...>"),
        good.replace("\\x6e\\x61\\x74", "\\x6f\\x74\\x68"),
        good.replace("execve(", "execveat("),
        good.replace(" = 0", ""),
    ] {
        assert!(
            std::panic::catch_unwind(|| audit_trace_text(&bad, binary)).is_err(),
            "accepted invalid exec trace: {bad}"
        );
    }
}

fn traced_native(
    root: &Path,
    binary: &Path,
    evidence: &Path,
    output: &Path,
    extra: &[OsString],
    label: &str,
    code: i32,
) -> Vec<u8> {
    let mut args = vec![output.as_os_str().to_os_string()];
    args.extend_from_slice(extra);
    traced_native_argv(root, binary, evidence, &args, label, code)
}

fn traced_native_argv(
    root: &Path,
    binary: &Path,
    evidence: &Path,
    args: &[OsString],
    label: &str,
    code: i32,
) -> Vec<u8> {
    let trace = evidence.join(format!("{label}.trace"));
    let mut argv = vec![
        OsString::from("/usr/bin/strace"),
        OsString::from("-ff"),
        OsString::from("-s"),
        OsString::from("32768"),
        OsString::from("-xx"),
        OsString::from("-o"),
        trace.as_os_str().to_os_string(),
        OsString::from("-e"),
        OsString::from("trace=execve,execveat"),
        binary.as_os_str().to_os_string(),
    ];
    argv.extend_from_slice(args);
    let mut command = CommandSpec::new_os(argv, Duration::from_secs(30));
    command.cwd = Some(root.to_path_buf());
    command.env_remove = vec!["PYTHONPATH".into(), "PYTHONHOME".into()];
    let mut runner = NativeRunner;
    let captured = runner.run(&command).unwrap();
    fs::write(evidence.join(format!("{label}.stdout")), &captured.stdout).unwrap();
    fs::write(evidence.join(format!("{label}.stderr")), &captured.stderr).unwrap();
    fs::write(
        evidence.join(format!("{label}.status")),
        format!("{}\n", captured.code),
    )
    .unwrap();
    let mut attempts = 0;
    let mut files = 0;
    for entry in fs::read_dir(evidence).unwrap() {
        let entry = entry.unwrap();
        if !entry
            .file_name()
            .to_string_lossy()
            .starts_with(&format!("{label}.trace."))
        {
            continue;
        }
        files += 1;
        let raw = fs::read(entry.path()).unwrap();
        assert!(raw.len() <= 16 * 1024 * 1024 && (raw.is_empty() || raw.ends_with(b"\n")));
        let text = std::str::from_utf8(&raw).unwrap();
        attempts += audit_trace_text(text, binary);
    }
    assert!(files > 0 && attempts == 1, "unexpected {label} exec roster");
    fs::write(evidence.join(format!("{label}.exec-count")), b"1\n").unwrap();
    assert_eq!(captured.code, code, "{label} failed");
    captured.stdout
}

#[test]
#[ignore = "hosted unchanged Python export and native same-input comparison"]
fn fresh_original_car_docs_match_native_generation_and_check_mode() {
    let root = required("MANAGER_CARS_SOURCE_ROOT").canonicalize().unwrap();
    let original = required("MANAGER_CARS_ORIGINAL_OUTPUT");
    let original_stdout = required("MANAGER_CARS_ORIGINAL_STDOUT");
    let binary = required("MANAGER_CARS_NATIVE_BINARY")
        .canonicalize()
        .unwrap();
    let binding = required("MANAGER_CARS_BINDING").canonicalize().unwrap();
    let evidence = required("MANAGER_CARS_EVIDENCE");
    assert!(!evidence.exists());
    fs::create_dir(&evidence).unwrap();
    let held_binary = fs::read(&binary).unwrap();
    let held_binding = fs::read(&binding).unwrap();
    let binding_meta = fs::symlink_metadata(&binding).unwrap();
    assert!(binding_meta.is_file() && !binding_meta.file_type().is_symlink());
    assert!(held_binding.len() >= 20 && &held_binding[..4] == b"\x7fELF");
    let binding_identity = (
        binding_meta.dev(),
        binding_meta.ino(),
        binding_meta.len(),
        binding_meta.mode(),
        (binding_meta.mtime(), binding_meta.mtime_nsec()),
        (binding_meta.ctime(), binding_meta.ctime_nsec()),
    );
    let inputs = Inputs {
        binary: &binary,
        root: &root,
        evidence: &evidence,
    };
    let source_before = BRANDS
        .iter()
        .map(|brand| {
            let path = root.join("opendbc/car").join(brand).join("values.py");
            (path.clone(), sha(&fs::read(path).unwrap()))
        })
        .chain(
            ["opendbc/car/__init__.py", "opendbc/car/docs_definitions.py"].map(|name| {
                let path = root.join(name);
                (path.clone(), sha(&fs::read(path).unwrap()))
            }),
        )
        .collect::<Vec<_>>();
    let output = evidence.join("native");
    let stdout = invoke(inputs, &output, &[], "generate", 0);
    assert_eq!(stdout, fs::read(&original_stdout).unwrap());
    for brand in BRANDS {
        let original_bytes = fs::read(original.join(format!("{brand}.json"))).unwrap();
        let native_bytes = fs::read(output.join(format!("{brand}.json"))).unwrap();
        let prior = fs::read(
            root.join("rust/crates/manager/data/cars")
                .join(format!("{brand}.json")),
        )
        .unwrap();
        assert_eq!(
            native_bytes, original_bytes,
            "fresh source car names changed: {brand}"
        );
        assert_eq!(
            original_bytes, prior,
            "original current source differs from installed manager data: {brand}"
        );
        let names: serde_json::Value = serde_json::from_slice(&native_bytes).unwrap();
        let names = names.as_array().unwrap();
        assert!(!names.is_empty() && names.iter().all(serde_json::Value::is_string));
    }
    let check = invoke(inputs, &output, &[OsString::from("--check")], "check", 0);
    assert_eq!(check, stdout);
    let changed = output.join("ford.json");
    fs::write(&changed, b"[\"wrong\"]\n").unwrap();
    invoke(
        inputs,
        &output,
        &[OsString::from("--check")],
        "changed-check",
        1,
    );
    assert_eq!(fs::read(&changed).unwrap(), b"[\"wrong\"]\n");
    let symlink_target = evidence.join("gm-target.json");
    fs::rename(output.join("gm.json"), &symlink_target).unwrap();
    symlink(&symlink_target, output.join("gm.json")).unwrap();
    let symlink_stdout = invoke(inputs, &output, &[], "symlink-output", 0);
    assert_eq!(symlink_stdout, stdout);
    assert!(fs::symlink_metadata(output.join("gm.json"))
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(
        fs::read(&symlink_target).unwrap(),
        fs::read(original.join("gm.json")).unwrap()
    );
    let missing_check = evidence.join("missing-check-output");
    invoke(
        inputs,
        &missing_check,
        &[OsString::from("--check")],
        "missing-check",
        1,
    );
    assert!(missing_check.is_dir());
    assert!(!missing_check.join("hyundai.json").exists());
    let unsupported_output = evidence.join("unsupported-binding-output");
    invoke(
        inputs,
        &unsupported_output,
        &[
            OsString::from("--binding"),
            binding.as_os_str().to_os_string(),
        ],
        "unsupported-binding-option",
        2,
    );
    assert!(
        !unsupported_output.exists(),
        "unsupported option created output"
    );
    let changed_root = evidence.join("changed-source-root");
    fs::create_dir_all(changed_root.join("rust/tools")).unwrap();
    for (path, _) in &source_before {
        let relative = path.strip_prefix(&root).unwrap();
        let target = changed_root.join(relative);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::copy(path, target).unwrap();
    }
    let changed_definition = changed_root.join("opendbc/car/docs_definitions.py");
    let mut changed_bytes = fs::read(&changed_definition).unwrap();
    changed_bytes.extend_from_slice(b"\n# source revision requires review\n");
    fs::write(&changed_definition, changed_bytes).unwrap();
    let changed_output = evidence.join("changed-source-output");
    invoke(
        Inputs {
            root: &changed_root,
            ..inputs
        },
        &changed_output,
        &[],
        "changed-source",
        1,
    );
    assert!(!changed_output.exists(), "changed source created output");
    assert!(fs::read_to_string(evidence.join("changed-source.stderr"))
        .unwrap()
        .contains("manager car source revision changed: opendbc/car/docs_definitions.py"));
    assert_eq!(sha(&fs::read(&binary).unwrap()), sha(&held_binary));
    assert_eq!(sha(&fs::read(&binding).unwrap()), sha(&held_binding));
    let binding_after = fs::symlink_metadata(&binding).unwrap();
    assert_eq!(
        (
            binding_after.dev(),
            binding_after.ino(),
            binding_after.len(),
            binding_after.mode(),
            (binding_after.mtime(), binding_after.mtime_nsec()),
            (binding_after.ctime(), binding_after.ctime_nsec()),
        ),
        binding_identity,
        "original Params provider identity changed"
    );
    for (path, digest) in source_before {
        assert_eq!(
            sha(&fs::read(path).unwrap()),
            digest,
            "opendbc values changed during generation"
        );
    }
}

#[test]
#[ignore = "hosted same-ELF source-present/absent generator proof against captured original bytes"]
fn native_car_generator_preserves_original_bytes_in_both_source_states() {
    let checkout = required("MANAGER_CARS_RETIREMENT_ROOT")
        .canonicalize()
        .unwrap();
    let evidence = required("MANAGER_CARS_RETIREMENT_EVIDENCE");
    let require_absent = match std::env::var("MANAGER_CARS_RETIREMENT_REQUIRE_ABSENT")
        .expect("MANAGER_CARS_RETIREMENT_REQUIRE_ABSENT required")
        .as_str()
    {
        "0" => false,
        "1" => true,
        value => panic!("invalid absence requirement: {value}"),
    };
    assert!(!evidence.exists());
    fs::create_dir(&evidence).unwrap();
    let historical = checkout.join("rust/tools/generate_manager_cars.py");
    assert_eq!(
        fs::symlink_metadata(&historical)
            .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound),
        require_absent,
    );
    let binary = HeldInput::open(
        required("MANAGER_CARS_RETIREMENT_BINARY")
            .canonicalize()
            .unwrap(),
        true,
    );
    let strace = HeldInput::open(PathBuf::from("/usr/bin/strace"), true);
    let installed = ORIGINAL_BYTES
        .iter()
        .map(|(brand, digest)| {
            let pin = HeldInput::open(
                checkout
                    .join("rust/crates/manager/data/cars")
                    .join(format!("{brand}.json")),
                false,
            );
            assert_eq!(
                pin.digest, *digest,
                "installed data differs from captured original {brand}"
            );
            pin
        })
        .collect::<Vec<_>>();
    let private = evidence.join("private-root");
    fs::create_dir_all(private.join("rust/tools")).unwrap();
    for (name, expected) in ORIGINAL_SOURCES {
        let source = HeldInput::open(checkout.join(name), false);
        assert_eq!(
            source.digest, expected,
            "original source revision changed: {name}"
        );
        let target = private.join(name);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(&target, source.bytes()).unwrap();
    }
    let sources = [checkout.as_path(), private.as_path()].map(|root| {
        ORIGINAL_SOURCES
            .iter()
            .map(|(name, expected)| {
                let pin = HeldInput::open(root.join(name), false);
                assert_eq!(pin.digest.as_str(), *expected);
                pin
            })
            .collect::<Vec<_>>()
    });
    assert!(
        fs::symlink_metadata(private.join("rust/tools/generate_manager_cars.py"))
            .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
    );
    for (label, root) in [
        ("checkout", checkout.as_path()),
        ("private", private.as_path()),
    ] {
        let output = evidence.join(format!("{label}-output"));
        assert_eq!(
            traced_native(
                root,
                &binary.path,
                &evidence,
                &output,
                &[],
                &format!("{label}-generate"),
                0
            ),
            ORIGINAL_STDOUT
        );
        for ((brand, _), original) in ORIGINAL_BYTES.iter().zip(&installed) {
            assert_eq!(
                fs::read(output.join(format!("{brand}.json"))).unwrap(),
                original.bytes()
            );
        }
        assert_eq!(
            traced_native(
                root,
                &binary.path,
                &evidence,
                &output,
                &[OsString::from("--check")],
                &format!("{label}-check"),
                0
            ),
            ORIGINAL_STDOUT
        );
        let changed = output.join("ford.json");
        fs::write(&changed, b"[\"wrong\"]\n").unwrap();
        traced_native(
            root,
            &binary.path,
            &evidence,
            &output,
            &[OsString::from("--check")],
            &format!("{label}-changed-check"),
            1,
        );
        assert_eq!(fs::read(changed).unwrap(), b"[\"wrong\"]\n");
    }
    let reappeared = private.join("rust/tools/generate_manager_cars.py");
    fs::write(&reappeared, b"unexpected generator reappearance").unwrap();
    let rejected = evidence.join("reappearance-output");
    traced_native(
        &private,
        &binary.path,
        &evidence,
        &rejected,
        &[],
        "reappearance",
        1,
    );
    assert!(!rejected.exists());
    assert!(fs::read_to_string(evidence.join("reappearance.stderr"))
        .unwrap()
        .contains("manager car historical generator source changed"));
    fs::remove_file(reappeared).unwrap();
    // The unchanged original admission capture recorded help=0,
    // missing/unknown=2 and bad binding=1. The native interface deliberately
    // has no Python binding option, so that option is rejected at parse time.
    let help = traced_native_argv(
        &checkout,
        &binary.path,
        &evidence,
        &[OsString::from("--help")],
        "native-help",
        0,
    );
    assert!(help.starts_with(b"usage: generate-manager-cars "));
    for (label, args) in [
        ("native-missing", vec![]),
        ("native-unknown", vec![OsString::from("--unknown")]),
        (
            "native-reject-binding-option",
            vec![
                evidence.join("no-binding-output").into_os_string(),
                OsString::from("--binding"),
                OsString::from("not-a-binding.so"),
            ],
        ),
    ] {
        assert!(traced_native_argv(&checkout, &binary.path, &evidence, &args, label, 2).is_empty());
    }
    assert!(!evidence.join("no-binding-output").exists());
    for pin in sources.iter().flatten().chain(installed.iter()) {
        pin.verify();
    }
    binary.verify();
    strace.verify();
    assert_eq!(
        fs::symlink_metadata(historical)
            .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound),
        require_absent,
    );
    assert!(
        fs::symlink_metadata(private.join("rust/tools/generate_manager_cars.py"))
            .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
    );
    let exec_counts = [
        "checkout-generate",
        "checkout-check",
        "checkout-changed-check",
        "private-generate",
        "private-check",
        "private-changed-check",
        "reappearance",
        "native-help",
        "native-missing",
        "native-unknown",
        "native-reject-binding-option",
    ]
    .into_iter()
    .map(|name| {
        let count = fs::read_to_string(evidence.join(format!("{name}.exec-count"))).unwrap();
        assert_eq!(count, "1\n", "wrong exec count for {name}");
        (name, 1usize)
    })
    .collect::<std::collections::BTreeMap<_, _>>();
    fs::write(
        evidence.join("result.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "passed":true,"checkout_generator_absent":require_absent,
            "private_generator_absent":true,"original_brand_count":7,
            "positive_source_states":2,"closed_native_execs":11,
            "closed_native_execs_by_invocation":exec_counts,
            "original_bytes_sha256":ORIGINAL_BYTES,
            "binary_sha256":binary.digest,"strace_sha256":strace.digest,
        }))
        .unwrap(),
    )
    .unwrap();
}
