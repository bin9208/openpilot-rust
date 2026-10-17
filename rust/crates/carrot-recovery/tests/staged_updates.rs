use std::{
    fs::{self, File},
    os::unix::fs::PermissionsExt,
    path::Path,
    process::Command,
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
fn main() {
    let argv: Vec<_> = std::env::args_os().skip(1).collect();
    if argv.first().is_some_and(|arg| Path::new(arg).is_file()) {
        openpilot_process_supervision::run_child(File::open(&argv[0]).unwrap()).unwrap();
        return;
    }
    if let Some(code) = openpilot_carrot_recovery::actions::internal_command(&argv).unwrap() {
        std::process::exit(code);
    }
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("checkout");
    fs::create_dir(&root).unwrap();
    git(&root, &["init", "-q", "-b", "dev"]);
    git(&root, &["config", "user.name", "Fixture"]);
    git(&root, &["config", "user.email", "fixture@example.invalid"]);
    fs::write(root.join("source"), "old").unwrap();
    git(&root, &["add", "."]);
    git(&root, &["commit", "-qm", "old"]);
    let old = git(&root, &["rev-parse", "HEAD"]);
    fs::write(root.join("source"), "new").unwrap();
    git(&root, &["commit", "-qam", "new"]);
    let target = git(&root, &["rev-parse", "HEAD"]);
    git(&root, &["branch", "candidate"]);
    let remote = temp.path().join("remote.git");
    git(
        temp.path(),
        &[
            "clone",
            "--bare",
            root.to_str().unwrap(),
            remote.to_str().unwrap(),
        ],
    );
    git(&root, &["reset", "--hard", &old]);
    git(
        &root,
        &[
            "remote",
            "add",
            "origin",
            "https://example.invalid/my/runtime.git",
        ],
    );
    // Explicit factory reset selects the default repository; keep that fetch
    // inside the same owned bare repository too.
    git(
        &root,
        &[
            "config",
            "--add",
            &format!("url.file://{}.insteadOf", remote.display()),
            "https://github.com/bin9208/openpilot-rust.git",
        ],
    );
    git(
        &root,
        &[
            "config",
            "--add",
            &format!("url.file://{}.insteadOf", remote.display()),
            "https://example.invalid/my/runtime.git",
        ],
    );
    fs::write(root.join("source"), "user edit").unwrap();
    fs::create_dir_all(root.join("rust/bin")).unwrap();
    fs::write(root.join("rust/runtime-manifest.json"), "old package").unwrap();
    let helper = root.join("rust/bin/openpilot-runtime-install");
    fs::write(
        &helper,
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > package-attempt\nexit 17\n",
    )
    .unwrap();
    fs::set_permissions(&helper, fs::Permissions::from_mode(0o755)).unwrap();
    let bin = temp.path().join("bin");
    fs::create_dir(&bin).unwrap();
    let executable = bin.join("recovery");
    fs::copy(std::env::current_exe().unwrap(), &executable).unwrap();
    fs::hard_link(&executable, bin.join("openpilot-process-child")).unwrap();
    for (action, value) in [
        ("__stage-repair", None),
        ("__stage-checkout", Some("candidate")),
        ("__stage-commit", Some(target.as_str())),
        ("__stage-reset-repo", Some("candidate")),
    ] {
        let mut command = Command::new(&executable);
        command.args(["__repo-update", "--repo", root.to_str().unwrap(), action]);
        if let Some(value) = value {
            command.arg(value);
        }
        let output = command
            .env("CARROT_REPO_LOCK_PATH", temp.path().join("lock"))
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(17),
            "{action}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(git(&root, &["rev-parse", "HEAD"]), old);
        assert_eq!(
            fs::read_to_string(root.join("source")).unwrap(),
            "user edit"
        );
        assert_eq!(
            fs::read_to_string(root.join("rust/runtime-manifest.json")).unwrap(),
            "old package"
        );
        assert!(fs::read_to_string(root.join("package-attempt"))
            .unwrap()
            .starts_with("--stage-update\n"));
        println!("PASS recovery package failure preserves active pair {action}");
    }
}
