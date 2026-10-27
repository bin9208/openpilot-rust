//! Configuration and request drift must fail before an unverified candidate or
//! a fake controller can be used. Positive kernel/AGNOS cases run separately.
use openpilot_runtime_build::{
    generation::{Config, Stores},
    resources, Source,
};
use std::{collections::BTreeMap, fs, path::Path, process::Command};

fn config(root: &Path) -> Config {
    Config {
        schema: 1,
        stores: Stores {
            sources: root.join("sources"),
            objects: root.join("objects"),
            generations: root.join("generations"),
            params: root.join("params"),
        },
        cache: root.join("cache"),
        toolchain: root.join("toolchain"),
        build_plan: root.join("plan.json"),
        layout: root.join("layout.json"),
        resources: resources::Policy {
            schema: 1,
            memory_cgroup_parent: root.join("not-a-controller"),
            maximum_memory_bytes: 1,
            minimum_available_memory_bytes: 1,
            filesystems: BTreeMap::from([(
                root.to_owned(),
                resources::DiskBudget {
                    maximum_growth_bytes: 1,
                    minimum_free_bytes: 1,
                },
            )]),
        },
    }
}

#[test]
fn changed_configuration_is_rejected_by_both_real_entrypoints_before_any_scope_setup() {
    let temp = tempfile::tempdir().unwrap();
    let mut config = config(temp.path());
    let path = temp.path().join("config.json");
    fs::write(&path, serde_json::to_vec(&config).unwrap()).unwrap();
    let (_, pin) = Config::read_pinned(&path).unwrap();
    config.resources.maximum_memory_bytes += 1;
    fs::write(&path, serde_json::to_vec(&config).unwrap()).unwrap();
    fs::write(temp.path().join("previous.log"), b"previous normal log").unwrap();
    for (program, mode) in [
        (
            env!("CARGO_BIN_EXE_openpilot-native-build-scope"),
            "--verify",
        ),
        (env!("CARGO_BIN_EXE_openpilot-runtime-native"), "--build"),
    ] {
        let mut command = Command::new(program);
        command
            .arg("--config")
            .arg(&path)
            .arg("--configuration-sha256")
            .arg(&pin)
            .arg(mode);
        if mode == "--build" {
            command.arg("--root").arg(temp.path());
        }
        let output = command.output().unwrap();
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("configuration changed"),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!config.cache.exists());
        assert!(!config.stores.generations.exists());
        assert_eq!(
            fs::read(temp.path().join("previous.log")).unwrap(),
            b"previous normal log"
        );
    }
}

fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("/usr/bin/git")
        .arg("-C")
        .arg(root)
        .args([
            "-c",
            "user.name=Owned request fixture",
            "-c",
            "user.email=ci@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}

fn source(root: &Path) -> Source {
    Source {
        commit: git(root, &["rev-parse", "HEAD"]),
        tree: git(root, &["rev-parse", "HEAD^{tree}"]),
    }
}

#[test]
fn a_later_clean_commit_cannot_be_reported_as_the_original_build_request() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("source");
    fs::create_dir(&root).unwrap();
    git(&root, &["init", "--quiet"]);
    fs::write(root.join("README"), b"original request\n").unwrap();
    git(&root, &["add", "."]);
    git(&root, &["commit", "--quiet", "-m", "Original request"]);
    let requested = source(&root);
    fs::write(root.join("README"), b"later clean request\n").unwrap();
    git(&root, &["add", "."]);
    git(&root, &["commit", "--quiet", "-m", "Later request"]);
    let later = source(&root);
    assert_ne!(requested, later);
    assert!(git(&root, &["status", "--porcelain", "--untracked-files=all"]).is_empty());
    let config = config(temp.path());
    let path = temp.path().join("config.json");
    fs::write(&path, serde_json::to_vec(&config).unwrap()).unwrap();
    let (_, pin) = Config::read_pinned(&path).unwrap();
    let attempt = temp.path().join("owned-attempt");
    fs::create_dir(&attempt).unwrap();
    fs::write(
        attempt.join("candidate.json"),
        serde_json::to_vec(&serde_json::json!({
            "schema":1, "configuration_sha256":pin, "generation_id":"a".repeat(64), "source":later
        }))
        .unwrap(),
    )
    .unwrap();
    fs::write(temp.path().join("previous.log"), b"previous normal log").unwrap();
    let error = resources::candidate(&config, &path, &root, &attempt, &pin, &requested)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("candidate differs from its source/configuration"),
        "{error}"
    );
    assert!(!config.stores.generations.exists());
    assert_eq!(
        fs::read(temp.path().join("previous.log")).unwrap(),
        b"previous normal log"
    );
}
