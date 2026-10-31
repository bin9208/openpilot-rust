//! Exercise the real logging/loggerd build scripts with offline std-only Cargo inputs.
use std::{fs, path::Path, process::Command};

fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args([
            "-c",
            "user.name=Owned source-stamp fixture",
            "-c",
            "user.email=source-stamp@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
        ])
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn build(manifest: &Path, target: &Path, home: &Path) -> (bool, String) {
    let output = Command::new(env!("CARGO"))
        .current_dir(manifest.parent().unwrap())
        .args(["build", "--locked", "--offline", "--message-format=json"])
        .arg("--manifest-path")
        .arg(manifest)
        .arg("--target-dir")
        .arg(target)
        .env("CARGO_HOME", home)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env_remove("OPENPILOT_NATIVE_INCREMENTAL")
        .env_remove("CARGO_BUILD_TARGET")
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let artifacts: Vec<serde_json::Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .filter(|row| {
            row["reason"] == "compiler-artifact"
                && row["target"]["name"] == "owned-source-stamp"
                && row["executable"].is_string()
        })
        .collect();
    assert_eq!(artifacts.len(), 1);
    let artifact = &artifacts[0];
    let output = Command::new(artifact["executable"].as_str().unwrap())
        .env_clear()
        .output()
        .unwrap();
    assert!(output.status.success());
    (
        artifact["fresh"].as_bool().unwrap(),
        String::from_utf8(output.stdout).unwrap(),
    )
}

#[test]
fn real_git_inputs_stay_fresh_and_packed_to_loose_refs_update_both_source_stamps() {
    for (name, script, prefix) in [
        ("logging", include_str!("../build.rs"), "OPENPILOT_LOGGING"),
        ("loggerd", include_str!("../../loggerd/build.rs"), "LOGGERD"),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("source");
        let package = root.join("rust/crates").join(name);
        fs::create_dir_all(package.join("src")).unwrap();
        fs::create_dir_all(root.join("openpilot/common")).unwrap();
        fs::write(
            root.join("openpilot/common/version.h"),
            "#define COMMA_VERSION \"owned\"\n",
        )
        .unwrap();
        fs::write(root.join("README.md"), "owned tracked input\n").unwrap();
        fs::write(package.join("build.rs"), script).unwrap();
        fs::write(
            package.join("Cargo.toml"),
            "[package]\nname='owned-source-stamp'\nversion='0.1.0'\nedition='2021'\n[workspace]\n",
        )
        .unwrap();
        fs::write(
            package.join("Cargo.lock"),
            "version = 4\n\n[[package]]\nname = \"owned-source-stamp\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        fs::write(package.join("src/main.rs"), format!(
            "fn main() {{ println!(\"{{}} {{}} {{}}\", env!(\"{prefix}_SOURCE_COMMIT\"), env!(\"{prefix}_SOURCE_TREE\"), env!(\"{prefix}_SOURCE_STAMP_MODE\")); }}\n"
        )).unwrap();
        git(
            &root,
            &["init", "--quiet", "--initial-branch=owned/nested/source"],
        );
        git(&root, &["add", "."]);
        git(&root, &["commit", "--quiet", "-m", "Owned initial inputs"]);
        let initial = git(&root, &["rev-parse", "HEAD"]);
        let target = temp.path().join("target");
        let home = temp.path().join("cargo-home");
        let check = |fresh, commit: &str, tree: &str| {
            let (actual, stamp) = build(&package.join("Cargo.toml"), &target, &home);
            assert_eq!(actual, fresh, "{name}: {stamp}");
            assert_eq!(stamp, format!("{commit} {tree} exact-source\n"), "{name}");
        };
        let packed = root.join(".git/packed-refs");
        let branch = root.join(".git/refs/heads/owned/nested/source");
        assert!(!packed.exists());
        check(false, &initial, "clean");
        check(true, &initial, "clean");
        assert!(!packed.exists()); // The build must not manufacture this input.

        git(&root, &["pack-refs", "--all", "--prune"]);
        assert!(packed.is_file() && !branch.exists());
        check(false, &initial, "clean");
        check(true, &initial, "clean");
        fs::write(root.join(".git/unrelated-observation"), "not a ref\n").unwrap();
        check(true, &initial, "clean"); // Do not recursively watch all of .git.

        let unchanged = [
            root.join(".git/HEAD"),
            root.join(".git/index"),
            packed.clone(),
        ]
        .map(|path| {
            (
                fs::read(&path).unwrap(),
                fs::metadata(path).unwrap().modified().unwrap(),
            )
        });
        let tree = git(&root, &["rev-parse", "HEAD^{tree}"]);
        let next = git(
            &root,
            &[
                "commit-tree",
                &tree,
                "-p",
                &initial,
                "-m",
                "Same tree, new commit",
            ],
        );
        git(
            &root,
            &["update-ref", "refs/heads/owned/nested/source", &next],
        );
        assert!(branch.is_file());
        assert_eq!(git(&root, &["rev-parse", "HEAD^{tree}"]), tree);
        for (path, before) in [root.join(".git/HEAD"), root.join(".git/index"), packed]
            .into_iter()
            .zip(unchanged)
        {
            assert_eq!(
                (
                    fs::read(&path).unwrap(),
                    fs::metadata(path).unwrap().modified().unwrap()
                ),
                before
            );
        }
        check(false, &next, "clean");
        check(true, &next, "clean");

        git(&root, &["update-ref", "--no-deref", "HEAD", &initial]);
        check(false, &initial, "clean");
        check(true, &initial, "clean");
        fs::write(root.join("README.md"), "owned changed tracked input\n").unwrap();
        check(false, &initial, "dirty");
        check(true, &initial, "dirty");
        git(&root, &["add", "README.md"]);
        check(false, &initial, "dirty");
        check(true, &initial, "dirty");
    }
}
