//! Real native process/lock/Git integration, with only package delivery failing
//! at an owned executable boundary. The installer has its own full archive tests.
use openpilot_carrot_server::{
    auto_update_pull::{Attempt, Effects, Pull},
    git_state::{Store, Time},
    git_status::Repository,
    Value,
};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
};

fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
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
struct Fixture {
    _root: tempfile::TempDir,
    repository: PathBuf,
    target: String,
    previous: String,
    lock: PathBuf,
    state: PathBuf,
    old_path: std::ffi::OsString,
}
impl Fixture {
    fn new(package_status: i32) -> Self {
        let root = tempfile::tempdir().unwrap();
        let repository = root.path().join("checkout");
        fs::create_dir(&repository).unwrap();
        git(&repository, &["init", "-q", "-b", "dev"]);
        git(
            &repository,
            &["config", "user.name", "Owned transaction fixture"],
        );
        git(
            &repository,
            &["config", "user.email", "fixture@example.invalid"],
        );
        fs::write(repository.join("tracked"), "old source").unwrap();
        git(&repository, &["add", "tracked"]);
        git(&repository, &["commit", "-qm", "old"]);
        let previous = git(&repository, &["rev-parse", "HEAD"]);
        fs::write(repository.join("tracked"), "new source").unwrap();
        git(&repository, &["commit", "-qam", "new"]);
        let target = git(&repository, &["rev-parse", "HEAD"]);
        git(&repository, &["branch", "candidate", &target]);
        git(
            &repository,
            &["update-ref", "refs/remotes/origin/candidate", &target],
        );
        git(&repository, &["reset", "--hard", &previous]);
        git(
            &repository,
            &[
                "remote",
                "add",
                "origin",
                "https://github.com/bin9208/openpilot-rust.git",
            ],
        );
        git(
            &repository,
            &["update-ref", "refs/remotes/origin/dev", &target],
        );
        git(&repository, &["config", "branch.dev.remote", "origin"]);
        git(
            &repository,
            &["config", "branch.dev.merge", "refs/heads/dev"],
        );
        fs::write(
            repository.join("tracked"),
            "user edit must survive preparation",
        )
        .unwrap();
        fs::create_dir_all(repository.join("rust/bin")).unwrap();
        fs::write(
            repository.join("rust/runtime-manifest.json"),
            "old native package",
        )
        .unwrap();
        let helper = repository.join("rust/bin/openpilot-runtime-install");
        fs::write(&helper, format!("#!/bin/sh\nset -eu\n[ \"$1\" = --stage-update ]\nprintf '%s\\n' \"$@\" > '{}'\nexit {package_status}\n", repository.join("package-attempt").display())).unwrap();
        fs::set_permissions(&helper, fs::Permissions::from_mode(0o755)).unwrap();
        let bin = root.path().join("owned-network-boundary");
        fs::create_dir(&bin).unwrap();
        fs::write(bin.join("git"), format!(r#"#!/bin/sh
if [ "$1" = fetch ]; then exit 0; fi
if [ "$1" = ls-remote ]; then printf '%s\trefs/heads/dev\n%s\trefs/heads/candidate\n' '{target}' '{target}'; exit 0; fi
exec /usr/bin/git "$@"
"#)).unwrap();
        fs::set_permissions(bin.join("git"), fs::Permissions::from_mode(0o755)).unwrap();
        let old_path = std::env::var_os("PATH").unwrap_or_default();
        let mut search = vec![bin];
        search.extend(std::env::split_paths(&old_path));
        std::env::set_var("PATH", std::env::join_paths(search).unwrap());
        Self {
            old_path,
            lock: root.path().join("repository.lock"),
            state: root.path().join("state"),
            _root: root,
            repository,
            target,
            previous,
        }
    }
    fn assert_untouched(&self) {
        assert_eq!(git(&self.repository, &["rev-parse", "HEAD"]), self.previous);
        assert_eq!(
            fs::read_to_string(self.repository.join("tracked")).unwrap(),
            "user edit must survive preparation"
        );
        assert_eq!(
            fs::read_to_string(self.repository.join("rust/runtime-manifest.json")).unwrap(),
            "old native package"
        );
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        std::env::set_var("PATH", &self.old_path);
    }
}
async fn manual_updates_preserve_active_pair() {
    use openpilot_carrot_server::tools::{
        config::{Config, Paths},
        service::Service,
    };
    for (action, fields) in [
        (
            "git_checkout",
            vec![
                ("branch", "candidate"),
                ("kind", "local"),
                ("name", "candidate"),
            ],
        ),
        ("git_reset", vec![("mode", "hard"), ("target", "candidate")]),
        ("git_reset_repo_checkout", vec![("branch", "candidate")]),
    ] {
        let fixture = Fixture::new(17);
        let root = fixture._root.path();
        let service = Service::new(Config {
            paths: Paths {
                repository: fixture.repository.clone(),
                lock: fixture.lock.clone(),
                launcher: std::env::current_exe().unwrap(),
                videos: root.join("videos"),
                logs: root.join("logs"),
                calibration: [root.join("calibration-a"), root.join("calibration-b")],
                tmux_log: root.join("tmux"),
                backup: root.join("backup"),
                history: fixture.state.join("tools.json"),
            },
            params: None,
            git_status: None,
            auto_update: None,
        });
        let mut body = vec![("action", Value::text(action))];
        body.extend(
            fields
                .into_iter()
                .map(|(key, value)| (key, Value::text(value))),
        );
        let body = Value::Object(
            body.into_iter()
                .map(|(key, value)| (key.chars().map(u32::from).collect(), value))
                .collect(),
        );
        let reply = service.sync(body).await.unwrap();
        assert!(
            !reply.value.get("ok").truth(),
            "package delivery failure reported success"
        );
        service.shutdown().await.unwrap();
        fixture.assert_untouched();
        assert!(
            fixture.repository.join("package-attempt").is_file(),
            "maintenance action bypassed native package transaction: {action}"
        );
        println!("PASS native manual update package failure {action}");
    }
}

async fn automatic_update_preserves_active_pair(package_status: i32) {
    let fixture = Fixture::new(package_status);
    let pull = Pull::new(
        Repository {
            directory: fixture.repository.clone(),
            lock: fixture.lock.clone(),
            launcher: std::env::current_exe().unwrap(),
        },
        Arc::new(Store::new(fixture.state.clone())),
        Effects {
            clock: Arc::new(|| Time {
                seconds: Value::integer(1700000000),
                nanoseconds: Value::integer(1700000000000000000_u64),
            }),
            alert: Arc::new(|_, _| Ok(())),
            notify: Arc::new(|_| Box::pin(async { Ok(()) })),
        },
    );
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&fixture.lock)
        .unwrap();
    lock.lock().unwrap();
    let (_sender, stopped) = tokio::sync::watch::channel(false);
    let result = pull
        .run(Attempt {
            target: &fixture.target,
            lock: Arc::new(lock),
            stopped,
        })
        .await
        .unwrap();
    fixture.assert_untouched();
    assert_eq!(
        result,
        if package_status == 0 {
            (true, true, fixture.target.clone())
        } else {
            (false, false, String::new())
        }
    );
    if package_status != 0 {
        let state = Store::new(fixture.state.clone()).auto_update();
        assert!(state.get("error_code").text_eq("runtime_package_failed"));
        assert!(!state.get("error").string().unwrap().is_empty());
        assert_eq!(state.get("stage_rc"), &Value::integer(package_status));
        assert_eq!(state.get("reset_rc"), &Value::Null);
    }
    let arguments = fs::read_to_string(fixture.repository.join("package-attempt")).unwrap();
    assert!(arguments.lines().any(|line| line == fixture.target));
    println!("PASS native automatic update package status {package_status}");
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    if let Some(descriptor) = std::env::args_os()
        .nth(1)
        .filter(|arg| Path::new(arg).is_file())
    {
        openpilot_process_supervision::run_child(fs::File::open(descriptor)?)?;
        return Ok(());
    }
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            manual_updates_preserve_active_pair().await;
            automatic_update_preserves_active_pair(17).await;
            automatic_update_preserves_active_pair(0).await;
        });
    Ok(())
}
