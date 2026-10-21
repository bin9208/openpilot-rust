#![cfg(all(target_os = "linux", feature = "recovery-family-native"))]
// Parent integration candidate for tests/recovery_family_frozen.rs.
// Register with required-features = ["recovery-family-native"] after review.
use openpilot_dev_tools::recovery_family::{
    original_fixture::OriginalFixture, run_actions_frozen, run_cwp_frozen, run_http_frozen,
    run_support_frozen, run_support_shutdown_frozen, run_terminal_frozen,
    run_terminal_lifecycle_frozen, run_upload_frozen, ActionsFrozenConfig, CwpFrozenConfig,
    HttpFrozenConfig, Pin, SupportFrozenConfig, SupportShutdownConfig, TerminalFrozenConfig,
    TerminalLifecycleConfig, UploadFrozenConfig,
};
use serde_json::json;
use std::{
    env, fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

type TestResult<T> = Result<T, Box<dyn std::error::Error>>;

fn required(name: &str) -> PathBuf {
    PathBuf::from(env::var_os(name).unwrap_or_else(|| panic!("{name} is required")))
}

const HISTORICAL: &[&str] = &[
    "rust/tools/carrot_recovery_http_control.py",
    "rust/tools/carrot_recovery_terminal_control.py",
    "rust/tools/carrot_recovery_terminal_lifecycle.py",
    "rust/tools/carrot_recovery_support_control.py",
    "rust/tools/carrot_recovery_support_shutdown.py",
    "rust/tools/carrot_recovery_actions_control.py",
    "rust/tools/check_carrot_recovery_upload.py",
    "rust/tools/carrot_recovery_cwp_check.py",
    "rust/tools/carrot_recovery_source.py",
    "rust/tools/carrot_recovery_support_source.py",
    "rust/tools/carrot_recovery_cwp_source.py",
    "rust/tools/carrot_recovery_actions_fixtures.py",
    "rust/tools/carrot_recovery_actions_tools.py",
];

fn source_state(
    root: &Path,
    original: &OriginalFixture,
    present: bool,
    held: &[Pin],
) -> TestResult<()> {
    assert!(root.join("rust/tools").is_dir());
    for (index, relative) in HISTORICAL.iter().enumerate() {
        let path = root.join(relative);
        if present {
            held[index].verify()?;
            assert_eq!(
                held[index].digest(),
                original.original_input_sha256(relative)?
            );
        } else {
            assert_eq!(
                fs::symlink_metadata(&path).unwrap_err().kind(),
                ErrorKind::NotFound
            );
        }
    }
    Ok(())
}

#[test]
fn eight_native_recovery_controllers_same_elf_present_and_absent() -> TestResult<()> {
    let original = OriginalFixture::open()?;
    original.require_complete_family()?;
    let checkout = required("RECOVERY_FAMILY_CHECKOUT_ROOT");
    let private = required("RECOVERY_FAMILY_PRIVATE_ROOT");
    let evidence = required("RECOVERY_FAMILY_FROZEN_EVIDENCE");
    let binary = required("RECOVERY_FAMILY_BINARY");
    let actions = required("RECOVERY_FAMILY_ACTIONS_EXAMPLE");
    let upload = required("RECOVERY_FAMILY_UPLOAD_EXAMPLE");
    let child = required("RECOVERY_FAMILY_PROCESS_CHILD");
    let cwp = required("RECOVERY_FAMILY_CWP_TEST_BINARY");
    let tmux = required("RECOVERY_FAMILY_TMUX");
    let checkout_present = match env::var("RECOVERY_FAMILY_HISTORICAL_PRESENT")?.as_str() {
        "1" => true,
        "0" => false,
        _ => return Err("RECOVERY_FAMILY_HISTORICAL_PRESENT must be 0 or 1".into()),
    };
    let elfs = [
        Pin::open(&binary, 256 * 1024 * 1024)?,
        Pin::open(&actions, 256 * 1024 * 1024)?,
        Pin::open(&upload, 256 * 1024 * 1024)?,
        Pin::open(&child, 256 * 1024 * 1024)?,
        Pin::open(&cwp, 256 * 1024 * 1024)?,
        Pin::open(&tmux, 32 * 1024 * 1024)?,
    ];
    fs::create_dir(&evidence)?;
    let mut errors = Vec::<String>::new();
    for (label, root, historical_present) in [
        ("checkout", &checkout, checkout_present),
        ("private", &private, false),
    ] {
        let lane = evidence.join(label);
        fs::create_dir(&lane)?;
        let server = root.join("openpilot/selfdrive/carrot/recovery/server.py");
        let repo_update = root.join("openpilot/common/repo_update.py");
        let source = Pin::open(&server, 4 * 1024 * 1024)?;
        let helper = Pin::open(&repo_update, 1024 * 1024)?;
        assert_eq!(
            source.digest(),
            original.original_input_sha256("openpilot/selfdrive/carrot/recovery/server.py")?
        );
        assert_eq!(
            helper.digest(),
            original.original_input_sha256("openpilot/common/repo_update.py")?
        );
        let held: Vec<Pin> = if historical_present {
            HISTORICAL
                .iter()
                .map(|relative| Pin::open(&root.join(relative), 1024 * 1024))
                .collect::<TestResult<_>>()?
        } else {
            Vec::new()
        };
        source_state(root, &original, historical_present, &held)?;
        macro_rules! case {
            ($name:literal, $run:expr) => {{
                if let Err(error) = $run {
                    errors.push(format!("{label}/{}: {error}", $name));
                }
                source_state(root, &original, historical_present, &held)?;
                source.verify()?;
                helper.verify()?;
                for elf in &elfs {
                    elf.verify()?;
                }
            }};
        }
        case!(
            "http",
            run_http_frozen(
                &HttpFrozenConfig {
                    repository: root,
                    source_server: &server,
                    candidate: &binary,
                    output: &lane.join("http")
                },
                |name| original.observation(name)
            )
        );
        case!(
            "terminal",
            run_terminal_frozen(
                &TerminalFrozenConfig {
                    repository: root,
                    source_server: &server,
                    candidate: &binary,
                    output: &lane.join("terminal")
                },
                |name| original.observation(name)
            )
        );
        case!(
            "lifecycle",
            run_terminal_lifecycle_frozen(
                &TerminalLifecycleConfig {
                    repository: root,
                    candidate: &binary,
                    output: &lane.join("lifecycle")
                },
                |name| original.observation(name)
            )
        );
        case!(
            "support",
            run_support_frozen(
                &SupportFrozenConfig {
                    repository: root,
                    source_server: &server,
                    candidate: &binary,
                    fixture_binary: &actions,
                    process_child: &child,
                    output: &lane.join("support")
                },
                |name| original.observation(name)
            )
        );
        case!(
            "shutdown",
            run_support_shutdown_frozen(
                &SupportShutdownConfig {
                    repository: root,
                    candidate: &binary,
                    output: &lane.join("shutdown")
                },
                |name| original.observation(name)
            )
        );
        case!(
            "actions",
            run_actions_frozen(
                &ActionsFrozenConfig {
                    repository: root,
                    source_server: &server,
                    repo_update: &repo_update,
                    candidate: &actions,
                    process_child: &child,
                    tmux: &tmux,
                    output: &lane.join("actions")
                },
                &original
            )
        );
        case!(
            "upload",
            run_upload_frozen(
                &UploadFrozenConfig {
                    repository: root,
                    source_server: &server,
                    candidate: &upload,
                    actions_binary: &actions,
                    process_child: &child,
                    output: &lane.join("upload")
                },
                |name| original.observation(name)
            )
        );
        case!(
            "cwp",
            run_cwp_frozen(
                &CwpFrozenConfig {
                    repository: root,
                    source_server: &server,
                    test_binary: &cwp,
                    output: &lane.join("cwp")
                },
                |name| original.observation(name)
            )
        );
    }
    fs::write(
        evidence.join("failures.json"),
        serde_json::to_vec_pretty(&errors)?,
    )?;
    if !errors.is_empty() {
        return Err(format!("{} independent recovery controls failed", errors.len()).into());
    }
    fs::write(
        evidence.join("admission.json"),
        serde_json::to_vec_pretty(
            &json!({"pass":true,"lanes":2,"controllers_per_lane":8,"historical_sources":HISTORICAL.len(),"same_elf_digests":elfs.iter().map(Pin::digest).collect::<Vec<_>>()}),
        )?,
    )?;
    Ok(())
}
