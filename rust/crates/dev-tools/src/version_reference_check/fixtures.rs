//! Original mechanical fixture mutations. No expected responses or candidate imports.
use super::process::Cancellation;
use crate::{
    qa_process::{CommandSpec, NativeRunner},
    seal::Result,
};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

pub(super) fn environment(base: &BTreeMap<OsString, OsString>) -> BTreeMap<OsString, OsString> {
    let mut env = base.clone();
    for (key, value) in [
        ("GIT_CONFIG_GLOBAL", "/dev/null"),
        ("GIT_CONFIG_SYSTEM", "/dev/null"),
        ("GIT_CONFIG_NOSYSTEM", "1"),
        ("GIT_AUTHOR_DATE", "2020-01-02T03:04:05+00:00"),
        ("GIT_COMMITTER_DATE", "2020-01-02T03:04:05+00:00"),
    ] {
        env.insert(key.into(), value.into());
    }
    env
}
fn git(
    path: &Path,
    args: &[&str],
    env: &BTreeMap<OsString, OsString>,
    cancel: &Cancellation,
) -> Result<Vec<u8>> {
    let mut spec = CommandSpec::new_os(
        std::iter::once(OsString::from("git"))
            .chain(args.iter().map(|arg| OsString::from(*arg)))
            .collect(),
        Duration::from_secs(30),
    );
    spec.cwd = Some(path.into());
    spec.stdout_limit = usize::MAX;
    spec.stderr_limit = usize::MAX;
    spec.set_os_env(env.clone());
    spec.env_remove = std::env::vars_os()
        .filter_map(|(key, _)| (!env.contains_key(&key)).then_some(key))
        .collect();
    let result = NativeRunner.run_controlled(
        &spec,
        Instant::now() + Duration::from_secs(30),
        &cancel.flag(),
    )?;
    if result.code != 0 {
        return Err(format!(
            "Version Git fixture {args:?}: {}",
            String::from_utf8_lossy(&result.stderr)
        )
        .into());
    }
    Ok(result.stdout)
}
fn repo(
    path: &Path,
    tracking: bool,
    origin: bool,
    env: &BTreeMap<OsString, OsString>,
    cancel: &Cancellation,
) -> Result<()> {
    fs::create_dir(path)?;
    git(path, &["init", "-b", "main"], env, cancel)?;
    for (key, value) in [
        ("user.name", "Version fixture"),
        ("user.email", "version@example.invalid"),
        ("commit.gpgsign", "false"),
        ("core.hooksPath", "/dev/null"),
    ] {
        git(path, &["config", key, value], env, cancel)?;
    }
    fs::create_dir_all(path.join("openpilot/common"))?;
    fs::write(
        path.join("openpilot/common/version.h"),
        b"#define COMMA_VERSION \"1.2.3-test\"\n",
    )?;
    fs::write(
        path.join("RELEASES.md"),
        b"First line\nsecond line\n\nnext release\n",
    )?;
    fs::write(path.join("tracked"), b"original\n")?;
    git(path, &["add", "."], env, cancel)?;
    git(path, &["commit", "-m", "fixture"], env, cancel)?;
    if origin {
        git(
            path,
            &[
                "remote",
                "add",
                "origin",
                "git@github.com:commaai/openpilot.git",
            ],
            env,
            cancel,
        )?;
    }
    if tracking {
        if !origin {
            return Err("tracking fixture requires origin".into());
        }
        git(
            path,
            &["update-ref", "refs/remotes/origin/main", "HEAD"],
            env,
            cancel,
        )?;
        git(
            path,
            &["branch", "--set-upstream-to=origin/main"],
            env,
            cancel,
        )?;
    }
    Ok(())
}
fn corrupt_origin(path: &Path) -> Result<()> {
    let config = path.join(".git/config");
    let original = fs::read(&config)?;
    let needle = b"git@github.com:commaai/openpilot.git";
    let at = original
        .windows(needle.len())
        .position(|v| v == needle)
        .ok_or("fixture origin absent")?;
    let mut bytes = original[..at].to_vec();
    bytes.extend_from_slice(b"https://bad\xff.invalid/repo");
    bytes.extend_from_slice(&original[at + needle.len()..]);
    fs::write(config, bytes)?;
    Ok(())
}
pub(super) fn scenario(
    root: &Path,
    name: &str,
    base: &BTreeMap<OsString, OsString>,
    cancel: &Cancellation,
) -> Result<(PathBuf, BTreeMap<OsString, OsString>)> {
    let mut env = environment(base);
    let cwd = match name {
        "dirty-process-cwd" => {
            repo(&root.join("cwd-oddity"), true, true, &env, cancel)?;
            fs::create_dir(root.join("prebuilt-without-git"))?;
            fs::write(root.join("prebuilt-without-git/prebuilt"), [])?;
            root.join("cwd-oddity")
        }
        "dirty-cwd-missing-origin" => {
            fs::write(root.join("cwd-oddity/prebuilt"), [])?;
            root.join("prebuilt-without-git")
        }
        "cache-and-cwd" => {
            repo(&root.join("cache"), true, true, &env, cancel)?;
            repo(&root.join("elsewhere"), false, false, &env, cancel)?;
            root.join("cache")
        }
        "empty-default-cache" => {
            repo(&root.join("empty-cache"), false, false, &env, cancel)?;
            root.join("empty-cache")
        }
        "dirty-exception-cache" => {
            repo(&root.join("dirty-exception"), true, true, &env, cancel)?;
            corrupt_origin(&root.join("dirty-exception"))?;
            root.join("dirty-exception")
        }
        "boundaries" => {
            repo(&root.join(name), true, true, &env, cancel)?;
            fs::create_dir(root.join("empty"))?;
            root.join(name)
        }
        "build-json" => {
            repo(&root.join("json"), true, true, &env, cancel)?;
            root.join("json")
        }
        "missing-executable" => {
            repo(&root.join(name), true, true, &env, cancel)?;
            fs::create_dir(root.join("bin"))?;
            env.insert("PATH".into(), root.join("bin").into_os_string());
            root.join(name)
        }
        _ => {
            let path = root.join(name);
            repo(
                &path,
                !["prebuilt", "nontracking", "missingorigin"].contains(&name),
                name != "missingorigin",
                &env,
                cancel,
            )?;
            match name {
                "dirty" | "staged" => {
                    fs::write(path.join("tracked"), b"modified\n")?;
                    if name == "staged" {
                        git(&path, &["add", "tracked"], &env, cancel)?;
                    }
                }
                "untracked" => fs::write(path.join("untracked"), b"ignored by diff")?,
                "prebuilt" => fs::write(path.join("prebuilt"), [])?,
                "detached" => {
                    git(&path, &["checkout", "--detach", "HEAD"], &env, cancel)?;
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
                        &env,
                        cancel,
                    )?;
                    git(
                        &path,
                        &["update-ref", "refs/remotes/upstream/main", "HEAD"],
                        &env,
                        cancel,
                    )?;
                    git(
                        &path,
                        &["branch", "--set-upstream-to=upstream/main"],
                        &env,
                        cancel,
                    )?;
                }
                _ => {}
            }
            path
        }
    };
    Ok((cwd, env))
}
pub(super) fn before_call(
    root: &Path,
    name: &str,
    index: usize,
    base: &BTreeMap<OsString, OsString>,
    cancel: &Cancellation,
) -> Result<()> {
    let env = environment(base);
    let boundary = root.join("boundaries");
    match (name, index) {
        ("cache-and-cwd", 4) => {
            let p = root.join("cache");
            fs::write(p.join("tracked"), b"changed\n")?;
            fs::write(p.join("prebuilt"), [])?;
            for args in [
                &["add", "."][..],
                &["commit", "-m", "second"],
                &["branch", "missing"],
                &[
                    "remote",
                    "set-url",
                    "origin",
                    "https://changed.invalid/repo.git",
                ],
            ] {
                git(&p, args, &env, cancel)?;
            }
        }
        ("empty-default-cache", 1) => {
            git(
                &root.join("empty-cache"),
                &["remote", "add", "origin", "https://added.invalid/repo"],
                &env,
                cancel,
            )?;
        }
        ("dirty-exception-cache", 1) => {
            git(
                &root.join("dirty-exception"),
                &["remote", "set-url", "origin", "https://fixed.invalid/repo"],
                &env,
                cancel,
            )?;
        }
        ("boundaries", 4) => repo(&root.join("not-yet-present"), true, true, &env, cancel)?,
        ("boundaries", 5) => corrupt_origin(&boundary)?,
        ("boundaries", 7) => {
            git(
                &boundary,
                &[
                    "remote",
                    "set-url",
                    "origin",
                    "https://recovered.invalid/repo",
                ],
                &env,
                cancel,
            )?;
        }
        ("boundaries", 9) => fs::write(boundary.join("RELEASES.md"), b"one\r\ntwo\r\n\r\nsecond")?,
        ("boundaries", 10) => {
            fs::write(boundary.join("openpilot/common/version.h"), b"bad\xff\"1\"")?
        }
        ("boundaries", 11) => fs::write(
            boundary.join("openpilot/common/version.h"),
            b"no quoted version",
        )?,
        ("boundaries", 12) => fs::write(
            boundary.join("openpilot/common/version.h"),
            b"before \"a\" after \"b\"",
        )?,
        ("boundaries", 13) => {
            git(
                &boundary,
                &[
                    "worktree",
                    "add",
                    "--detach",
                    root.join("gitfile")
                        .to_str()
                        .ok_or("non-Unicode fixture path")?,
                    "HEAD",
                ],
                &env,
                cancel,
            )?;
        }
        ("boundaries", 14) => {
            fs::create_dir(root.join("unreadable"))?;
            fs::set_permissions(root.join("unreadable"), fs::Permissions::from_mode(0o0))?;
        }
        ("boundaries", 15) => {
            fs::set_permissions(root.join("unreadable"), fs::Permissions::from_mode(0o700))?;
            fs::write(root.join("empty/build.json"), b"{}")?;
        }
        ("boundaries", 16) => {
            fs::remove_file(root.join("empty/build.json"))?;
            std::os::unix::fs::symlink("build.json", root.join("empty/build.json"))?;
        }
        ("build-json", 0) => fs::write(root.join("json/build.json"), b"{}")?,
        ("build-json", 1) => fs::write(
            root.join("json/build.json"),
            b"{\"channel\":\"nightly\",\"openpilot\":{\"version\":\"fresh\"}}",
        )?,
        ("build-json", 2) => fs::write(root.join("json/build.json"), b"{")?,
        ("build-json", 3) => fs::write(root.join("json/build.json"), b"\xff")?,
        ("build-json", 4) => {
            fs::remove_file(root.join("json/build.json"))?;
            fs::create_dir(root.join("json/build.json"))?;
        }
        ("missing-executable", 1) => {
            let paths = base
                .get(&OsString::from("PATH"))
                .ok_or("fixture PATH absent")?;
            let executable = std::env::split_paths(paths)
                .map(|p| p.join("git"))
                .find(|p| p.is_file())
                .ok_or("fixture Git executable absent")?;
            std::os::unix::fs::symlink(executable.canonicalize()?, root.join("bin/git"))?;
        }
        _ => {}
    }
    Ok(())
}
