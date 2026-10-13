use flate2::{write::GzEncoder, Compression};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};
use tempfile::TempDir;

const REQUIRED: [&str; 13] = [
    "openpilot-launch",
    "openpilot-manager",
    "openpilot-process-child",
    "openpilot-carrot-recovery",
    "openpilot-carrot-web-watchdog",
    "openpilot-carrot-server",
    "openpilot-agnos",
    "openpilot-agnos-updater",
    "openpilot-text-window",
    "openpilot-usbgpu-model",
    "openpilot-bootlog",
    "openpilot-usbgpu-worker",
    "openpilot-usbgpu-probe",
];

#[derive(Clone, Copy)]
enum Package {
    Valid,
    WrongElf,
    WrongMode,
    MissingBinary,
    Duplicate,
    UnexpectedSource,
    EscapingLink,
    InternalLink,
    LongPath,
    WrongArtifactHash,
    SourceAsset,
    Traversal,
    DuplicateManifest,
}

struct Fixture {
    dir: TempDir,
    sha: String,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("checkout");
        fs::create_dir(&root).unwrap();
        for args in [
            vec!["init", "-q"],
            vec![
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.invalid",
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                "fixture",
            ],
        ] {
            assert!(Command::new("git")
                .args(args)
                .current_dir(&root)
                .status()
                .unwrap()
                .success());
        }
        let output = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&root)
            .output()
            .unwrap();
        Self {
            dir,
            sha: String::from_utf8(output.stdout).unwrap().trim().into(),
        }
    }
    fn root(&self) -> std::path::PathBuf {
        self.dir.path().join("checkout")
    }
    fn archive(&self) {
        self.package(Package::Valid);
    }
    fn package(&self, variant: Package) {
        let mut elf = vec![0_u8; 64];
        elf[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
        elf[16..20].copy_from_slice(&[2, 0, 183, 0]);
        elf[20] = 1;
        elf[52] = 64;
        if matches!(variant, Package::WrongElf) {
            elf[18] = 62;
        }
        let mut files = Vec::new();
        let mut builder = tar::Builder::new(GzEncoder::new(Vec::new(), Compression::fast()));
        for binary in REQUIRED {
            if matches!(variant, Package::MissingBinary) && binary == "openpilot-manager" {
                continue;
            }
            let path = format!("rust/bin/{binary}");
            files.push(json!({"path": path, "sha256": if matches!(variant,Package::WrongArtifactHash) { "0".repeat(64) } else { format!("{:x}", Sha256::digest(&elf)) }, "executable": true}));
            append(
                &mut builder,
                &path,
                &elf,
                if matches!(variant, Package::WrongMode) {
                    0o644
                } else {
                    0o755
                },
            );
        }
        let mut unpacked = 64 * files.len();
        append(
            &mut builder,
            "rust/bin/usbgpu-assets/kernel",
            b"kernel",
            0o644,
        );
        unpacked += 6;
        files.push(json!({"path":"rust/bin/usbgpu-assets/kernel","sha256":format!("{:x}",Sha256::digest(b"kernel")),"executable":false}));
        match variant {
            Package::Duplicate => {
                append(&mut builder, "rust/bin/openpilot-launch", &elf, 0o755);
                unpacked += 64;
            }
            Package::UnexpectedSource => {
                files.push(json!({"path":"launch_openpilot.sh","sha256":format!("{:x}",Sha256::digest(b"malicious")),"executable":false}));
                append(&mut builder, "launch_openpilot.sh", b"malicious", 0o644);
                unpacked += 9;
            }
            Package::EscapingLink | Package::InternalLink => {
                let path = "rust/bin/launcher-link";
                let mut header = tar::Header::new_gnu();
                header.set_entry_type(tar::EntryType::Symlink);
                header.set_size(0);
                header.set_mode(0o777);
                header
                    .set_link_name(if matches!(variant, Package::InternalLink) {
                        "openpilot-launch"
                    } else {
                        "../../../outside"
                    })
                    .unwrap();
                header.set_cksum();
                builder.append_data(&mut header, path, &b""[..]).unwrap();
                files.push(json!({"path":path,"sha256":format!("{:x}",Sha256::digest(&elf)),"executable":false}));
            }
            Package::LongPath => {
                let path = format!("rust/bin/{}/metadata.json", "long".repeat(35));
                append(&mut builder, &path, b"{}", 0o644);
                unpacked += 2;
                files.push(json!({"path":path,"sha256":format!("{:x}",Sha256::digest(b"{}")),"executable":false}));
            }
            Package::SourceAsset => {
                let path = "openpilot/selfdrive/assets/fonts/KaiGenGothicKR-Bold.ttf";
                append(&mut builder, path, b"fixture-font", 0o644);
                unpacked += 12;
                files.push(json!({"path":path,"sha256":format!("{:x}",Sha256::digest(b"fixture-font")),"executable":false}));
            }
            Package::Traversal => {
                let mut header = tar::Header::new_gnu();
                header.set_size(9);
                header.set_mode(0o644);
                header.as_mut_bytes()[..18].copy_from_slice(b"../../outside-file");
                header.set_cksum();
                builder.append(&header, &b"malicious"[..]).unwrap();
                unpacked += 9;
            }
            Package::DuplicateManifest => {
                files.push(files[0].clone());
            }
            Package::Valid
            | Package::WrongElf
            | Package::WrongMode
            | Package::MissingBinary
            | Package::WrongArtifactHash => (),
        }
        let manifest = serde_json::to_vec(&json!({"schema":1,"source_commit":self.sha,"agnos_version":"19.8-carrot-bt1","files":files})).unwrap();
        append(&mut builder, "rust/runtime-manifest.json", &manifest, 0o644);
        let archive = builder.into_inner().unwrap().finish().unwrap();
        fs::write(self.dir.path().join("runtime.tar.gz"), &archive).unwrap();
        let descriptor = json!({"schema":1,"source_commit":self.sha,"architecture":"aarch64","agnos_version":"19.8-carrot-bt1","manifest_sha256":format!("{:x}", Sha256::digest(&manifest)),"archive":{"name":format!("openpilot-rust-{}-aarch64.tar.gz",self.sha),"sha256":format!("{:x}", Sha256::digest(&archive)),"bytes":archive.len(),"unpacked_bytes":unpacked+manifest.len()}});
        fs::write(
            self.dir.path().join("descriptor.json"),
            serde_json::to_vec(&descriptor).unwrap(),
        )
        .unwrap();
    }
    fn run(&self, extra: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_openpilot-runtime-install"))
            .arg("--root")
            .arg(self.root())
            .args(extra)
            .output()
            .unwrap()
    }
    fn install(&self) -> Output {
        Command::new(env!("CARGO_BIN_EXE_openpilot-runtime-install"))
            .arg("--root")
            .arg(self.root())
            .arg("--archive")
            .arg(self.dir.path().join("runtime.tar.gz"))
            .arg("--descriptor")
            .arg(self.dir.path().join("descriptor.json"))
            .arg("--reserve-bytes")
            .arg("0")
            .output()
            .unwrap()
    }
    fn descriptor(&self, change: impl FnOnce(&mut serde_json::Value)) {
        let path = self.dir.path().join("descriptor.json");
        let mut value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        change(&mut value);
        fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
    }
}

fn append<W: std::io::Write>(builder: &mut tar::Builder<W>, path: &str, bytes: &[u8], mode: u32) {
    let mut header = tar::Header::new_gnu();
    header.set_size(u64::try_from(bytes.len()).unwrap());
    header.set_mode(mode);
    header.set_cksum();
    builder
        .append_data(&mut header, Path::new(path), bytes)
        .unwrap();
}

#[test]
fn fresh_checkout_installs_offline_package() {
    // Given a source checkout and complete AArch64 package.
    let fixture = Fixture::new();
    fixture.archive();
    // When the real installer handles the offline package.
    let result = fixture.install();
    // Then startup paths exist and a networking-free check succeeds.
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(fixture.root().join("rust/bin/openpilot-launch").is_file());
    assert!(fixture.run(&["--check"]).status.success());
}

macro_rules! rejected_package {
    ($name:ident,$kind:expr) => {
        #[test]
        fn $name() {
            // Given an existing runtime and a malformed but descriptor-sealed package.
            let fixture = Fixture::new();
            fixture.package($kind);
            fs::create_dir_all(fixture.root().join("rust/bin")).unwrap();
            fs::write(fixture.root().join("rust/bin/previous"), b"preserved").unwrap();
            // When the real offline CLI attempts installation.
            let result = fixture.install();
            // Then failure preserves the previous generation.
            assert!(!result.status.success());
            assert_eq!(
                fs::read(fixture.root().join("rust/bin/previous")).unwrap(),
                b"preserved"
            );
            assert!(!fixture.root().join("rust/outside-file").exists());
        }
    };
}
rejected_package!(wrong_elf_is_rejected, Package::WrongElf);
rejected_package!(missing_executable_mode_is_rejected, Package::WrongMode);
rejected_package!(missing_required_binary_is_rejected, Package::MissingBinary);
rejected_package!(duplicate_member_is_rejected, Package::Duplicate);
rejected_package!(
    arbitrary_source_write_is_rejected,
    Package::UnexpectedSource
);

macro_rules! rejected_descriptor {
    ($name:ident,$field:literal,$value:expr) => {
        #[test]
        fn $name() {
            // Given an unsupported release identity field.
            let fixture = Fixture::new();
            fixture.archive();
            fixture.descriptor(|v| v[$field] = json!($value));
            // When the real CLI validates the descriptor.
            let result = fixture.install();
            // Then no runtime is activated.
            assert!(!result.status.success());
            assert!(!fixture.root().join("rust/bin").exists());
        }
    };
}
rejected_descriptor!(unknown_release_schema_is_rejected, "schema", 2);
rejected_descriptor!(
    wrong_release_architecture_is_rejected,
    "architecture",
    "x86_64"
);
rejected_descriptor!(wrong_agnos_identity_is_rejected, "agnos_version", "wrong");
rejected_descriptor!(
    wrong_manifest_digest_is_rejected,
    "manifest_sha256",
    "0".repeat(64)
);

#[test]
fn missing_install_check_does_not_invoke_curl() {
    use std::os::unix::fs::PermissionsExt;
    // Given an empty checkout and a curl sentinel at the front of the child process PATH.
    let fixture = Fixture::new();
    let commands = fixture.dir.path().join("commands");
    fs::create_dir(&commands).unwrap();
    let marker = fixture.dir.path().join("network-called");
    let script = commands.join("curl");
    fs::write(
        &script,
        format!("#!/bin/sh\ntouch '{}'\nexit 1\n", marker.display()),
    )
    .unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    // When running the actual networking-free --check CLI.
    let result = Command::new(env!("CARGO_BIN_EXE_openpilot-runtime-install"))
        .arg("--root")
        .arg(fixture.root())
        .arg("--check")
        .env(
            "PATH",
            format!("{}:{}", commands.display(), std::env::var("PATH").unwrap()),
        )
        .output()
        .unwrap();
    // Then absent installation is reported without calling the transport.
    assert!(!result.status.success());
    assert!(!marker.exists());
}
rejected_package!(escaping_symlink_is_rejected, Package::EscapingLink);
rejected_package!(wrong_artifact_hash_is_rejected, Package::WrongArtifactHash);
rejected_package!(path_traversal_is_rejected, Package::Traversal);
rejected_package!(
    duplicate_manifest_artifact_is_rejected,
    Package::DuplicateManifest
);

#[test]
fn allowlisted_source_asset_is_installed() {
    // Given a package containing a sealed source font.
    let fixture = Fixture::new();
    fixture.package(Package::SourceAsset);
    // When the real installer activates it.
    let result = fixture.install();
    // Then the source asset is installed and verifies through the original manifest layout.
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        fs::read(
            fixture
                .root()
                .join("openpilot/selfdrive/assets/fonts/KaiGenGothicKR-Bold.ttf")
        )
        .unwrap(),
        b"fixture-font"
    );
    assert!(fixture.run(&["--check"]).status.success());
}

#[test]
fn corrupted_same_sha_generation_is_repaired_without_in_place_writes() {
    // Given a corrupted installed generation and its still-valid original offline package.
    let fixture = Fixture::new();
    fixture.archive();
    assert!(fixture.install().status.success());
    let previous = fs::read_link(fixture.root().join("rust/.runtime-active")).unwrap();
    fs::write(fixture.root().join("rust/bin/openpilot-launch"), b"corrupt").unwrap();
    assert!(!fixture.run(&["--check"]).status.success());
    // When repairing the same source SHA.
    let result = fixture.install();
    // Then a distinct verified generation replaces the corrupted one.
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_ne!(
        fs::read_link(fixture.root().join("rust/.runtime-active")).unwrap(),
        previous
    );
    assert!(fixture.run(&["--check"]).status.success());
}

#[test]
fn source_parent_symlink_cannot_write_outside_checkout() {
    // Given an allowlisted asset whose checkout parent is an external directory alias.
    let fixture = Fixture::new();
    fixture.package(Package::SourceAsset);
    let outside = fixture.dir.path().join("outside");
    fs::create_dir(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, fixture.root().join("openpilot")).unwrap();
    // When activation reaches that source asset.
    let result = fixture.install();
    // Then it refuses to follow the directory alias and the external directory stays empty.
    assert!(!result.status.success());
    assert_eq!(fs::read_dir(outside).unwrap().count(), 0);
}

#[test]
fn truncated_archive_is_rejected_even_with_updated_compressed_digest() {
    // Given a truncated gzip stream sealed by a freshly recalculated descriptor hash.
    let fixture = Fixture::new();
    fixture.archive();
    let path = fixture.dir.path().join("runtime.tar.gz");
    let mut bytes = fs::read(&path).unwrap();
    bytes.truncate(bytes.len() - 8);
    fs::write(path, &bytes).unwrap();
    fixture.descriptor(|v| {
        v["archive"]["bytes"] = json!(bytes.len());
        v["archive"]["sha256"] = json!(format!("{:x}", Sha256::digest(&bytes)));
    });
    // When extraction consumes the entire compressed stream.
    let result = fixture.install();
    // Then the missing gzip footer prevents activation.
    assert!(!result.status.success());
    assert!(!fixture.root().join("rust/bin").exists());
}

#[test]
fn interrupted_source_copy_recovers_without_network() {
    // Given an activation journal and altered source asset preceding the pointer switch.
    let fixture = Fixture::new();
    fixture.package(Package::SourceAsset);
    assert!(fixture.install().status.success());
    let link = fs::read_link(fixture.root().join("rust/.runtime-active")).unwrap();
    fs::write(
        fixture
            .root()
            .join("openpilot/selfdrive/assets/fonts/KaiGenGothicKR-Bold.ttf"),
        b"old-font",
    )
    .unwrap();
    fs::write(
        fixture.root().join("rust/.runtime-install-journal.json"),
        serde_json::to_vec(&json!({"generation":link.file_name().unwrap().to_str().unwrap()}))
            .unwrap(),
    )
    .unwrap();
    // When the offline check replays the interrupted activation.
    let result = fixture.run(&["--check"]);
    // Then the source copy is repaired and all installed files verify.
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        fs::read(
            fixture
                .root()
                .join("openpilot/selfdrive/assets/fonts/KaiGenGothicKR-Bold.ttf")
        )
        .unwrap(),
        b"fixture-font"
    );
}

#[test]
fn completed_journal_cleanup_removes_only_known_backup() {
    // Given a completed activation interrupted after journal removal but before backup removal.
    let fixture = Fixture::new();
    fixture.archive();
    assert!(fixture.install().status.success());
    fs::create_dir_all(fixture.root().join("rust/.runtime-install-rollback/bin")).unwrap();
    fs::write(
        fixture
            .root()
            .join("rust/.runtime-install-rollback/bin/previous"),
        b"old",
    )
    .unwrap();
    // When a networking-free check sees the verified active generation.
    let result = fixture.run(&["--check"]);
    // Then it clears the known completed backup.
    assert!(result.status.success());
    assert!(!fixture
        .root()
        .join("rust/.runtime-install-rollback")
        .exists());
}

#[test]
fn internal_symlink_is_installed() {
    // Given a manifest-sealed internal link.
    let fixture = Fixture::new();
    fixture.package(Package::InternalLink);
    // When installing the package.
    let result = fixture.install();
    // Then the link resolves to a verified internal binary.
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        fs::read_link(fixture.root().join("rust/bin/launcher-link")).unwrap(),
        Path::new("openpilot-launch")
    );
}

#[test]
fn gnu_long_path_is_installed() {
    // Given a valid GNU extension path exceeding the legacy tar header field.
    let fixture = Fixture::new();
    fixture.package(Package::LongPath);
    // When installing through the real CLI.
    let result = fixture.install();
    // Then the complete package verifies.
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn wrong_source_commit_is_rejected() {
    // Given a package describing another checkout.
    let fixture = Fixture::new();
    fixture.archive();
    fixture.descriptor(|v| v["source_commit"] = json!("0".repeat(40)));
    // When installation is attempted.
    let result = fixture.install();
    // Then no runtime activates.
    assert!(!result.status.success());
    assert!(!fixture.root().join("rust/bin").exists());
}

#[test]
fn wrong_download_digest_is_rejected() {
    // Given a descriptor with a wrong archive checksum.
    let fixture = Fixture::new();
    fixture.archive();
    fixture.descriptor(|v| v["archive"]["sha256"] = json!("0".repeat(64)));
    // When installation is attempted.
    let result = fixture.install();
    // Then no runtime activates.
    assert!(!result.status.success());
    assert!(!fixture.root().join("rust/bin").exists());
}

#[test]
fn insufficient_disk_preserves_user_data() {
    // Given a reserve exceeding any possible available filesystem space.
    let fixture = Fixture::new();
    fixture.archive();
    fs::write(fixture.root().join("user-data"), b"preserved").unwrap();
    // When installation checks space before staging downloads.
    let result = fixture.run(&["--reserve-bytes", "18446744073705357311"]);
    // Then it refuses installation and preserves data.
    assert!(!result.status.success());
    assert_eq!(
        fs::read(fixture.root().join("user-data")).unwrap(),
        b"preserved"
    );
    assert!(!fixture.root().join("rust/.runtime-packages").exists());
}

#[test]
fn matching_install_is_reused_without_archive_or_network() {
    // Given a valid installed package with the original local input deleted.
    let fixture = Fixture::new();
    fixture.archive();
    assert!(fixture.install().status.success());
    fs::remove_file(fixture.dir.path().join("runtime.tar.gz")).unwrap();
    fs::remove_file(fixture.dir.path().join("descriptor.json")).unwrap();
    let old = fs::read_link(fixture.root().join("rust/.runtime-active")).unwrap();
    // When normal installation is requested.
    let result = fixture.run(&[]);
    // Then it reuses the installed generation with no download.
    assert!(result.status.success());
    assert_eq!(
        fs::read_link(fixture.root().join("rust/.runtime-active")).unwrap(),
        old
    );
}

#[test]
fn journal_recovers_interrupted_legacy_bin_migration() {
    // Given a verified generation and persisted journal after the old bin was moved.
    let fixture = Fixture::new();
    fixture.archive();
    assert!(fixture.install().status.success());
    let active = fs::read_link(fixture.root().join("rust/.runtime-active")).unwrap();
    let name = active.file_name().unwrap().to_str().unwrap();
    fs::create_dir(fixture.root().join("rust/.runtime-install-rollback")).unwrap();
    fs::remove_file(fixture.root().join("rust/bin")).unwrap();
    fs::create_dir(fixture.root().join("rust/.runtime-install-rollback/bin")).unwrap();
    fs::write(
        fixture.root().join("rust/.runtime-install-journal.json"),
        serde_json::to_vec(&json!({"generation":name})).unwrap(),
    )
    .unwrap();
    // When a networking-free check acquires the installer lock.
    let result = fixture.run(&["--check"]);
    // Then it repairs startup paths and clears the completed journal and backup.
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(fixture.root().join("rust/bin/openpilot-launch").is_file());
    assert!(!fixture
        .root()
        .join("rust/.runtime-install-journal.json")
        .exists());
    assert!(!fixture
        .root()
        .join("rust/.runtime-install-rollback")
        .exists());
}

#[test]
fn concurrent_installers_activate_one_generation() {
    // Given two real installer processes targeting the same checkout and package.
    let fixture = Fixture::new();
    fixture.archive();
    let spawn = || {
        Command::new(env!("CARGO_BIN_EXE_openpilot-runtime-install"))
            .arg("--root")
            .arg(fixture.root())
            .arg("--archive")
            .arg(fixture.dir.path().join("runtime.tar.gz"))
            .arg("--descriptor")
            .arg(fixture.dir.path().join("descriptor.json"))
            .spawn()
            .unwrap()
    };
    // When both race for the advisory lock.
    let mut first = spawn();
    let mut second = spawn();
    // Then both succeed with exactly one verified generation.
    assert!(first.wait().unwrap().success());
    assert!(second.wait().unwrap().success());
    assert_eq!(
        fs::read_dir(fixture.root().join("rust/.runtime-packages"))
            .unwrap()
            .count(),
        1
    );
    assert!(fixture.run(&["--check"]).status.success());
}
