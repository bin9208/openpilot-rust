//! Actual compile/reuse/run; no SDK tree audit or generation/sealer involved.
use openpilot_runtime_build::{generation::Layout, prepare::Plan, user_build::compile_plan};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{fs, os::unix::fs::symlink, path::PathBuf, process::Command};

#[test]
fn normal_build_runs_changed_code_preserves_unchanged_outputs_and_never_seals() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("source");
    fs::create_dir_all(root.join("rust/src/bin")).unwrap();
    fs::write(
        root.join("rust/Cargo.toml"),
        "[package]\nname='user-fixture'\nversion='0.1.0'\nedition='2021'\n",
    )
    .unwrap();
    fs::write(
        root.join("rust/Cargo.lock"),
        "version = 4\n[[package]]\nname = \"user-fixture\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    fs::write(
        root.join("rust/src/main.rs"),
        "fn main(){println!(\"42\");}\n",
    )
    .unwrap();
    fs::write(
        root.join("rust/src/bin/stable.rs"),
        "fn main(){println!(\"stable\");}\n",
    )
    .unwrap();
    let old = temp.path().join("old-runtime");
    fs::create_dir(&old).unwrap();
    fs::write(old.join("previous"), "untouched old runtime").unwrap();
    symlink(&old, root.join("rust/bin")).unwrap();
    let compiler = Command::new("rustc")
        .args(["--print", "sysroot"])
        .output()
        .unwrap();
    assert!(compiler.status.success());
    let toolchain = PathBuf::from(String::from_utf8(compiler.stdout).unwrap().trim()).join("bin");
    let version = Command::new(toolchain.join("rustc"))
        .arg("-vV")
        .output()
        .unwrap();
    let version = String::from_utf8(version.stdout).unwrap();
    let target = version
        .lines()
        .find_map(|s| s.strip_prefix("host: "))
        .unwrap();
    let recipe = json!({"schema":1,"target":target,"packages":["user-fixture"],
        "binaries":["user-fixture","stable"],"features":[],"inputs":{"sdk.receipt":"a".repeat(64)},
        "input_trees":{"missing-sdk":"sdk.receipt"},"environment":{},"expected_growth_bytes":8388608});
    let audit = json!({"schema":1,"target":target,"packages":["not-a-runtime-package"],
        "binaries":["seal-runtime-package","runtime-elf-audit"],"features":[],"inputs":{},
        "environment":{},"expected_growth_bytes":8388608});
    let plan: Plan = serde_json::from_value(json!({"schema":1,"groups":{"00":recipe,"99":audit},
        "expected_source_growth_bytes":1,"remaining_object_growth_bytes":1}))
    .unwrap();
    let provider = temp.path().join("provider");
    fs::create_dir(&provider).unwrap();
    fs::write(provider.join("data"), "model/provider fixture").unwrap();
    let layout = Layout {
        schema: 1,
        toolchain_root: toolchain.clone(),
        provider_root: provider,
        provider_receipt: "unused-receipt".into(),
        files: std::collections::BTreeMap::from([("rust/bin/data".into(), "data".into())]),
        libraries: vec![],
        expected_source_growth_bytes: 1,
        remaining_object_growth_bytes: 1,
    };
    let cache = temp.path().join("cache");
    let legacy = cache.join("previous-cargo-cache");
    fs::create_dir_all(legacy.join("target")).unwrap();
    // Only a cache hint is consumed, not the previous audit fields. Actual
    // compilation/freshness below must still come from Cargo itself.
    fs::write(
        legacy.join("last-successful-build.json"),
        serde_json::to_vec(&json!({"schema":3,"toolchain_directory":toolchain,
            "target_directory":legacy.join("target"),
            "recipe_sha256":format!("{:x}",Sha256::digest(serde_json::to_vec(&plan.groups["00"]).unwrap()))})).unwrap(),
    ).unwrap();
    let build = || compile_plan(&root, &cache, &toolchain, &plan, &layout).unwrap();
    let first = build();
    assert_eq!(first.compiled, 2);
    assert!(
        !cache.join("user-cargo").exists(),
        "reuse the previous Cargo target"
    );
    assert!(!first.development_audit_executed && !first.sealed_generation_created);
    assert_eq!(
        root.join("rust/bin").read_link().unwrap(),
        PathBuf::from(".source-bin")
    );
    assert_eq!(
        fs::read_to_string(old.join("previous")).unwrap(),
        "untouched old runtime"
    );
    let output = || {
        Command::new(root.join("rust/bin/user-fixture"))
            .output()
            .unwrap()
            .stdout
    };
    assert_eq!(output(), b"42\n");
    let stable = fs::metadata(root.join("rust/bin/stable"))
        .unwrap()
        .modified()
        .unwrap();
    let data = fs::metadata(root.join("rust/bin/data"))
        .unwrap()
        .modified()
        .unwrap();
    let second = build();
    assert_eq!((second.compiled, second.fresh, second.installed), (0, 2, 0));
    fs::write(
        root.join("rust/src/main.rs"),
        "fn main(){println!(\"43\");}\n",
    )
    .unwrap();
    let changed = build();
    assert_eq!((changed.compiled, changed.fresh), (1, 1));
    assert_eq!(output(), b"43\n");
    assert_eq!(
        fs::metadata(root.join("rust/bin/stable"))
            .unwrap()
            .modified()
            .unwrap(),
        stable
    );
    assert_eq!(
        fs::metadata(root.join("rust/bin/data"))
            .unwrap()
            .modified()
            .unwrap(),
        data
    );
    fs::write(root.join("rust/src/main.rs"), "not valid rust source\n").unwrap();
    assert!(compile_plan(&root, &cache, &toolchain, &plan, &layout).is_err());
    assert_eq!(
        output(),
        b"43\n",
        "failed compile must not replace the last built executable"
    );
    assert!(!root.join("rust/runtime-manifest.json").exists());
    assert!(!cache.join("generations").exists());
}
