use openpilot_updated::{common, paths::Paths, process::Commands, Error};
use std::{path::Path, process::Command};

struct LocalCommands;

impl Commands for LocalCommands {
    fn run(&mut self, argv: &[String], cwd: Option<&Path>) -> Result<String, Error> {
        let output = Command::new(&argv[0])
            .args(&argv[1..])
            .current_dir(cwd.unwrap())
            .output()?;
        if !output.status.success() {
            return Err(Error::Command {
                command: argv.to_vec(),
                code: output.status.code().unwrap_or(-1),
                output: String::from_utf8(output.stderr)?,
            });
        }
        Ok(String::from_utf8(output.stdout)?)
    }
}

fn checkout(status: i32) -> (tempfile::TempDir, Paths) {
    let root = tempfile::tempdir().unwrap();
    let paths = Paths {
        base: root.path().join("base"),
        staging: root.path().join("stage"),
        lock: root.path().join("lock"),
        system_root: root.path().into(),
    };
    std::fs::create_dir_all(paths.finalized().join("rust/tools")).unwrap();
    std::fs::write(paths.finalized().join("rust/Cargo.toml"), "[workspace]\n").unwrap();
    std::fs::write(paths.finalized().join("rust/tools/ensure_runtime.sh"), format!("set -eu\n[ \"$1\" = --root ]\n[ \"$2\" = \"$(pwd)\" ]\ntouch package-attempted\nexit {status}\n")).unwrap();
    (root, paths)
}

#[test]
fn successful_package_install_allows_consistent_update() {
    let (_root, paths) = checkout(0);
    common::finalize_runtime_package(&paths, &mut LocalCommands).unwrap();
    assert!(paths.finalized().join("package-attempted").is_file());
    assert!(common::get_consistent_flag(&paths.finalized()));
}

#[test]
fn failed_package_install_clears_previous_consistent_marker() {
    let (_root, paths) = checkout(17);
    common::set_consistent_flag(&paths, &paths.finalized(), true).unwrap();
    let result = common::finalize_runtime_package(&paths, &mut LocalCommands);
    assert!(matches!(result, Err(Error::Command { code: 17, .. })));
    assert!(!common::get_consistent_flag(&paths.finalized()));
}

#[test]
fn rust_checkout_without_distribution_entry_is_not_activated() {
    let (_root, paths) = checkout(0);
    std::fs::remove_file(paths.finalized().join("rust/tools/ensure_runtime.sh")).unwrap();
    assert!(common::finalize_runtime_package(&paths, &mut LocalCommands).is_err());
    assert!(!common::get_consistent_flag(&paths.finalized()));
}

#[test]
fn original_checkout_without_rust_retains_original_update_path() {
    let (_root, paths) = checkout(0);
    std::fs::remove_dir_all(paths.finalized().join("rust")).unwrap();
    common::finalize_runtime_package(&paths, &mut LocalCommands).unwrap();
    assert!(common::get_consistent_flag(&paths.finalized()));
    assert!(!paths.finalized().join("package-attempted").exists());
}

struct Hardware;
impl openpilot_hardware_info::HardwareInfo for Hardware {
    fn get_device_type(&self) -> Result<String, openpilot_hardware_info::Error> {
        Ok("pc".into())
    }
}

struct NoFlash;
impl openpilot_updated::agnos::Agnos for NoFlash {
    fn get_target_slot_number(&mut self) -> Result<u32, Error> {
        Err(Error::Contract("unexpected OS operation"))
    }
    fn flash_agnos_update(&mut self, _: &Path, _: u32) -> Result<(), Error> {
        Err(Error::Contract("unexpected OS operation"))
    }
}

fn finalize_checkout(paths: &Paths) -> Result<(), Error> {
    std::fs::rename(paths.finalized(), paths.merged()).unwrap();
    let mut commands = LocalCommands;
    for arguments in [
        vec!["git", "init", "-q"],
        vec!["git", "add", "."],
        vec![
            "git",
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-qm",
            "fixture",
        ],
    ] {
        commands
            .run(
                &arguments.into_iter().map(String::from).collect::<Vec<_>>(),
                Some(&paths.merged()),
            )
            .unwrap();
    }
    run_updater(paths, false)
}

fn run_updater(paths: &Paths, initialize: bool) -> Result<(), Error> {
    with_commands(paths, &mut LocalCommands, |updater| {
        if initialize {
            updater.init_overlay()
        } else {
            updater.finalize_update()
        }
    })
}

fn with_commands(
    paths: &Paths,
    commands: &mut dyn Commands,
    action: impl FnOnce(&mut openpilot_updated::updater::Updater<'_>) -> Result<(), Error>,
) -> Result<(), Error> {
    let mut logger = openpilot_logging::producer::Factory::new(format!(
        "ipc://{}",
        paths.system_root.join("logs.sock").display()
    ))?
    .logger();
    let mut flash = NoFlash;
    let mut updater = openpilot_updated::updater::Updater::new(
        openpilot_updated::Params::open(&paths.system_root)?,
        openpilot_updated::updater::Context {
            paths,
            commands,
            hardware: &Hardware,
            agnos: &mut flash,
            clock: &openpilot_timed::clock::SystemClock,
            logger: &mut logger,
            is_agnos: false,
        },
    );
    action(&mut updater)
}

#[test]
fn complete_updater_finalization_runs_target_package_before_activation() {
    let (_root, paths) = checkout(0);
    finalize_checkout(&paths).unwrap();
    assert!(paths.finalized().join("package-attempted").is_file());
    assert!(common::get_consistent_flag(&paths.finalized()));
}

#[test]
fn complete_updater_finalization_rejects_unpublished_package() {
    let (_root, paths) = checkout(17);
    assert!(matches!(
        finalize_checkout(&paths),
        Err(Error::Command { code: 17, .. })
    ));
    assert!(!common::get_consistent_flag(&paths.finalized()));
}

#[test]
fn native_updater_stages_from_merged_source_without_copying_or_deleting_ready_update() {
    use std::os::unix::fs::PermissionsExt;
    let (_root, paths) = checkout(0);
    std::fs::create_dir_all(paths.base.join("rust/bin")).unwrap();
    std::fs::write(paths.base.join("rust/runtime-manifest.json"), "native").unwrap();
    let helper = paths.base.join("rust/bin/openpilot-runtime-install");
    std::fs::write(
        &helper,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\nexit 17\n",
            paths.base.join("attempt").display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o755)).unwrap();
    let result = finalize_checkout(&paths);
    assert!(matches!(result, Err(Error::Command { code: 17, .. })));
    let arguments = std::fs::read_to_string(paths.base.join("attempt")).unwrap();
    assert!(arguments.contains("--stage-update\n"));
    assert!(arguments.contains(&format!("--source\n{}\n", paths.merged().display())));
    assert!(!paths.finalized().exists());
}

#[test]
fn updater_does_not_erase_pending_native_update_or_previous_generation() {
    let (_root, paths) = checkout(0);
    std::fs::create_dir_all(&paths.base).unwrap();
    std::fs::write(
        paths.finalized().join(".runtime-update-ready.json"),
        "ready",
    )
    .unwrap();
    let backup = paths.staging.join("old_openpilot.saved");
    std::fs::create_dir_all(&backup).unwrap();
    std::fs::write(backup.join("live-helper"), "preserve").unwrap();
    common::set_consistent_flag(&paths, &paths.finalized(), true).unwrap();
    let mut commands = OverlayCommands::default();
    with_commands(&paths, &mut commands, |updater| updater.init_overlay()).unwrap();
    assert!(commands
        .calls
        .iter()
        .any(|args| args.iter().any(|arg| arg == "mount")));
    assert!(common::get_consistent_flag(&paths.finalized()));
    assert_eq!(
        std::fs::read_to_string(paths.finalized().join(".runtime-update-ready.json")).unwrap(),
        "ready"
    );
    assert_eq!(
        std::fs::read_to_string(backup.join("live-helper")).unwrap(),
        "preserve"
    );
}

#[derive(Default)]
struct OverlayCommands {
    calls: Vec<Vec<String>>,
}
impl Commands for OverlayCommands {
    fn run(&mut self, argv: &[String], _: Option<&Path>) -> Result<String, Error> {
        self.calls.push(argv.to_vec());
        if argv.first().is_some_and(|arg| arg == "git")
            && argv.get(1).is_some_and(|arg| arg == "fetch")
        {
            return Err(Error::Command {
                command: argv.to_vec(),
                code: 23,
                output: "owned fetch failure".into(),
            });
        }
        Ok(String::new())
    }
}

#[test]
fn failed_fetch_preserves_ready_update_and_its_available_status() {
    let (_root, paths) = checkout(0);
    std::fs::create_dir_all(&paths.base).unwrap();
    std::fs::create_dir_all(paths.merged()).unwrap();
    std::fs::write(
        paths.finalized().join(".runtime-update-ready.json"),
        "ready",
    )
    .unwrap();
    common::set_consistent_flag(&paths, &paths.finalized(), true).unwrap();
    let mut commands = OverlayCommands::default();
    let result = with_commands(&paths, &mut commands, |updater| {
        updater.params.put("UpdaterTargetBranch", b"dev-md")?;
        updater.params.put_bool("UpdateAvailable", true)?;
        updater.fetch_update()
    });
    assert!(matches!(result, Err(Error::Command { code: 23, .. })));
    assert!(common::get_consistent_flag(&paths.finalized()));
    assert_eq!(
        std::fs::read_to_string(paths.finalized().join(".runtime-update-ready.json")).unwrap(),
        "ready"
    );
    assert!(openpilot_updated::Params::open(&paths.system_root)
        .unwrap()
        .boolean("UpdateAvailable")
        .unwrap());
}

#[test]
fn interrupted_activation_still_requires_startup_recovery() {
    let (_root, paths) = checkout(0);
    std::fs::create_dir_all(&paths.base).unwrap();
    std::fs::write(
        paths.staging.join(".runtime-update-journal.json"),
        "journal",
    )
    .unwrap();
    let mut commands = OverlayCommands::default();
    assert!(with_commands(&paths, &mut commands, |updater| updater.init_overlay()).is_err());
    assert!(commands.calls.is_empty());
}
