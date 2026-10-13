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
            commands: &mut commands,
            hardware: &Hardware,
            agnos: &mut flash,
            clock: &openpilot_timed::clock::SystemClock,
            logger: &mut logger,
            is_agnos: false,
        },
    );
    updater.finalize_update()
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
