//! #419: exercise the real updater against distinct origin/rust Git histories.
use openpilot_updated::{
    agnos::Agnos,
    common,
    paths::Paths,
    process::Commands,
    updater::{Context, Updater},
    Error, Params,
};
use std::{path::Path, process::Command};

const ORIGIN_URL: &str = "https://github.com/fixture/original.git";
const RUST_URL: &str = "https://github.com/fixture/rust.git";

fn git_command(cwd: &Path) -> Command {
    let mut command = Command::new("git");
    command.current_dir(cwd);
    // Do not inherit developer/runner Git configuration or contact a network.
    for (key, _) in
        std::env::vars_os().filter(|(key, _)| key.as_encoded_bytes().starts_with(b"GIT_"))
    {
        command.env_remove(key);
    }
    command
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_ALLOW_PROTOCOL", "file");
    command
}

fn git(cwd: &Path, argv: &[&str]) -> String {
    let output = git_command(cwd).args(argv).output().unwrap();
    assert!(output.status.success(), "git {argv:?}: {output:?}");
    String::from_utf8(output.stdout).unwrap().trim().into()
}

#[derive(Default)]
struct FixtureCommands {
    transports: Vec<(String, String)>,
    calls: Vec<Vec<String>>,
}

impl Commands for FixtureCommands {
    fn run(&mut self, argv: &[String], cwd: Option<&Path>) -> Result<String, Error> {
        assert_eq!(argv[0], "git", "fixture must never install or flash");
        self.calls.push(argv.to_vec());
        let mut command = git_command(cwd.unwrap());
        if matches!(argv.get(1).map(String::as_str), Some("ls-remote" | "fetch")) {
            // Transport substitution ONLY: config/get-url retain supported URLs.
            // Real Git still selects the remote, advertises refs, fetches objects,
            // applies refspecs, and updates tracking config. No canned Git output.
            for (url, local) in &self.transports {
                command.args(["-c", &format!("url.{local}.insteadOf={url}")]);
            }
        }
        let output = command.args(&argv[1..]).output()?;
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

struct Fixture {
    _root: tempfile::TempDir,
    paths: Paths,
    origin: String,
    rust: String,
    origin_main: String,
    rust_main: String,
    dev_md: String,
    preview: String,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let origin = root.path().join("origin.git");
        let rust = root.path().join("rust.git");
        for bare in [&origin, &rust] {
            git(
                root.path(),
                &[
                    "init",
                    "--bare",
                    "--initial-branch=main",
                    bare.to_str().unwrap(),
                ],
            );
        }
        let seed = root.path().join("seed");
        git(
            root.path(),
            &["init", "--initial-branch=main", seed.to_str().unwrap()],
        );
        git(&seed, &["config", "user.name", "Fixture"]);
        git(&seed, &["config", "user.email", "fixture@example.invalid"]);
        let mut commits = Vec::new();
        for (content, remote, branch) in [
            ("original main", &origin, "main"),
            ("rust main", &rust, "main"),
            ("rust dev-md", &rust, "dev-md"),
            ("rust preview", &rust, "preview"),
        ] {
            std::fs::write(seed.join("source"), content).unwrap();
            git(&seed, &["add", "source"]);
            git(&seed, &["commit", "-qm", content]);
            commits.push(git(&seed, &["rev-parse", "HEAD"]));
            git(
                &seed,
                &[
                    "push",
                    remote.to_str().unwrap(),
                    &format!("HEAD:refs/heads/{branch}"),
                ],
            );
        }
        let paths = Paths {
            base: root.path().join("base"),
            staging: root.path().join("stage"),
            lock: root.path().join("lock"),
            system_root: root.path().into(),
        };
        std::fs::create_dir_all(&paths.staging).unwrap();
        for checkout in [&paths.base, &paths.merged()] {
            git(
                root.path(),
                &[
                    "clone",
                    "--no-local",
                    origin.to_str().unwrap(),
                    checkout.to_str().unwrap(),
                ],
            );
            git(checkout, &["remote", "set-url", "origin", ORIGIN_URL]);
            git(checkout, &["remote", "add", "rust", RUST_URL]);
        }
        Self {
            origin: origin.to_str().unwrap().into(),
            rust: rust.to_str().unwrap().into(),
            origin_main: commits[0].clone(),
            rust_main: commits[1].clone(),
            dev_md: commits[2].clone(),
            preview: commits[3].clone(),
            _root: root,
            paths,
        }
    }

    fn commands(&self) -> FixtureCommands {
        FixtureCommands {
            transports: vec![
                (ORIGIN_URL.into(), self.origin.clone()),
                (RUST_URL.into(), self.rust.clone()),
            ],
            ..FixtureCommands::default()
        }
    }

    fn config(&self, key: &str, value: &str) {
        for checkout in [&self.paths.base, &self.paths.merged()] {
            git(checkout, &["config", key, value]);
        }
    }

    fn branch(&self, branch: &str, remote: &str) {
        for checkout in [&self.paths.base, &self.paths.merged()] {
            git(checkout, &["checkout", "-B", branch]);
        }
        self.config(&format!("branch.{branch}.remote"), remote);
        self.config(
            &format!("branch.{branch}.merge"),
            &format!("refs/heads/{branch}"),
        );
    }

    fn with_updater(&self, commands: &mut dyn Commands, action: impl FnOnce(&mut Updater<'_>)) {
        let mut logger = openpilot_logging::producer::Factory::new(format!(
            "ipc://{}",
            self.paths.system_root.join("logs.sock").display()
        ))
        .unwrap()
        .logger();
        let mut agnos = NoFlash;
        let mut updater = Updater::new(
            Params::open(&self.paths.system_root).unwrap(),
            Context {
                paths: &self.paths,
                commands,
                hardware: &Hardware,
                agnos: &mut agnos,
                clock: &openpilot_timed::clock::SystemClock,
                logger: &mut logger,
                is_agnos: false,
            },
        );
        action(&mut updater);
    }

    fn assert_fetched(&self, remote: &str, branch: &str, commit: &str) {
        for checkout in [self.paths.merged(), self.paths.finalized()] {
            assert_eq!(git(&checkout, &["rev-parse", "HEAD"]), commit);
            assert_eq!(
                git(&checkout, &["rev-parse", "--abbrev-ref", "HEAD"]),
                branch
            );
            assert_eq!(
                git(&checkout, &["rev-parse", "--abbrev-ref", "@{u}"]),
                format!("{remote}/{branch}")
            );
        }
        assert!(common::get_consistent_flag(&self.paths.finalized()));
    }
}

struct Hardware;
impl openpilot_hardware_info::HardwareInfo for Hardware {
    fn get_device_type(&self) -> Result<String, openpilot_hardware_info::Error> {
        Ok("pc".into())
    }
}
struct NoFlash;
impl Agnos for NoFlash {
    fn get_target_slot_number(&mut self) -> Result<u32, Error> {
        panic!("unexpected OS operation")
    }
    fn flash_agnos_update(&mut self, _: &Path, _: u32) -> Result<(), Error> {
        panic!("unexpected OS operation")
    }
}

fn assert_discovery_remote(commands: &FixtureCommands, remote: &str) {
    let probes: Vec<_> = commands
        .calls
        .iter()
        .filter(|call| call.get(1).is_some_and(|arg| arg == "ls-remote"))
        .cloned()
        .collect();
    assert_eq!(
        probes,
        [
            vec!["git", "ls-remote", remote, "HEAD"],
            vec!["git", "ls-remote", "--heads", remote],
        ]
    );
}

#[test]
fn discovery_and_probe_explicitly_use_current_branch_remote() {
    let fixture = Fixture::new();
    fixture.branch("dev-md", "rust");
    let mut commands = fixture.commands();
    // The selected rust source is reachable even when the unrelated origin is not.
    commands.transports[0].1.push_str("/unavailable");
    fixture.with_updater(&mut commands, |updater| {
        updater.check_for_update().unwrap();
        assert!(updater.has_internet);
        assert_eq!(
            updater.branches["dev-md"].as_deref(),
            Some(fixture.dev_md.as_str())
        );
        assert_eq!(
            updater.branches["main"].as_deref(),
            Some(fixture.rust_main.as_str())
        );
        assert!(updater.update_available().unwrap());
    });
    assert_discovery_remote(&commands, "rust");
}

#[test]
fn fetch_uses_rust_when_origin_has_no_dev_md_and_preserves_origin_config() {
    let fixture = Fixture::new();
    fixture.branch("dev-md", "rust");
    // A narrow inherited refspec must be repaired on the selected remote only.
    fixture.config(
        "remote.rust.fetch",
        "+refs/heads/main:refs/remotes/rust/main",
    );
    fixture.config(
        "remote.origin.fetch",
        "+refs/heads/main:refs/remotes/origin/main",
    );
    let origin_fetch = git(
        &fixture.paths.merged(),
        &["config", "--get-all", "remote.origin.fetch"],
    );
    fixture.with_updater(&mut fixture.commands(), |updater| {
        updater.fetch_update().unwrap()
    });
    fixture.assert_fetched("rust", "dev-md", &fixture.dev_md);
    assert_eq!(
        git(
            &fixture.paths.merged(),
            &["config", "--get-all", "remote.origin.fetch"]
        ),
        origin_fetch
    );
    assert_eq!(
        git(&fixture.paths.merged(), &["remote", "get-url", "origin"]),
        ORIGIN_URL
    );
    assert_eq!(
        git(&fixture.paths.base, &["rev-parse", "HEAD"]),
        fixture.origin_main
    );
}

#[test]
fn ordinary_origin_tracking_still_discovers_and_fetches_origin() {
    let fixture = Fixture::new();
    let mut commands = fixture.commands();
    fixture.with_updater(&mut commands, |updater| {
        updater.check_for_update().unwrap();
        assert!(updater.has_internet);
        assert_eq!(updater.branches.len(), 1);
        assert_eq!(
            updater.branches["main"].as_deref(),
            Some(fixture.origin_main.as_str())
        );
        assert!(!updater.update_available().unwrap());
        updater.fetch_update().unwrap();
    });
    fixture.assert_fetched("origin", "main", &fixture.origin_main);
}

#[test]
fn absent_branch_remote_falls_back_to_origin_despite_other_remote_defaults() {
    let fixture = Fixture::new();
    for checkout in [&fixture.paths.base, &fixture.paths.merged()] {
        git(checkout, &["config", "--unset", "branch.main.remote"]);
    }
    fixture.config("remote.pushDefault", "rust");
    let mut commands = fixture.commands();
    fixture.with_updater(&mut commands, |updater| {
        updater.check_for_update().unwrap();
        assert_eq!(updater.branches.len(), 1);
        updater.fetch_update().unwrap();
    });
    fixture.assert_fetched("origin", "main", &fixture.origin_main);
}

#[test]
fn explicit_named_remote_and_supported_ssh_urls_are_preserved() {
    for url in [
        RUST_URL,
        "ssh://git@github.com/fixture/rust.git",
        "git@github.com:fixture/rust.git",
    ] {
        let fixture = Fixture::new();
        fixture.branch("dev-md", "rust");
        for checkout in [&fixture.paths.base, &fixture.paths.merged()] {
            git(checkout, &["remote", "rename", "rust", "release-source"]);
        }
        fixture.config("remote.release-source.url", url);
        let mut commands = fixture.commands();
        commands.transports.push((url.into(), fixture.rust.clone()));
        fixture.with_updater(&mut commands, |updater| {
            updater.check_for_update().unwrap();
            updater.fetch_update().unwrap();
        });
        fixture.assert_fetched("release-source", "dev-md", &fixture.dev_md);
        assert_eq!(
            git(
                &fixture.paths.merged(),
                &["remote", "get-url", "release-source"]
            ),
            url
        );
        assert_discovery_remote(&commands, "release-source");
    }
}

#[test]
fn requested_target_uses_current_branch_remote_then_tracks_that_remote() {
    let fixture = Fixture::new();
    fixture.branch("dev-md", "rust");
    // Target configuration must not override the CURRENT branch's source.
    fixture.config("branch.preview.remote", "origin");
    fixture.with_updater(&mut fixture.commands(), |updater| {
        updater
            .params
            .put("UpdaterTargetBranch", b"preview")
            .unwrap();
        updater.check_for_update().unwrap();
        updater.fetch_update().unwrap();
        updater.check_for_update().unwrap();
        assert!(!updater.update_available().unwrap());
    });
    fixture.assert_fetched("rust", "preview", &fixture.preview);
}

#[test]
fn same_updater_reselects_remote_after_current_branch_switches() {
    let fixture = Fixture::new();
    fixture.branch("dev-md", "rust");
    let mut commands = fixture.commands();
    fixture.with_updater(&mut commands, |updater| {
        updater.check_for_update().unwrap();
        assert!(updater.branches.contains_key("dev-md"));
        fixture.branch("main", "origin");
        updater.check_for_update().unwrap();
        assert_eq!(updater.branches.len(), 1, "stale rust refs must be cleared");
        assert_eq!(
            updater.branches["main"].as_deref(),
            Some(fixture.origin_main.as_str())
        );
        updater.fetch_update().unwrap();
    });
    fixture.assert_fetched("origin", "main", &fixture.origin_main);
    let second_check = FixtureCommands {
        calls: commands
            .calls
            .iter()
            .filter(|call| call.get(1).is_some_and(|arg| arg == "ls-remote"))
            .skip(2)
            .cloned()
            .collect(),
        ..FixtureCommands::default()
    };
    assert_discovery_remote(&second_check, "origin");
}

fn assert_rejected(configure: impl Fn(&Fixture)) {
    for fetch in [false, true] {
        let fixture = Fixture::new();
        configure(&fixture);
        let before = git(&fixture.paths.merged(), &["config", "--local", "--list"]);
        std::fs::create_dir_all(fixture.paths.finalized()).unwrap();
        std::fs::write(
            fixture.paths.finalized().join(".runtime-update-ready.json"),
            "ready",
        )
        .unwrap();
        common::set_consistent_flag(&fixture.paths, &fixture.paths.finalized(), true).unwrap();
        let mut commands = fixture.commands();
        fixture.with_updater(&mut commands, |updater| {
            updater.params.put_bool("UpdateAvailable", true).unwrap();
            let result = if fetch {
                updater.fetch_update()
            } else {
                updater.check_for_update()
            };
            assert!(
                result.is_err(),
                "invalid remote must be rejected, fetch={fetch}"
            );
            assert!(updater.params.boolean("UpdateAvailable").unwrap());
        });
        assert!(
            !commands
                .calls
                .iter()
                .any(|call| matches!(call.get(1).map(String::as_str), Some("ls-remote" | "fetch"))),
            "reject before remote I/O: {:?}",
            commands.calls
        );
        assert_eq!(
            git(&fixture.paths.merged(), &["rev-parse", "HEAD"]),
            fixture.origin_main
        );
        assert_eq!(
            git(&fixture.paths.merged(), &["config", "--local", "--list"]),
            before
        );
        assert!(common::get_consistent_flag(&fixture.paths.finalized()));
        assert_eq!(
            std::fs::read_to_string(fixture.paths.finalized().join(".runtime-update-ready.json"))
                .unwrap(),
            "ready"
        );
    }
}

#[test]
fn missing_configured_remote_does_not_fall_back_to_origin() {
    assert_rejected(|f| f.config("branch.main.remote", "missing"));
}

#[test]
fn configured_remote_without_url_is_rejected_before_remote_io() {
    assert_rejected(|f| {
        for checkout in [&f.paths.base, &f.paths.merged()] {
            git(checkout, &["config", "--unset", "remote.origin.url"]);
        }
    });
}

#[test]
fn empty_configured_remote_does_not_fall_back_to_origin() {
    assert_rejected(|f| f.config("branch.main.remote", ""));
}

#[test]
fn dot_remote_is_rejected_before_network_or_checkout_changes() {
    assert_rejected(|f| f.config("branch.main.remote", "."));
}

#[test]
fn missing_fallback_origin_is_rejected_before_remote_io() {
    assert_rejected(|f| {
        for checkout in [&f.paths.base, &f.paths.merged()] {
            git(checkout, &["remote", "remove", "origin"]);
        }
    });
}

#[test]
fn local_remote_urls_are_rejected_before_remote_io() {
    for kind in ["absolute", "relative", "file"] {
        assert_rejected(|f| {
            f.config(
                "remote.origin.url",
                &match kind {
                    "absolute" => f.origin.clone(),
                    "relative" => "../../origin.git".into(),
                    _ => format!("file://{}", f.origin),
                },
            )
        });
    }
}

#[test]
fn unsupported_remote_transports_are_rejected_before_remote_io() {
    for url in ["ext::false", "ftp://example.invalid/repo.git"] {
        assert_rejected(|f| f.config("remote.origin.url", url));
    }
}
