use openpilot_runtime_launcher::native::Context;
use std::{fs, process::Command};

#[test]
fn incomplete_native_context_is_rejected_before_startup() {
    assert!(Context::from_arguments(None, None, None).unwrap().is_none());
    assert!(Context::from_arguments(Some("/missing-config".into()), None, None).is_err());
    assert!(Context::from_arguments(
        None,
        Some("/requested-checkout".into()),
        Some("a".repeat(64))
    )
    .is_err());
}

#[test]
fn changed_configuration_is_rejected_at_the_actual_launch_handoff() {
    use openpilot_runtime_build::generation::{Config, Stores};
    use openpilot_runtime_build::resources::{DiskBudget, Policy};
    use std::collections::BTreeMap;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let mut config = Config {
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
        resources: Policy {
            schema: 1,
            memory_cgroup_parent: root.join("not-a-controller"),
            maximum_memory_bytes: 1,
            minimum_available_memory_bytes: 1,
            filesystems: BTreeMap::from([(
                root.to_owned(),
                DiskBudget {
                    maximum_growth_bytes: 1,
                    minimum_free_bytes: 1,
                },
            )]),
        },
    };
    let path = root.join("config.json");
    fs::write(&path, serde_json::to_vec(&config).unwrap()).unwrap();
    let (_, pin) = Config::read_pinned(&path).unwrap();
    config.cache = root.join("changed-cache");
    fs::write(&path, serde_json::to_vec(&config).unwrap()).unwrap();
    fs::write(root.join("previous.log"), b"previous normal runtime log").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_openpilot-launch"))
        .env("OPENPILOT_NATIVE_LAUNCH_CONFIG_SHA256", pin)
        .arg("--root")
        .arg(root)
        .arg("--native-config")
        .arg(path)
        .arg("--update-checkout")
        .arg(root)
        .arg("--generation")
        .arg("a".repeat(64))
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("configuration changed before handoff"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!config.cache.exists());
    assert!(!config.stores.generations.exists());
    assert_eq!(
        fs::read(root.join("previous.log")).unwrap(),
        b"previous normal runtime log"
    );
}

#[test]
fn protocol_requires_the_actual_source_built_updater() {
    let Some(updater) = std::env::var_os("NATIVE_PROTOCOL_UPDATED_BINARY") else {
        // The missing-tool case is always exercised. The full focused Actions
        // job also passes the binary built from the very same source checkout.
        let temp = tempfile::tempdir().unwrap();
        let result = Command::new(env!("CARGO_BIN_EXE_openpilot-launch"))
            .arg("--verify-native-generation-protocol")
            .arg("--root")
            .arg(temp.path())
            .output()
            .unwrap();
        assert!(!result.status.success());
        return;
    };
    let temp = tempfile::tempdir().unwrap();
    let bin = temp.path().join("rust/bin");
    fs::create_dir_all(&bin).unwrap();
    fs::copy(updater, bin.join("openpilot-updated")).unwrap();
    let probe = || {
        Command::new(env!("CARGO_BIN_EXE_openpilot-launch"))
            .arg("--verify-native-generation-protocol")
            .arg("--root")
            .arg(temp.path())
            .output()
            .unwrap()
    };
    let result = probe();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&result.stdout).unwrap(),
        serde_json::json!({"source_generation_protocol":1,"startup":true,"updates":true})
    );
    fs::remove_file(bin.join("openpilot-updated")).unwrap();
    let result = probe();
    assert!(
        !result.status.success(),
        "missing updater must not advertise native update support"
    );
}
