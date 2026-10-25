//! Maintained publisher configuration policy; provider execution is separate.
use openpilot_dev_tools::ci_policy::parse;
use serde_json::Value;
use std::{fs, path::PathBuf};

const BINARY: &str = "MODEL_CATALOG_PUBLISH_BINARY";
const HASH: &str = "MODEL_CATALOG_PUBLISH_SHA256";
const EXPORT: &str =
    "Stage two independent model datasets and export separate current-host catalogs";
const CONSUME: &str =
    "Run same held native ELFs in actual checkout and private six-source-absent root";
const DIRECT: &str = "Build and validate immutable native pipeline catalog";

fn required(ok: bool, message: &str) -> Result<(), String> {
    if ok {
        Ok(())
    } else {
        Err(message.into())
    }
}
fn policy(workflow: &Value) -> Result<(), String> {
    for key in [BINARY, HASH] {
        required(workflow["env"][key].is_null(), "workflow ambient publisher")?;
    }
    for name in ["model-family-retirement", "model-pipelines"] {
        let job = &workflow["jobs"][name];
        for key in [BINARY, HASH] {
            required(job["env"][key].is_null(), "job ambient publisher")?;
        }
        let steps = job["steps"].as_array().ok_or("missing steps")?;
        let binding = steps
            .iter()
            .position(|s| s["id"] == "bridge-bind")
            .ok_or("missing bind")?;
        let run = steps[binding]["run"].as_str().ok_or("missing bind shell")?;
        for token in [
            "publisher=$(readlink -f \"$GITHUB_WORKSPACE/rust/target/debug/publish-model-catalog\")",
            "publisher_sha=$(sha256sum \"$publisher\"",
            "catalog-publisher-binary=%s\\ncatalog-publisher-sha256=%s",
            ">> \"$GITHUB_OUTPUT\"", "\"$publisher\" >> \"$proof/binding-paths.txt\"",
            "git ls-files -- rust/crates/model-runtime/src",
            "rust/crates/model-runtime/Cargo.toml", "rust/Cargo.lock", "rust/rust-toolchain.toml",
        ] {
            required(run.contains(token), "publisher lacks same-source binding/seals")?;
        }
        let build = if name == "model-family-retirement" {
            steps.iter().position(|s| s["run"].as_str().is_some_and(|r| r.contains("--bin model-run --bin offline-llvm-guarded-add --bin publish-model-catalog")))
        } else {
            steps.iter().position(|s| s["run"].as_str().is_some_and(|r| r.contains("-p openpilot-model-runtime --bins --locked")))
        }.ok_or("publisher not built")?;
        required(build < binding, "publisher must link before binding")?;
        for title in if name == "model-family-retirement" {
            &[EXPORT, CONSUME][..]
        } else {
            &[DIRECT][..]
        } {
            required(
                steps.iter().filter(|s| s["name"] == *title).count() == 1,
                "missing caller/consumer",
            )?;
        }
        for (index, step) in steps.iter().enumerate() {
            let selected = if name == "model-family-retirement" {
                step["name"] == EXPORT || step["name"] == CONSUME
            } else {
                step["name"] == DIRECT
            };
            for (key, output) in [
                (BINARY, "catalog-publisher-binary"),
                (HASH, "catalog-publisher-sha256"),
            ] {
                if selected {
                    required(index > binding, "caller precedes binding")?;
                    required(
                        step["env"][key]
                            == format!("${{{{ steps.bridge-bind.outputs.{output} }}}}"),
                        "caller missing exact paired output",
                    )?;
                } else {
                    required(
                        step["env"][key].is_null(),
                        "publisher escaped selected caller scope",
                    )?;
                }
            }
        }
    }
    Ok(())
}
fn workflow() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .join(".github/workflows/rust.yml");
    parse(&fs::read_to_string(path).unwrap()).unwrap()
}
#[test]
fn maintained_model_callers_require_paired_publisher_without_ambient_activation() {
    policy(&workflow()).unwrap();
}
#[test]
fn caller_policy_rejects_missing_pin_leaked_scope_late_link_and_unsealed_source() {
    let original = workflow();
    for name in ["model-family-retirement", "model-pipelines"] {
        let selected: Vec<usize> = original["jobs"][name]["steps"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
            .filter(|(_, s)| !s["env"][BINARY].is_null())
            .map(|(i, _)| i)
            .collect();
        for index in selected {
            for key in [BINARY, HASH] {
                let mut changed = original.clone();
                changed["jobs"][name]["steps"][index]["env"]
                    .as_object_mut()
                    .unwrap()
                    .remove(key);
                assert!(policy(&changed).is_err(), "accepted {name}/{index}/{key}");
            }
        }
        for key in [BINARY, HASH] {
            let mut changed = original.clone();
            changed["jobs"][name]["env"][key] = "ambient".into();
            assert!(policy(&changed).is_err());
        }
        let binding = original["jobs"][name]["steps"]
            .as_array()
            .unwrap()
            .iter()
            .position(|s| s["id"] == "bridge-bind")
            .unwrap();
        let mut changed = original.clone();
        let text = changed["jobs"][name]["steps"][binding]["run"]
            .as_str()
            .unwrap()
            .replace(
                "git ls-files -- rust/crates/model-runtime/src",
                "git ls-files -- rust/crates/model-runtime/absent",
            );
        changed["jobs"][name]["steps"][binding]["run"] = text.into();
        assert!(policy(&changed).is_err());
        let mut changed = original.clone();
        let steps = changed["jobs"][name]["steps"].as_array_mut().unwrap();
        let build = steps
            .iter()
            .position(|s| {
                s["run"].as_str().is_some_and(|r| {
                    r.contains("--bin publish-model-catalog")
                        || r.contains("-p openpilot-model-runtime --bins --locked")
                })
            })
            .unwrap();
        steps.swap(build, binding);
        assert!(policy(&changed).is_err());
    }
}
