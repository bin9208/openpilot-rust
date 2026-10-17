use flate2::{write::GzEncoder, Compression};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};
use tempfile::TempDir;

// ARM integration tests use an explicit QEMU launcher; production has no override.
fn installer() -> std::ffi::OsString {
    std::env::var_os("RUNTIME_INSTALLER_TEST_LAUNCHER")
        .unwrap_or_else(|| env!("CARGO_BIN_EXE_openpilot-runtime-install").into())
}

const REQUIRED: [&str; 16] = [
    "openpilot-launch",
    "openpilot-runtime-install",
    "openpilot-manager",
    "openpilot-process-child",
    "openpilot-reboot",
    "openpilot-startup-recovery",
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
    WakeAssets,
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
            Package::WakeAssets => {
                for name in [
                    "panda_tesla_wake.bin.signed",
                    "panda_h7_tesla_wake.bin.signed",
                ] {
                    let path = format!("panda/board/obj/{name}");
                    append(&mut builder, &path, b"verified-wake-fixture", 0o644);
                    unpacked += 21;
                    files.push(json!({"path":path,"sha256":format!("{:x}",Sha256::digest(b"verified-wake-fixture")),"executable":false}));
                }
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
        Command::new(installer())
            .arg("--root")
            .arg(self.root())
            .args(extra)
            .output()
            .unwrap()
    }
    fn install(&self) -> Output {
        Command::new(installer())
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
    let result = Command::new(installer())
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
        Command::new(installer())
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

impl Fixture {
    fn update_target(&mut self) -> String {
        self.archive();
        assert!(self.install().status.success());
        let previous = self.sha.clone();
        assert!(Command::new("git")
            .args([
                "remote",
                "add",
                "origin",
                "https://github.com/bin9208/openpilot-rust.git"
            ])
            .current_dir(self.root())
            .status()
            .unwrap()
            .success());
        fs::write(self.root().join("target-source"), "new source").unwrap();
        for arguments in [
            vec!["add", "target-source"],
            vec![
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.invalid",
                "commit",
                "-qm",
                "target",
            ],
        ] {
            assert!(Command::new("git")
                .args(arguments)
                .current_dir(self.root())
                .status()
                .unwrap()
                .success());
        }
        self.sha = String::from_utf8(
            Command::new("git")
                .args(["rev-parse", "HEAD"])
                .current_dir(self.root())
                .output()
                .unwrap()
                .stdout,
        )
        .unwrap()
        .trim()
        .into();
        assert!(Command::new("git")
            .args(["branch", "candidate", &self.sha])
            .current_dir(self.root())
            .status()
            .unwrap()
            .success());
        assert!(Command::new("git")
            .args(["reset", "--hard", &previous])
            .current_dir(self.root())
            .output()
            .unwrap()
            .status
            .success());
        self.archive();
        fs::create_dir_all(self.dir.path().join("params/d")).unwrap();
        fs::write(self.dir.path().join("params/d/IsOnroad"), "0").unwrap();
        previous
    }
    fn stage_update(&self) -> Output {
        self.stage_selected(&[])
    }
    fn stage_selected(&self, selection: &[&str]) -> Output {
        self.stage_target(&self.sha, selection)
    }
    fn stage_target(&self, target: &str, selection: &[&str]) -> Output {
        Command::new(installer())
            .arg("--stage-update")
            .arg("--root")
            .arg(self.root())
            .arg("--target")
            .arg(target)
            .arg("--staging")
            .arg(self.dir.path().join("safe_staging"))
            .arg("--params-root")
            .arg(self.dir.path().join("params"))
            .arg("--archive")
            .arg(self.dir.path().join("runtime.tar.gz"))
            .arg("--descriptor")
            .arg(self.dir.path().join("descriptor.json"))
            .args(["--reserve-bytes", "0"])
            .args(selection)
            .output()
            .unwrap()
    }
    fn head(&self) -> String {
        String::from_utf8(
            Command::new("git")
                .args(["rev-parse", "HEAD"])
                .current_dir(self.root())
                .output()
                .unwrap()
                .stdout,
        )
        .unwrap()
        .trim()
        .into()
    }
}

fn update_git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args([
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
        ])
        .args(args)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}

#[test]
fn pending_update_can_be_reselected_without_changing_active_source() {
    let mut fixture = Fixture::new();
    let previous = fixture.update_target();
    assert!(fixture.stage_update().status.success());
    let first = fixture.sha.clone();
    let tree = update_git(
        &fixture.root(),
        &["rev-parse", &format!("{first}^{{tree}}")],
    );
    fixture.sha = update_git(
        &fixture.root(),
        &["commit-tree", &tree, "-p", &first, "-m", "second target"],
    );
    update_git(
        &fixture.root(),
        &["branch", "second-candidate", &fixture.sha],
    );
    fixture.archive();
    let finalized = fixture.dir.path().join("safe_staging/finalized");
    let ready = fs::read(finalized.join(".runtime-update-ready.json")).unwrap();
    let manifest = fs::read(finalized.join("rust/runtime-manifest.json")).unwrap();
    let archive = fixture.dir.path().join("runtime.tar.gz");
    let valid = fs::read(&archive).unwrap();
    fs::write(&archive, b"broken replacement").unwrap();
    let failed = fixture.stage_update();
    assert!(!failed.status.success());
    assert_eq!(
        fs::read(finalized.join(".runtime-update-ready.json")).unwrap(),
        ready
    );
    assert_eq!(
        fs::read(finalized.join("rust/runtime-manifest.json")).unwrap(),
        manifest
    );
    assert!(finalized.join(".overlay_consistent").is_file());
    assert_eq!(update_git(&finalized, &["rev-parse", "HEAD"]), first);
    fs::write(&archive, valid).unwrap();
    let result = fixture.stage_update();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(fixture.head(), previous);
    assert_eq!(update_git(&finalized, &["rev-parse", "HEAD"]), fixture.sha);
    assert!(fixture.run(&["--check"]).status.success());
}

#[test]
fn same_commit_reselection_honors_branch_and_detach() {
    let mut fixture = Fixture::new();
    let previous = fixture.update_target();
    assert!(fixture.stage_update().status.success());
    let finalized = fixture.dir.path().join("safe_staging/finalized");
    for (args, expected) in [
        (&["--branch", "candidate"][..], "candidate"),
        (&["--detach"][..], ""),
    ] {
        let result = fixture.stage_selected(args);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(
            update_git(&finalized, &["branch", "--show-current"]),
            expected
        );
        assert_eq!(fixture.head(), previous);
    }
}

#[test]
fn complete_update_is_staged_without_mutating_the_active_source_package_pair() {
    let mut fixture = Fixture::new();
    let previous = fixture.update_target();
    let old_manifest = fs::read(fixture.root().join("rust/runtime-manifest.json")).unwrap();
    let result = fixture.stage_update();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(fixture.head(), previous);
    assert_eq!(
        fs::read(fixture.root().join("rust/runtime-manifest.json")).unwrap(),
        old_manifest
    );
    let finalized = fixture.dir.path().join("safe_staging/finalized");
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(finalized.join("rust/runtime-manifest.json")).unwrap())
            .unwrap();
    assert_eq!(manifest["source_commit"], fixture.sha);
    assert!(finalized.join("target-source").is_file());
    assert!(finalized.join(".git").is_dir());
    assert!(!finalized.join(".git/objects/info/alternates").exists());
    let origin = Command::new("git")
        .args(["remote", "get-url", "origin"])
        .current_dir(finalized)
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8(origin.stdout).unwrap().trim(),
        "https://github.com/bin9208/openpilot-rust.git"
    );
}

#[test]
fn missing_target_release_keeps_old_source_package_and_user_data() {
    let mut fixture = Fixture::new();
    let previous = fixture.update_target();
    let manifest = fs::read(fixture.root().join("rust/runtime-manifest.json")).unwrap();
    let user_data = fixture.dir.path().join("user-route");
    fs::write(&user_data, b"preserve route").unwrap();
    fs::remove_file(fixture.dir.path().join("descriptor.json")).unwrap();
    assert!(!fixture.stage_update().status.success());
    assert_eq!(fixture.head(), previous);
    assert_eq!(
        fs::read(fixture.root().join("rust/runtime-manifest.json")).unwrap(),
        manifest
    );
    assert_eq!(fs::read(user_data).unwrap(), b"preserve route");
    assert!(!fixture.dir.path().join("safe_staging/finalized").exists());
    assert!(!fixture.root().join("target-source").exists());
}

#[test]
fn preparation_succeeds_for_onroad_or_missing_is_onroad() {
    for onroad in [true, false] {
        let mut fixture = Fixture::new();
        let previous = fixture.update_target();
        let is_onroad = fixture.dir.path().join("params/d/IsOnroad");
        if onroad {
            fs::write(&is_onroad, "1").unwrap();
        } else {
            fs::remove_file(&is_onroad).unwrap();
        }
        let result = fixture.stage_update();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(fixture.head(), previous);
        let check = fixture.run(&["--check"]);
        assert!(
            check.status.success(),
            "{}",
            String::from_utf8_lossy(&check.stderr)
        );
        let finalized = fixture.dir.path().join("safe_staging/finalized");
        let ready = finalized.join(".runtime-update-ready.json");
        assert!(ready.is_file(), "missing {}", ready.display());
        let ready_value: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&ready).unwrap()).unwrap();
        assert!(ready_value.is_object());
        let overlay = finalized.join(".overlay_consistent");
        assert!(overlay.is_file(), "missing {}", overlay.display());
    }
}

#[test]
fn staged_source_records_configured_origin_url() {
    let mut fixture = Fixture::new();
    fixture.update_target();
    let configured = "https://example.invalid/my/runtime.git";
    assert!(Command::new("git")
        .args(["remote", "set-url", "origin", configured])
        .current_dir(fixture.root())
        .status()
        .unwrap()
        .success());
    let result = fixture.stage_update();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let output = Command::new("git")
        .args(["config", "remote.origin.url"])
        .current_dir(fixture.dir.path().join("safe_staging/finalized"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), configured);
}

#[test]
fn startup_activates_the_complete_pair_and_retains_old_helpers() {
    let mut fixture = Fixture::new();
    let previous = fixture.update_target();
    assert!(fixture.stage_update().status.success());
    let stage = fixture.dir.path().join("safe_staging");
    assert!(openpilot_runtime_installer::transaction::activate(
        &fixture.root(),
        &stage,
        &fixture.dir.path().join("params")
    )
    .unwrap());
    assert_eq!(fixture.head(), fixture.sha);
    assert!(fixture.run(&["--check"]).status.success());
    let backup = fs::read_dir(&stage)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("old_openpilot.")
        })
        .unwrap();
    let old = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&backup)
        .output()
        .unwrap();
    assert_eq!(String::from_utf8(old.stdout).unwrap().trim(), previous);
    assert!(backup.join("rust/bin/openpilot-carrot-server").is_file());
    assert!(!stage.join(".runtime-update-journal.json").exists());
    assert!(!openpilot_runtime_installer::transaction::activate(
        &fixture.root(),
        &stage,
        &fixture.dir.path().join("params")
    )
    .unwrap());
}

#[test]
fn crash_recovery_before_and_after_atomic_exchange_keeps_a_bootable_root() {
    for after_exchange in [false, true] {
        let mut fixture = Fixture::new();
        let previous = fixture.update_target();
        assert!(fixture.stage_update().status.success());
        let stage = fixture.dir.path().join("safe_staging");
        let finalized = stage.join("finalized");
        fs::copy(
            finalized.join(".runtime-update-ready.json"),
            stage.join(".runtime-update-journal.json"),
        )
        .unwrap();
        if after_exchange {
            rustix::fs::renameat_with(
                rustix::fs::CWD,
                fixture.root(),
                rustix::fs::CWD,
                &finalized,
                rustix::fs::RenameFlags::EXCHANGE,
            )
            .unwrap();
        }
        // A complete source/package pair exists at the normal boot path at both crash points.
        assert_eq!(
            fixture.head(),
            if after_exchange {
                fixture.sha.clone()
            } else {
                previous.clone()
            }
        );
        assert!(fixture.run(&["--check"]).status.success());
        assert!(openpilot_runtime_installer::transaction::activate(
            &fixture.root(),
            &stage,
            &fixture.dir.path().join("params")
        )
        .unwrap());
        assert_eq!(fixture.head(), fixture.sha);
        assert!(fixture.run(&["--check"]).status.success());
    }
}

#[test]
fn same_sha_package_repair_recovers_using_the_unique_ready_marker() {
    let mut fixture = Fixture::new();
    fixture.update_target();
    // Choose the active SHA to model rebuilding/repairing that package.
    fixture.sha = fixture.head();
    fixture.archive();
    assert!(fixture.stage_update().status.success());
    let stage = fixture.dir.path().join("safe_staging");
    let finalized = stage.join("finalized");
    fs::copy(
        finalized.join(".runtime-update-ready.json"),
        stage.join(".runtime-update-journal.json"),
    )
    .unwrap();
    rustix::fs::renameat_with(
        rustix::fs::CWD,
        fixture.root(),
        rustix::fs::CWD,
        &finalized,
        rustix::fs::RenameFlags::EXCHANGE,
    )
    .unwrap();
    assert!(openpilot_runtime_installer::transaction::activate(
        &fixture.root(),
        &stage,
        &fixture.dir.path().join("params")
    )
    .unwrap());
    assert!(fixture.run(&["--check"]).status.success());
    assert!(!finalized.exists());
}

#[test]
fn tampered_staged_source_is_rejected_without_touching_active_pair() {
    let mut fixture = Fixture::new();
    let previous = fixture.update_target();
    let result = fixture.stage_update();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let stage = fixture.dir.path().join("safe_staging");
    fs::write(stage.join("finalized/target-source"), "tampered source").unwrap();
    assert!(openpilot_runtime_installer::transaction::activate(
        &fixture.root(),
        &stage,
        &fixture.dir.path().join("params")
    )
    .is_err());
    assert_eq!(fixture.head(), previous);
    let check = fixture.run(&["--check"]);
    assert!(
        check.status.success(),
        "{}",
        String::from_utf8_lossy(&check.stderr)
    );
    assert!(stage.join("finalized/rust/bin/openpilot-launch").is_file());
}

#[test]
fn activation_succeeds_for_onroad_or_missing_is_onroad() {
    for onroad in [true, false] {
        let mut fixture = Fixture::new();
        fixture.update_target();
        let result = fixture.stage_update();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let is_onroad = fixture.dir.path().join("params/d/IsOnroad");
        if onroad {
            fs::write(&is_onroad, "1").unwrap();
        } else {
            fs::remove_file(&is_onroad).unwrap();
        }
        let stage = fixture.dir.path().join("safe_staging");
        assert!(openpilot_runtime_installer::transaction::activate(
            &fixture.root(),
            &stage,
            &fixture.dir.path().join("params")
        )
        .unwrap());
        assert_eq!(fixture.head(), fixture.sha);
        let check = fixture.run(&["--check"]);
        assert!(
            check.status.success(),
            "{}",
            String::from_utf8_lossy(&check.stderr)
        );
    }
}

impl Fixture {
    fn overlay_copy(&self) -> std::path::PathBuf {
        let finalized = self.dir.path().join("safe_staging/finalized");
        fs::create_dir_all(finalized.parent().unwrap()).unwrap();
        assert!(Command::new("git")
            .args(["clone", "--no-hardlinks", "--quiet", "--"])
            .arg(self.root())
            .arg(&finalized)
            .status()
            .unwrap()
            .success());
        assert!(Command::new("git")
            .args(["checkout", "--quiet", "--detach", &self.sha])
            .current_dir(&finalized)
            .status()
            .unwrap()
            .success());
        assert!(Command::new("git")
            .args([
                "remote",
                "set-url",
                "origin",
                "https://github.com/bin9208/openpilot-rust.git"
            ])
            .current_dir(&finalized)
            .status()
            .unwrap()
            .success());
        finalized
    }
    fn adopt_update(&self) -> Output {
        Command::new(installer())
            .arg("--adopt-update")
            .arg("--root")
            .arg(self.root())
            .arg("--staging")
            .arg(self.dir.path().join("safe_staging"))
            .arg("--params-root")
            .arg(self.dir.path().join("params"))
            .arg("--archive")
            .arg(self.dir.path().join("runtime.tar.gz"))
            .arg("--descriptor")
            .arg(self.dir.path().join("descriptor.json"))
            .args(["--reserve-bytes", "0"])
            .output()
            .unwrap()
    }
}

#[test]
fn native_updater_overlay_copy_uses_the_same_verified_atomic_activation() {
    let mut fixture = Fixture::new();
    let previous = fixture.update_target();
    let finalized = fixture.overlay_copy();
    let result = fixture.adopt_update();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(fixture.head(), previous);
    assert!(finalized.join(".runtime-update-ready.json").is_file());
    assert!(openpilot_runtime_installer::transaction::activate(
        &fixture.root(),
        &fixture.dir.path().join("safe_staging"),
        &fixture.dir.path().join("params")
    )
    .unwrap());
    assert_eq!(fixture.head(), fixture.sha);
    assert!(fixture.run(&["--check"]).status.success());
}

#[test]
fn unpublished_overlay_package_cannot_keep_an_old_consistent_marker() {
    let mut fixture = Fixture::new();
    let previous = fixture.update_target();
    let finalized = fixture.overlay_copy();
    fs::write(finalized.join(".overlay_consistent"), "old unchecked flag").unwrap();
    fs::remove_file(fixture.dir.path().join("descriptor.json")).unwrap();
    assert!(!fixture.adopt_update().status.success());
    assert_eq!(fixture.head(), previous);
    assert!(!finalized.join(".overlay_consistent").exists());
    assert!(!finalized.join(".runtime-update-ready.json").exists());
    assert!(fixture.run(&["--check"]).status.success());
}

#[test]
fn staged_checkout_preserves_other_local_branches() {
    let mut fixture = Fixture::new();
    fixture.update_target();
    assert!(fixture.stage_update().status.success());
    let result = Command::new("git")
        .args(["rev-parse", "--verify", "refs/heads/candidate"])
        .current_dir(fixture.dir.path().join("safe_staging/finalized"))
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "native source staging lost an existing local branch"
    );
    assert_eq!(
        String::from_utf8(result.stdout).unwrap().trim(),
        fixture.sha
    );
}

#[test]
fn staged_checkout_preserves_real_origin_branches_instead_of_clone_local_heads() {
    let mut fixture = Fixture::new();
    let previous = fixture.update_target();
    for args in [
        vec![
            "update-ref",
            "refs/remotes/origin/remote-only",
            &fixture.sha,
        ],
        vec!["update-ref", "refs/remotes/origin/dev", &previous],
        vec![
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/dev",
        ],
    ] {
        assert!(Command::new("git")
            .args(args)
            .current_dir(fixture.root())
            .status()
            .unwrap()
            .success());
    }
    let result = fixture.stage_update();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let finalized = fixture.dir.path().join("safe_staging/finalized");
    let refs = Command::new("git")
        .args([
            "for-each-ref",
            "--format=%(objectname) %(refname) %(symref)",
            "refs/remotes/origin/",
        ])
        .current_dir(&finalized)
        .output()
        .unwrap();
    assert!(refs.status.success());
    let refs = String::from_utf8(refs.stdout).unwrap();
    assert!(
        refs.contains(&format!("{} refs/remotes/origin/remote-only ", fixture.sha)),
        "real upstream branch lost: {refs}"
    );
    assert!(
        refs.contains(&format!("{previous} refs/remotes/origin/dev ")),
        "upstream tip changed: {refs}"
    );
    assert!(
        refs.contains("refs/remotes/origin/HEAD refs/remotes/origin/dev"),
        "upstream default lost: {refs}"
    );
    assert!(
        !refs.contains("refs/remotes/origin/candidate "),
        "local branch became upstream branch: {refs}"
    );
}

#[test]
fn implicit_staging_preserves_the_selected_branch_upstream() {
    let mut fixture = Fixture::new();
    fixture.update_target();
    let output = Command::new("git")
        .args(["branch", "--show-current"])
        .current_dir(fixture.root())
        .output()
        .unwrap();
    let branch = String::from_utf8(output.stdout).unwrap();
    let branch = branch.trim();
    for (key, value) in [
        (format!("branch.{branch}.remote"), "origin"),
        (format!("branch.{branch}.merge"), "refs/heads/release"),
    ] {
        assert!(Command::new("git")
            .args(["config", &key, value])
            .current_dir(fixture.root())
            .status()
            .unwrap()
            .success());
    }
    assert!(fixture.stage_update().status.success());
    let upstream = Command::new("git")
        .args(["config", "--get", &format!("branch.{branch}.merge")])
        .current_dir(fixture.dir.path().join("safe_staging/finalized"))
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8(upstream.stdout).unwrap().trim(),
        "refs/heads/release"
    );
}

#[test]
fn live_old_runtime_owner_requires_reboot_before_source_exchange() {
    use std::os::unix::fs::PermissionsExt;
    struct Live(std::process::Child);
    impl Drop for Live {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let mut fixture = Fixture::new();
    let previous = fixture.update_target();
    assert!(fixture.stage_update().status.success());
    let directory = fixture
        .root()
        .join(".runtime-packages/native-fixture/rust/bin");
    fs::create_dir_all(&directory).unwrap();
    let helper = directory.join("openpilot-carrot-server");
    {
        let mut output = fs::File::create(&helper).unwrap();
        std::io::copy(&mut fs::File::open("/bin/sleep").unwrap(), &mut output).unwrap();
        output.sync_all().unwrap();
    }
    fs::set_permissions(&helper, fs::Permissions::from_mode(0o755)).unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    let child = loop {
        match Command::new(&helper).arg("30").spawn() {
            Ok(child) => break child,
            Err(error)
                if error.raw_os_error() == Some(26) && std::time::Instant::now() < deadline =>
            {
                std::thread::sleep(std::time::Duration::from_millis(10))
            }
            Err(error) => panic!("native fixture failed to start: {error}"),
        }
    };
    let live = Live(child);
    let stage = fixture.dir.path().join("safe_staging");
    let result = openpilot_runtime_installer::transaction::activate(
        &fixture.root(),
        &stage,
        &fixture.dir.path().join("params"),
    );
    assert!(result.unwrap_err().to_string().contains("reboot required"));
    assert_eq!(fixture.head(), previous);
    assert!(!stage.join(".runtime-update-journal.json").exists());
    assert!(stage.join("finalized/.runtime-update-ready.json").exists());
    drop(live);
    assert!(openpilot_runtime_installer::transaction::activate(
        &fixture.root(),
        &stage,
        &fixture.dir.path().join("params")
    )
    .unwrap());
    assert_eq!(fixture.head(), fixture.sha);
}

#[test]
fn interrupted_source_preparation_cleans_only_owned_pending_directories() {
    let mut fixture = Fixture::new();
    fixture.update_target();
    let stage = fixture.dir.path().join("safe_staging");
    fs::create_dir_all(stage.join(".pending-update-stale0/checkout")).unwrap();
    fs::write(
        stage.join(".pending-update-stale0/checkout/partial"),
        "incomplete source",
    )
    .unwrap();
    fs::create_dir_all(stage.join("old_openpilot.saved")).unwrap();
    fs::write(
        stage.join("old_openpilot.saved/owner"),
        "retain old executable helpers",
    )
    .unwrap();
    let outside = fixture.dir.path().join("user-data");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("route"), "preserve").unwrap();
    std::os::unix::fs::symlink(&outside, stage.join(".pending-update-link")).unwrap();
    fs::write(stage.join(".pending-update-user-file"), "preserve").unwrap();
    let unknown = [
        ".pending-update-user-data",
        ".pending-update-",
        ".pending-update-short",
        ".pending-update-toolong0",
        ".pending-update-ab_cd1",
    ];
    for name in unknown {
        fs::create_dir(stage.join(name)).unwrap();
        fs::write(stage.join(name).join("user-file"), "preserve").unwrap();
    }
    assert!(fixture.stage_update().status.success());
    assert!(!stage.join(".pending-update-stale0").exists());
    for name in unknown {
        assert_eq!(
            fs::read_to_string(stage.join(name).join("user-file")).unwrap(),
            "preserve"
        );
    }
    assert!(stage.join(".pending-update-link").is_symlink());
    assert_eq!(
        fs::read_to_string(stage.join(".pending-update-user-file")).unwrap(),
        "preserve"
    );
    assert_eq!(
        fs::read_to_string(outside.join("route")).unwrap(),
        "preserve"
    );
    assert!(stage.join("old_openpilot.saved/owner").is_file());
    assert!(stage.join("finalized/.runtime-update-ready.json").is_file());
}

#[test]
fn symlink_finalized_checkout_is_rejected_without_exchanging_active_source() {
    let mut fixture = Fixture::new();
    let previous = fixture.update_target();
    assert!(fixture.stage_update().status.success());
    let stage = fixture.dir.path().join("safe_staging");
    let outside = fixture.dir.path().join("outside-ready-checkout");
    fs::rename(stage.join("finalized"), &outside).unwrap();
    std::os::unix::fs::symlink(&outside, stage.join("finalized")).unwrap();
    let result = openpilot_runtime_installer::transaction::activate(
        &fixture.root(),
        &stage,
        &fixture.dir.path().join("params"),
    );
    assert!(
        result.is_err(),
        "followed a symlink candidate into source exchange"
    );
    assert_eq!(fixture.head(), previous);
    assert!(fs::symlink_metadata(fixture.root()).unwrap().is_dir());
    assert!(stage.join("finalized").is_symlink());
    assert!(outside.join(".runtime-update-ready.json").is_file());
    assert!(!stage.join(".runtime-update-journal.json").exists());
}

#[test]
fn activation_rejects_staging_that_contains_the_active_checkout() {
    let fixture = Fixture::new();
    assert!(openpilot_runtime_installer::transaction::activate(
        &fixture.root(),
        fixture.dir.path(),
        &fixture.dir.path().join("params")
    )
    .is_err());
    assert!(!fixture.dir.path().join(".runtime-update.lock").exists());
}

#[test]
fn selected_wake_images_are_admitted_and_installed_with_sealed_hashes() {
    let fixture = Fixture::new();
    fixture.package(Package::WakeAssets);
    let result = fixture.install();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    for name in [
        "panda_tesla_wake.bin.signed",
        "panda_h7_tesla_wake.bin.signed",
    ] {
        assert_eq!(
            fs::read(fixture.root().join("panda/board/obj").join(name)).unwrap(),
            b"verified-wake-fixture"
        );
    }
    assert!(fixture.run(&["--check"]).status.success());
}

struct NoNetwork(std::cell::Cell<usize>);
impl openpilot_runtime_installer::Download for NoNetwork {
    fn download(&self, _: &str, _: &Path, _: u64) -> openpilot_runtime_installer::Result<()> {
        self.0.set(self.0.get() + 1);
        Err(openpilot_runtime_installer::Error::Invalid(
            "owned network unavailable".into(),
        ))
    }
}
fn cached_repair_options(
    fixture: &Fixture,
    target: &str,
) -> openpilot_runtime_installer::transaction::StageOptions {
    let args: Vec<std::ffi::OsString> = vec![
        "--root".into(),
        fixture.root().into_os_string(),
        "--target".into(),
        target.into(),
        "--staging".into(),
        fixture.dir.path().join("safe_staging").into_os_string(),
        "--params-root".into(),
        fixture.dir.path().join("params").into_os_string(),
        "--reserve-bytes".into(),
        "0".into(),
    ];
    openpilot_runtime_installer::transaction::StageOptions::parse(args.into_iter()).unwrap()
}
#[test]
fn cached_current_package_repairs_offline_without_mutating_active_files() {
    let mut fixture = Fixture::new();
    fixture.update_target();
    let previous = fixture.head();
    fs::write(
        fixture.root().join("rust/bin/openpilot-manager"),
        b"broken runtime",
    )
    .unwrap();
    let network = NoNetwork(std::cell::Cell::new(0));
    let prepared = openpilot_runtime_installer::transaction::prepare(
        cached_repair_options(&fixture, &previous),
        &network,
    );
    assert!(prepared.is_ok(), "{prepared:?}");
    assert_eq!(network.0.get(), 0);
    assert_eq!(fixture.head(), previous);
    assert_eq!(
        fs::read(fixture.root().join("rust/bin/openpilot-manager")).unwrap(),
        b"broken runtime"
    );
    assert!(openpilot_runtime_installer::transaction::activate(
        &fixture.root(),
        &fixture.dir.path().join("safe_staging"),
        &fixture.dir.path().join("params")
    )
    .unwrap());
    assert_eq!(fixture.head(), previous);
    assert!(fixture.run(&["--check"]).status.success());
}

#[test]
fn offline_repair_rejects_unbound_corrupt_missing_and_linked_cache() {
    use std::os::unix::fs::symlink;
    for mode in [
        "corrupt",
        "missing",
        "link",
        "descriptor",
        "different-source",
    ] {
        let mut fixture = Fixture::new();
        fixture.update_target();
        let previous = fixture.head();
        let generation = fixture
            .root()
            .join("rust/.runtime-active")
            .canonicalize()
            .unwrap();
        let archive = generation.join(".archive.tar.gz");
        let original = fs::read(fixture.root().join("rust/bin/openpilot-manager")).unwrap();
        match mode {
            "corrupt" => fs::write(&archive, b"bad archive").unwrap(),
            "missing" => fs::remove_file(&archive).unwrap(),
            "link" => {
                fs::remove_file(&archive).unwrap();
                symlink(fixture.dir.path().join("runtime.tar.gz"), &archive).unwrap();
            }
            "descriptor" => fs::write(generation.join(".runtime-release.json"), b"{}").unwrap(),
            _ => (),
        }
        let target = if mode == "different-source" {
            &fixture.sha
        } else {
            &previous
        };
        let network = NoNetwork(std::cell::Cell::new(0));
        let result = openpilot_runtime_installer::transaction::prepare(
            cached_repair_options(&fixture, target),
            &network,
        );
        assert!(result.is_err(), "{mode}");
        assert_eq!(
            network.0.get(),
            1,
            "{mode}: refused cache falls back to the configured downloader"
        );
        assert_eq!(fixture.head(), previous);
        assert_eq!(
            fs::read(fixture.root().join("rust/bin/openpilot-manager")).unwrap(),
            original
        );
        assert!(!fixture.dir.path().join("safe_staging/finalized").exists());
        if mode == "missing" {
            assert!(
                fixture.run(&["--check"]).status.success(),
                "legacy no-cache generation remains valid"
            );
        }
    }
}

#[test]
fn named_remote_selection_preserves_transport_and_other_branch_upstreams() {
    let mut fixture = Fixture::new();
    let previous = fixture.update_target();
    let root = fixture.root();
    update_git(
        &root,
        &[
            "remote",
            "add",
            "source",
            "https://example.invalid/runtime.git",
        ],
    );
    update_git(
        &root,
        &["update-ref", "refs/remotes/source/release", &fixture.sha],
    );
    update_git(&root, &["branch", "saved", &fixture.sha]);
    update_git(&root, &["config", "branch.saved.remote", "source"]);
    update_git(
        &root,
        &["config", "branch.saved.merge", "refs/heads/release"],
    );
    update_git(
        &root,
        &[
            "config",
            "url.file:///owned-mirror/.insteadOf",
            "https://example.invalid/",
        ],
    );
    let result = fixture.stage_target(
        "refs/remotes/source/release",
        &["--branch", "local-release"],
    );
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let finalized = fixture.dir.path().join("safe_staging/finalized");
    assert_eq!(fixture.head(), previous);
    for branch in ["saved", "local-release"] {
        assert_eq!(
            update_git(&finalized, &["config", &format!("branch.{branch}.remote")]),
            "source"
        );
        assert_eq!(
            update_git(&finalized, &["config", &format!("branch.{branch}.merge")]),
            "refs/heads/release"
        );
    }
    assert_eq!(
        update_git(
            &finalized,
            &["config", "url.file:///owned-mirror/.insteadOf"]
        ),
        "https://example.invalid/"
    );
}

#[test]
fn reselecting_the_same_commit_reuses_its_verified_package_without_network() {
    let mut fixture = Fixture::new();
    let previous = fixture.update_target();
    assert!(fixture.stage_update().status.success());
    let origin = "https://example.invalid/configured.git";
    update_git(&fixture.root(), &["remote", "set-url", "origin", origin]);
    let network = NoNetwork(std::cell::Cell::new(0));
    let result = openpilot_runtime_installer::transaction::prepare(
        cached_repair_options(&fixture, &fixture.sha),
        &network,
    );
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(network.0.get(), 0);
    assert_eq!(fixture.head(), previous);
    assert_eq!(
        update_git(
            &fixture.dir.path().join("safe_staging/finalized"),
            &["config", "remote.origin.url"]
        ),
        origin
    );
}
