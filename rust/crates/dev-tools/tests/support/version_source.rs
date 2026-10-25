//! Mechanical fixture operations copied from the original Version checker schedule.
//! No candidate crate, expected-value calculation, Python execution, or process-wide env/cwd edits.
use openpilot_dev_tools::qa_process::{CommandSpec, NativeRunner, Runner};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    time::Duration,
};

pub fn environment() -> BTreeMap<OsString, OsString> {
    let mut env: BTreeMap<_, _> = std::env::vars_os().collect();
    for (name, value) in [
        ("GIT_CONFIG_GLOBAL", "/dev/null"),
        ("GIT_CONFIG_SYSTEM", "/dev/null"),
        ("GIT_CONFIG_NOSYSTEM", "1"),
        ("GIT_AUTHOR_DATE", "2020-01-02T03:04:05+00:00"),
        ("GIT_COMMITTER_DATE", "2020-01-02T03:04:05+00:00"),
    ] {
        env.insert(name.into(), value.into());
    }
    env
}

pub fn git(path: &Path, args: &[&str]) -> Vec<u8> {
    let mut spec = CommandSpec::new_os(
        std::iter::once(OsString::from("git"))
            .chain(args.iter().map(|arg| OsString::from(*arg)))
            .collect(),
        Duration::from_secs(30),
    );
    spec.cwd = Some(path.into());
    spec.set_os_env(environment());
    let result = NativeRunner.run(&spec).unwrap();
    assert_eq!(
        result.code,
        0,
        "Git fixture {args:?}: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    result.stdout
}

pub fn repo(path: &Path, tracking: bool, origin: bool) {
    fs::create_dir(path).unwrap();
    git(path, &["init", "-b", "main"]);
    for (key, value) in [
        ("user.name", "Version fixture"),
        ("user.email", "version@example.invalid"),
        ("commit.gpgsign", "false"),
        ("core.hooksPath", "/dev/null"),
    ] {
        git(path, &["config", key, value]);
    }
    fs::create_dir_all(path.join("openpilot/common")).unwrap();
    fs::write(
        path.join("openpilot/common/version.h"),
        b"#define COMMA_VERSION \"1.2.3-test\"\n",
    )
    .unwrap();
    fs::write(
        path.join("RELEASES.md"),
        b"First line\nsecond line\n\nnext release\n",
    )
    .unwrap();
    fs::write(path.join("tracked"), b"original\n").unwrap();
    git(path, &["add", "."]);
    git(path, &["commit", "-m", "fixture"]);
    if origin {
        git(
            path,
            &[
                "remote",
                "add",
                "origin",
                "git@github.com:commaai/openpilot.git",
            ],
        );
    }
    if tracking {
        assert!(origin);
        git(path, &["update-ref", "refs/remotes/origin/main", "HEAD"]);
        git(path, &["branch", "--set-upstream-to=origin/main"]);
    }
}

fn corrupt_origin(path: &Path) {
    let file = path.join(".git/config");
    let original = fs::read(&file).unwrap();
    let needle = b"git@github.com:commaai/openpilot.git";
    let index = original
        .windows(needle.len())
        .position(|v| v == needle)
        .unwrap();
    let mut data = original[..index].to_vec();
    data.extend_from_slice(b"https://bad\xff.invalid/repo");
    data.extend_from_slice(&original[index + needle.len()..]);
    fs::write(file, data).unwrap();
}

/// Sets up one original worker's initial cwd and environment. Other operations
/// occur at their original call boundaries in before_call().
pub fn scenario(root: &Path, name: &str) -> (PathBuf, BTreeMap<OsString, OsString>) {
    let mut env = environment();
    let cwd = match name {
        "dirty-process-cwd" => {
            repo(&root.join("cwd-oddity"), true, true);
            let target = root.join("prebuilt-without-git");
            fs::create_dir(&target).unwrap();
            fs::write(target.join("prebuilt"), []).unwrap();
            root.join("cwd-oddity")
        }
        "dirty-cwd-missing-origin" => {
            fs::write(root.join("cwd-oddity/prebuilt"), []).unwrap();
            root.join("prebuilt-without-git")
        }
        "cache-and-cwd" => {
            repo(&root.join("cache"), true, true);
            repo(&root.join("elsewhere"), false, false);
            root.join("cache")
        }
        "empty-default-cache" => {
            repo(&root.join("empty-cache"), false, false);
            root.join("empty-cache")
        }
        "dirty-exception-cache" => {
            repo(&root.join("dirty-exception"), true, true);
            corrupt_origin(&root.join("dirty-exception"));
            root.join("dirty-exception")
        }
        "boundaries" => {
            repo(&root.join(name), true, true);
            fs::create_dir(root.join("empty")).unwrap();
            root.join(name)
        }
        "build-json" => {
            repo(&root.join("json"), true, true);
            root.join("json")
        }
        "missing-executable" => {
            repo(&root.join(name), true, true);
            fs::create_dir(root.join("bin")).unwrap();
            env.insert("PATH".into(), root.join("bin").into_os_string());
            root.join(name)
        }
        _ => {
            let path = root.join(name);
            repo(
                &path,
                !["prebuilt", "nontracking", "missingorigin"].contains(&name),
                name != "missingorigin",
            );
            match name {
                "dirty" | "staged" => {
                    fs::write(path.join("tracked"), b"modified\n").unwrap();
                    if name == "staged" {
                        git(&path, &["add", "tracked"]);
                    }
                }
                "untracked" => fs::write(path.join("untracked"), b"ignored by diff").unwrap(),
                "prebuilt" => fs::write(path.join("prebuilt"), []).unwrap(),
                "detached" => {
                    git(&path, &["checkout", "--detach", "HEAD"]);
                }
                "trackingremote" => {
                    git(
                        &path,
                        &[
                            "remote",
                            "add",
                            "upstream",
                            "https://example.invalid/fork.git",
                        ],
                    );
                    git(&path, &["update-ref", "refs/remotes/upstream/main", "HEAD"]);
                    git(&path, &["branch", "--set-upstream-to=upstream/main"]);
                }
                _ => {}
            }
            path
        }
    };
    (cwd, env)
}

pub fn before_call(root: &Path, name: &str, index: usize) {
    let boundary = root.join("boundaries");
    match (name, index) {
        ("cache-and-cwd", 4) => {
            let p = root.join("cache");
            fs::write(p.join("tracked"), b"changed\n").unwrap();
            fs::write(p.join("prebuilt"), []).unwrap();
            git(&p, &["add", "."]);
            git(&p, &["commit", "-m", "second"]);
            git(&p, &["branch", "missing"]);
            git(
                &p,
                &[
                    "remote",
                    "set-url",
                    "origin",
                    "https://changed.invalid/repo.git",
                ],
            );
        }
        ("empty-default-cache", 1) => {
            git(
                &root.join("empty-cache"),
                &["remote", "add", "origin", "https://added.invalid/repo"],
            );
        }
        ("dirty-exception-cache", 1) => {
            git(
                &root.join("dirty-exception"),
                &["remote", "set-url", "origin", "https://fixed.invalid/repo"],
            );
        }
        ("boundaries", 4) => repo(&root.join("not-yet-present"), true, true),
        ("boundaries", 5) => corrupt_origin(&boundary),
        ("boundaries", 7) => {
            git(
                &boundary,
                &[
                    "remote",
                    "set-url",
                    "origin",
                    "https://recovered.invalid/repo",
                ],
            );
        }
        ("boundaries", 9) => {
            fs::write(boundary.join("RELEASES.md"), b"one\r\ntwo\r\n\r\nsecond").unwrap()
        }
        ("boundaries", 10) => {
            fs::write(boundary.join("openpilot/common/version.h"), b"bad\xff\"1\"").unwrap()
        }
        ("boundaries", 11) => fs::write(
            boundary.join("openpilot/common/version.h"),
            b"no quoted version",
        )
        .unwrap(),
        ("boundaries", 12) => fs::write(
            boundary.join("openpilot/common/version.h"),
            b"before \"a\" after \"b\"",
        )
        .unwrap(),
        ("boundaries", 13) => {
            git(
                &boundary,
                &[
                    "worktree",
                    "add",
                    "--detach",
                    root.join("gitfile").to_str().unwrap(),
                    "HEAD",
                ],
            );
        }
        ("boundaries", 14) => {
            fs::create_dir(root.join("unreadable")).unwrap();
            fs::set_permissions(root.join("unreadable"), fs::Permissions::from_mode(0o0)).unwrap();
        }
        ("boundaries", 15) => {
            fs::set_permissions(root.join("unreadable"), fs::Permissions::from_mode(0o700))
                .unwrap();
            fs::write(root.join("empty/build.json"), b"{}").unwrap();
        }
        ("boundaries", 16) => {
            fs::remove_file(root.join("empty/build.json")).unwrap();
            std::os::unix::fs::symlink("build.json", root.join("empty/build.json")).unwrap();
        }
        ("build-json", 0) => fs::write(root.join("json/build.json"), b"{}").unwrap(),
        ("build-json", 1) => fs::write(
            root.join("json/build.json"),
            b"{\"channel\":\"nightly\",\"openpilot\":{\"version\":\"fresh\"}}",
        )
        .unwrap(),
        ("build-json", 2) => fs::write(root.join("json/build.json"), b"{").unwrap(),
        ("build-json", 3) => fs::write(root.join("json/build.json"), b"\xff").unwrap(),
        ("build-json", 4) => {
            fs::remove_file(root.join("json/build.json")).unwrap();
            fs::create_dir(root.join("json/build.json")).unwrap();
        }
        ("missing-executable", 1) => {
            let paths = std::env::var_os("PATH").unwrap();
            let executable = std::env::split_paths(&paths)
                .map(|p| p.join("git"))
                .find(|p| p.is_file())
                .unwrap();
            std::os::unix::fs::symlink(executable.canonicalize().unwrap(), root.join("bin/git"))
                .unwrap();
        }
        _ => {}
    }
}
