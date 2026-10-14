//! Original Python/SCons format oracles; these exercise native production APIs.
use openpilot_dev_tools::panda_build::{
    certificate_header, firmware_identity, gitversion_header, sign_firmware, verify_bootstub,
    verify_signed_firmware,
};
use std::{fs, path::PathBuf};

const UNSIGNED: &[u8] = include_bytes!("fixtures/panda/unsigned.bin");
const SIGNED: &[u8] = include_bytes!("fixtures/panda/signed.bin");
const DEBUG_PUBLIC: &[u8] = include_bytes!("fixtures/panda/debug.pub");
const RELEASE_PUBLIC: &[u8] = include_bytes!("fixtures/panda/release.pub");

fn repository() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap()
        .to_path_buf()
}

#[test]
fn native_headers_and_version_match_original_public_oracles() {
    let identity = firmware_identity(&repository()).unwrap();
    assert_eq!(identity, "DEV-1de1c7b1f187-DEBUG");
    assert_eq!(
        identity.as_bytes(),
        include_bytes!("fixtures/panda/version")
    );
    assert_eq!(
        gitversion_header(&identity).unwrap().as_bytes(),
        include_bytes!("fixtures/panda/gitversion.h")
    );
    assert_eq!(
        certificate_header(DEBUG_PUBLIC, RELEASE_PUBLIC)
            .unwrap()
            .as_bytes(),
        include_bytes!("fixtures/panda/cert.h")
    );
}

#[test]
fn native_signer_matches_original_setlen_vers_raw_sha1_rsa_bytes() {
    // The existing repository debug signing key is read in memory only. Never
    // copy its bytes into an output tree, assertion message, fixture or receipt.
    let key = fs::read(repository().join("panda/certs/debug")).unwrap();
    let signed = sign_firmware(UNSIGNED, &key, DEBUG_PUBLIC).unwrap();
    assert_eq!(signed, SIGNED);
    verify_signed_firmware(UNSIGNED, &signed, DEBUG_PUBLIC).unwrap();
    assert!(sign_firmware(UNSIGNED, &key, RELEASE_PUBLIC).is_err());
    assert!(sign_firmware(&UNSIGNED[..3], &key, DEBUG_PUBLIC).is_err());
}

#[test]
fn signature_admission_rejects_body_length_trailer_signature_and_key_mutation() {
    verify_signed_firmware(UNSIGNED, SIGNED, DEBUG_PUBLIC).unwrap();
    for index in [0, 4, 63, 64, 68, 72, SIGNED.len() - 1] {
        let mut changed = SIGNED.to_vec();
        changed[index] ^= 1;
        assert!(
            verify_signed_firmware(UNSIGNED, &changed, DEBUG_PUBLIC).is_err(),
            "accepted corruption at {index}"
        );
    }
    let mut changed_raw = UNSIGNED.to_vec();
    changed_raw[4] ^= 1;
    assert!(verify_signed_firmware(&changed_raw, SIGNED, DEBUG_PUBLIC).is_err());
    assert!(verify_signed_firmware(UNSIGNED, SIGNED, RELEASE_PUBLIC).is_err());
    for length in [0, 3, 127, 128, SIGNED.len() - 1] {
        assert!(verify_signed_firmware(UNSIGNED, &SIGNED[..length], DEBUG_PUBLIC).is_err());
    }
}

#[test]
fn cortex_m_boot_vectors_keep_original_address_and_thumb_contract() {
    let vectors = |stack: u32, reset: u32| [stack.to_le_bytes(), reset.to_le_bytes()].concat();
    for (stack, reset) in [(0x2000_0000, 0x0800_0001), (0x2fff_ffff, 0x0801_ffff)] {
        verify_bootstub(&vectors(stack, reset)).unwrap();
    }
    for (stack, reset) in [
        (0x1fff_ffff, 0x0800_0001),
        (0x3000_0000, 0x0800_0001),
        (0x2000_0000, 0x0800_0000),
        (0x2000_0000, 0x0802_0001),
        (0x2000_0000, 0x0800_0002),
    ] {
        assert!(verify_bootstub(&vectors(stack, reset)).is_err());
    }
    for size in 0..8 {
        assert!(verify_bootstub(&vec![0; size]).is_err());
    }
}

#[test]
fn cli_rejects_invalid_build_arguments_before_creating_output() {
    let temporary = tempfile::tempdir().unwrap();
    let output = temporary.path().join("must-not-exist");
    for extra in [
        vec!["--jobs", "0"],
        vec!["--jobs", "3"],
        vec!["--unknown", "value"],
        vec!["--stage", "--stage"],
    ] {
        let result = std::process::Command::new(env!("CARGO_BIN_EXE_build-runtime-panda"))
            .arg("--root")
            .arg(repository())
            .arg("--output")
            .arg(&output)
            .args(["--toolchain", "must-not-execute"])
            .args(&extra)
            .env("PATH", "")
            .output()
            .unwrap();
        assert!(!result.status.success(), "accepted {extra:?}");
        assert!(!output.exists(), "created output for {extra:?}");
    }
}

include!("support/panda_fixture.rs");

#[test]
fn stage_admits_all_files_license_and_receipt_before_any_publication() {
    for conflict in [
        "rust/bin/licenses/panda",
        "panda/board/obj",
        "rust/bin/licenses/panda/LICENSE",
    ] {
        let temporary = tempfile::tempdir().unwrap();
        let (root, output) = stage_fixture(temporary.path());
        let target = root.join(conflict);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(&target, b"conflict").unwrap();
        assert!(openpilot_dev_tools::panda_build::stage(&root, &output).is_err());
        if conflict != "panda/board/obj" {
            assert!(!root.join("panda/board/obj/panda.bin.signed").exists());
        }
        assert!(!root
            .join("rust/bin/licenses/panda/provenance.json")
            .exists());
    }
}
#[test]
fn stage_preserves_source_binding_and_accepts_only_verified_immutable_files() {
    let temporary = tempfile::tempdir().unwrap();
    let (root, output) = stage_fixture(temporary.path());
    openpilot_dev_tools::panda_build::stage(&root, &output).unwrap();
    openpilot_dev_tools::panda_build::stage(&root, &output).unwrap();
    assert_eq!(
        fs::read(root.join("panda/board/obj/panda.bin.signed")).unwrap(),
        SIGNED
    );
    assert!(!output.join("source/panda/certs/debug").exists());
    let unchanged = fs::read(root.join("rust/bin/licenses/panda/provenance.json")).unwrap();
    fs::write(output.join("source/panda/board/main.c"), b"changed source").unwrap();
    assert!(openpilot_dev_tools::panda_build::stage(&root, &output).is_err());
    assert_eq!(
        fs::read(root.join("rust/bin/licenses/panda/provenance.json")).unwrap(),
        unchanged
    );
}

#[test]
fn late_license_preparation_failure_preserves_existing_files_and_publishes_nothing_new() {
    use std::os::unix::fs::PermissionsExt;
    // Root bypasses ordinary directory write permissions; CI and owned host run
    // as unprivileged users. Do not pretend a root execution proved this case.
    if rustix::process::geteuid().is_root() {
        return;
    }
    let temporary = tempfile::tempdir().unwrap();
    let (root, output) = stage_fixture(temporary.path());
    let existing = root.join("panda/board/obj/panda.bin.signed");
    fs::create_dir_all(existing.parent().unwrap()).unwrap();
    fs::write(&existing, SIGNED).unwrap();
    let license = root.join("rust/bin/licenses/panda");
    fs::create_dir_all(&license).unwrap();
    fs::set_permissions(&license, fs::Permissions::from_mode(0o555)).unwrap();
    let result = openpilot_dev_tools::panda_build::stage(&root, &output);
    fs::set_permissions(&license, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(result.is_err());
    assert_eq!(fs::read(existing).unwrap(), SIGNED);
    for name in [
        "panda_h7.bin.signed",
        "bootstub.panda.bin",
        "bootstub.panda_h7.bin",
    ] {
        assert!(!root.join("panda/board/obj").join(name).exists());
    }
    assert!(!license.join("LICENSE").exists());
    assert!(!license.join("provenance.json").exists());
    assert_eq!(
        fs::read_dir(root.join("panda/board/obj")).unwrap().count(),
        1,
        "prepared firmware temporaries must be cleaned"
    );
}
