//! Native counterparts of tools/ci/tests/test_workflow_policy.py.
use openpilot_dev_tools::ci_policy::{
    TRUSTED_SOURCE, check_workflow_policy, parse, run_shell, validate_local_routing,
};
use std::{collections::BTreeMap, fs, path::PathBuf};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}
fn inputs() -> Vec<(String, String)> {
    fs::read_dir(root().join(".github/workflows"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_file())
        .map(|path| {
            (
                path.file_name().unwrap().to_str().unwrap().to_owned(),
                fs::read_to_string(path).unwrap(),
            )
        })
        .collect()
}
#[test]
fn maintained_workflow_files_fit_github_registration_byte_limit() {
    for (name, source) in inputs() {
        // String::len measures UTF-8 bytes, which GitHub admits before job creation.
        let bytes = source.len();
        assert!(
            bytes <= 500_000,
            "{name}: {bytes} bytes exceeds the 500000-byte workflow registration limit"
        );
    }
}
#[test]
fn maintained_workflow_policy_is_valid() {
    let files = inputs();
    check_workflow_policy(&files).unwrap();
}
#[test]
fn protected_workflow_mutations_are_rejected() {
    let original = inputs();
    for (name, from, to) in [
        ("integration.yml", "${{ always() }}", "${{ success() }}"),
        ("integration.yml", "test \"$BUILD\" = success", "true"),
        (
            "user-docs.yaml",
            "branches: [carrot-wip, dev, main]",
            "branches: [carrot-wip, other, main]",
        ),
    ] {
        let mut files = original.clone();
        let source = &mut files.iter_mut().find(|(file, _)| file == name).unwrap().1;
        assert!(
            source.contains(from),
            "mutation anchor absent: {name} {from}"
        );
        *source = source.replacen(from, to, 1);
        assert!(
            check_workflow_policy(&files).is_err(),
            "accepted {name}: {from} -> {to}"
        );
    }
}
#[test]
fn yaml_scalars_preserve_github_on_and_base_loader_values() {
    let value =
        parse("on: {push: {branches: ['**']}}\nnumber: 1\nflag: false\nquoted: '01'\n").unwrap();
    assert_eq!(value["on"]["push"]["branches"][0], "**");
    assert_eq!(value["number"], "1");
    assert_eq!(value["flag"], "false");
    assert_eq!(value["quoted"], "01");
}
#[test]
fn yaml_raw_scalars_anchors_and_multiline_match_base_loader() {
    let value = parse("on: true\nleading: 001\nhex: 0x1\nnull: null\nempty:\nquoted: '01'\ntext: |\n  first\n  second\nanchor: &settings {channel: 001, flag: true}\nalias: *settings\n").unwrap();
    assert_eq!(value["on"], "true");
    assert_eq!(value["leading"], "001");
    assert_eq!(value["hex"], "0x1");
    assert_eq!(value["null"], "null");
    assert_eq!(value["empty"], "");
    assert_eq!(value["quoted"], "01");
    assert_eq!(value["text"], "first\nsecond\n");
    assert_eq!(
        value["alias"],
        serde_json::json!({"channel":"001", "flag":"true"})
    );
    // Separate an alias key from ':' so the scanner does not consume the
    // delimiter as part of the anchor name (the maintained flow-YAML case).
    let keys =
        parse("{first: {&field working-directory: &folder source}, second: {*field : *folder}}")
            .unwrap();
    assert_eq!(keys["first"]["working-directory"], "source");
    assert_eq!(keys["second"], keys["first"]);
}

fn current_common_file_sha<'a>(
    contract: &'a serde_json::Value,
    name: &str,
    historical: &'a serde_json::Value,
) -> &'a str {
    if name != "tools/ci/local-arm" {
        return historical["sha256"].as_str().unwrap();
    }
    let overlay = &contract["github_hosted_535_overlay"];
    assert_eq!(overlay["issue"], 535);
    assert_eq!(
        overlay["baseline_source_commit"],
        "77663836a95a9e2aff9aa66592d8b06ed9c87cd9"
    );
    assert_eq!(overlay["files"].as_object().unwrap().len(), 1);
    assert_eq!(overlay["historical_acceptance_changed"], false);
    let current = &overlay["files"][name];
    assert_eq!(current["preimage_sha256"], historical["sha256"]);
    assert_eq!(
        current["sha256"],
        "a286863a38d2d122bd97b7f648086c1763e3aa863fe36fe7101f2f7a6ae7ada0"
    );
    current["sha256"].as_str().unwrap()
}

#[test]
fn common_foundation_files_match_the_separate_pinned_source_contract() {
    use sha2::{Digest, Sha256};
    let contract: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(root().join("tools/ci/local-routing-296-contract.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        contract["source_commit"],
        "77663836a95a9e2aff9aa66592d8b06ed9c87cd9"
    );
    assert_eq!(contract["files"].as_object().unwrap().len(), 4);
    assert_eq!(
        contract["action_distribution"]["commit"],
        "b2fc62472ce8183114d59dbeec4d1406df44e4c2"
    );
    assert_eq!(
        contract["action_distribution"]["source_commit"],
        contract["source_commit"]
    );
    for (name, source) in inputs() {
        let workflow = parse(&source).unwrap();
        for job in workflow["jobs"].as_object().unwrap().values() {
            for step in job["steps"].as_array().into_iter().flatten() {
                let Some(reference) = step["uses"].as_str() else {
                    continue;
                };
                for action in ["job-temp", "rust-path", "local-arm"] {
                    let prefix = format!("bin9208/openpilot-rust/.github/actions/{action}@");
                    if reference.starts_with(&prefix) {
                        assert_eq!(
                            reference,
                            format!("{prefix}b2fc62472ce8183114d59dbeec4d1406df44e4c2"),
                            "common distribution pin drift: {name}"
                        );
                    }
                }
            }
        }
    }
    for (name, seal) in contract["files"].as_object().unwrap() {
        let bytes = fs::read(root().join(name)).unwrap();
        assert_eq!(
            format!("{:x}", Sha256::digest(&bytes)),
            current_common_file_sha(&contract, name, seal),
            "common interface drift: {name}"
        );
    }
    let source = fs::read_to_string(root().join("tools/ci/local-arm")).unwrap();
    assert!(source.contains("physical_arm_cpu\":false"));
    assert!(source.contains("[[ $guest_machine == aarch64 ]]"));
    assert!(source.contains("[[ $(git -C \"$source_dir\" rev-parse HEAD) == \"$GITHUB_SHA\" ]]"));
}

#[test]
fn integration_aggregate_cannot_pass_failed_or_skipped_workers_with_or_true() {
    let mut files = inputs();
    let source = &mut files
        .iter_mut()
        .find(|(name, _)| name == "integration.yml")
        .unwrap()
        .1;
    assert!(source.contains("test \"$JETLINK\" = success"));
    *source = source.replacen(
        "test \"$JETLINK\" = success",
        "test \"$JETLINK\" = success || true",
        1,
    );
    assert!(check_workflow_policy(&files).is_err());
}

#[test]
fn diagnostic_common_helpers_match_verified_7766383_bytes_without_reexecution() {
    use sha2::{Digest, Sha256};
    let contract: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(root().join("tools/ci/diagnostic-296-contract.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        contract["common_source"],
        "77663836a95a9e2aff9aa66592d8b06ed9c87cd9"
    );
    assert_eq!(contract["common_files"].as_object().unwrap().len(), 4);
    assert_eq!(
        contract["action_distribution"]["commit"],
        "b2fc62472ce8183114d59dbeec4d1406df44e4c2"
    );
    assert_eq!(
        contract["action_distribution"]["source_commit"],
        contract["common_source"]
    );
    for (name, seal) in contract["common_files"].as_object().unwrap() {
        assert_eq!(
            format!("{:x}", Sha256::digest(fs::read(root().join(name)).unwrap())),
            current_common_file_sha(&contract, name, seal),
            "common source drift {name}"
        );
    }
}

#[test]
fn full_private_jobs_reject_shared_checkouts_or_unsealed_cleanup() {
    use openpilot_dev_tools::ci_policy::validate_private_job_layout;
    let workflows = inputs();
    let mut covered = 0;
    for (_, source) in workflows {
        let workflow = parse(&source).unwrap();
        for (name, job) in workflow["jobs"].as_object().unwrap() {
            if job["env"].get("CI296_ALIAS_BASENAME").is_none() {
                continue;
            }
            validate_private_job_layout(job).unwrap_or_else(|e| panic!("{name}: {e}"));
            covered += 1;
            let steps = job["steps"].as_array().unwrap();
            let checkout = steps
                .iter()
                .position(|s| {
                    s["uses"]
                        .as_str()
                        .unwrap_or("")
                        .starts_with("actions/checkout@")
                })
                .unwrap();
            let mut shared = job.clone();
            shared["steps"][checkout]["with"]["path"] = ".".into();
            assert!(validate_private_job_layout(&shared).is_err());
            let cleanup = steps
                .iter()
                .position(|s| s["name"] == "Clean only uploaded owned source alias and target")
                .unwrap();
            let mut unsealed = job.clone();
            unsealed["steps"][cleanup]["if"] = "${{ always() }}".into();
            assert!(validate_private_job_layout(&unsealed).is_err());
            let allocate=steps.iter().position(|s|s["name"]=="Allocate private source cache target and local action alias before checkout").unwrap();
            let mut unsafe_alias = job.clone();
            unsafe_alias["steps"][allocate]["run"] = steps[allocate]["run"]
                .as_str()
                .unwrap()
                .replace("ln -sT", "ln -sf")
                .into();
            assert!(validate_private_job_layout(&unsafe_alias).is_err());
        }
    }
    assert_eq!(covered, 79, "maintained private source roster changed");
}

#[test]
fn docs_private_cleanup_requires_exact_sdk_and_owned_recovery_receipts() {
    use openpilot_dev_tools::ci_policy::validate_private_job_layout;
    let workflow =
        parse(&fs::read_to_string(root().join(".github/workflows/user-docs.yaml")).unwrap())
            .unwrap();
    let job = &workflow["jobs"]["check_user_docs"];
    validate_private_job_layout(job).unwrap();
    let steps = job["steps"].as_array().unwrap();
    let at = |id: &str| steps.iter().position(|step| step["id"] == id).unwrap();
    let cleanup = at("ci296_source_cleanup");
    for id in ["prepared_rust_final", "ci296_docs_recovery"] {
        let mut changed = job.clone();
        let original = steps[cleanup]["if"].as_str().unwrap();
        changed["steps"][cleanup]["if"] = original
            .replace(&format!(" && steps.{id}.outcome == 'success'"), "")
            .into();
        assert!(
            validate_private_job_layout(&changed).is_err(),
            "docs cleanup omitted {id}"
        );
        let mut changed = job.clone();
        changed["steps"][at(id)]["run"] = "true".into();
        assert!(
            validate_private_job_layout(&changed).is_err(),
            "docs safety body omitted {id}"
        );
        let mut changed = job.clone();
        changed["steps"][at(id)]["if"] = TRUSTED_SOURCE.into();
        assert!(
            validate_private_job_layout(&changed).is_err(),
            "docs finalizer skipped on failure: {id}"
        );
    }
    for name in [
        "Check code and user documentation changes",
        "Test user documentation checker",
    ] {
        let mut changed = job.clone();
        let command = steps.iter().position(|step| step["name"] == name).unwrap();
        changed["steps"]
            .as_array_mut()
            .unwrap()
            .swap(command, at("ci296_source_seal"));
        assert!(
            validate_private_job_layout(&changed).is_err(),
            "docs command must precede its source seal"
        );
    }
    let mut changed = job.clone();
    changed["steps"][at("ci296_docs_recovery")]["run"] = steps[at("ci296_docs_recovery")]["run"]
        .as_str()
        .unwrap()
        .replace(
            "owner=$GITHUB_RUN_ID:$GITHUB_RUN_ATTEMPT:$GITHUB_JOB",
            "owner=foreign",
        )
        .into();
    assert!(validate_private_job_layout(&changed).is_err());
}

#[test]
fn every_existing_concurrency_scope_preserves_running_and_pending_jobs() {
    use openpilot_dev_tools::ci_policy::validate_concurrency;
    let mut covered = 0;
    for (name, text) in inputs() {
        let original = parse(&text).unwrap();
        validate_concurrency(&original).unwrap_or_else(|e| panic!("{name}: {e}"));
        let mut scopes = Vec::new();
        if original.get("concurrency").is_some() {
            scopes.push(None);
        }
        for (job, value) in original["jobs"].as_object().unwrap() {
            if value.get("concurrency").is_some() {
                scopes.push(Some(job.clone()));
            }
        }
        for job in scopes {
            covered += 1;
            for mutation in ["missing", "single", "cancels"] {
                let mut changed = original.clone();
                let concurrency = match &job {
                    Some(job) => &mut changed["jobs"][job]["concurrency"],
                    None => &mut changed["concurrency"],
                };
                match mutation {
                    "missing" => {
                        concurrency.as_object_mut().unwrap().remove("queue");
                    }
                    "single" => concurrency["queue"] = "single".into(),
                    "cancels" => concurrency["cancel-in-progress"] = "true".into(),
                    _ => unreachable!(),
                }
                assert!(
                    validate_concurrency(&changed).is_err(),
                    "{name} {job:?}: {mutation}"
                );
            }
        }
    }
    assert!(covered >= 10, "concurrency roster unexpectedly shrank");
    // Exercise job-scoped queues even when the maintained workflows currently
    // use workflow scope; the separately published focused recipe uses this.
    for scope in [
        "concurrency: {group: fixed, cancel-in-progress: false, queue: max}\njobs: {}\n",
        "jobs: {focus: {concurrency: {group: fixed, cancel-in-progress: false, queue: max}}}\n",
    ] {
        validate_concurrency(&parse(scope).unwrap()).unwrap();
        for (from, to) in [
            ("queue: max", "queue: single"),
            ("cancel-in-progress: false", "cancel-in-progress: true"),
        ] {
            assert!(validate_concurrency(&parse(&scope.replace(from, to)).unwrap()).is_err());
        }
    }
}

#[test]
fn arm_target_steps_and_nested_native_actions_require_explicit_command_file_shell() {
    use openpilot_dev_tools::ci_policy::validate_private_job_layout;
    let mut workflow =
        parse(&fs::read_to_string(root().join(".github/workflows/rust.yml")).unwrap()).unwrap();
    let native =
        parse(&fs::read_to_string(root().join(".github/workflows/rust-native-arm.yml")).unwrap())
            .unwrap();
    for name in [
        "planner-runtime",
        "carrot-man-runtime",
        "webrtc-runtime",
        "webcam-runtime",
        "xiaoge-runtime",
        "radard-runtime",
        "radar-arm",
        "carrot-navi-arm",
    ] {
        workflow["jobs"][format!("native/{name}")] = native["jobs"][name].clone();
    }
    let mut routed = 0;
    let mut calls = Vec::new();
    for (name, job) in workflow["jobs"].as_object().unwrap() {
        if job["env"].get("LOCAL_CI_ARCH").is_none() {
            continue;
        }
        routed += 1;
        validate_private_job_layout(job).unwrap();
        for (index, step) in job["steps"].as_array().unwrap().iter().enumerate() {
            if step.get("run").is_some() && step["env"]["CI296_HOST_CONTROL"] != "1" {
                let mut bypass = job.clone();
                bypass["steps"][index]["shell"] = "bash".into();
                assert!(
                    validate_private_job_layout(&bypass).is_err(),
                    "{name}: target Bash bypass"
                );
            }
            if step["uses"].as_str().is_some_and(|called| {
                ["native-json11", "native-eigen", "native-image-sdk"]
                    .iter()
                    .any(|action| called.ends_with(&format!("/.github/actions/{action}")))
            }) {
                calls.push(name.as_str());
                for shell in [None, Some("bash")] {
                    let mut bypass = job.clone();
                    match shell {
                        Some(shell) => bypass["steps"][index]["with"]["run-shell"] = shell.into(),
                        None => {
                            bypass["steps"][index]["with"]
                                .as_object_mut()
                                .unwrap()
                                .remove("run-shell");
                        }
                    }
                    assert!(
                        validate_private_job_layout(&bypass).is_err(),
                        "{name}: composite shell input bypass"
                    );
                }
            }
        }
    }
    assert_eq!(routed, 14);
    calls.sort_unstable();
    assert_eq!(
        calls,
        [
            "carrot-man-runtime",
            "native/carrot-man-runtime",
            "native/planner-runtime",
            "native/radar-arm",
            "native/radard-runtime",
            "native/webrtc-runtime",
            "native/xiaoge-runtime",
            "planner-runtime",
            "radard-runtime",
            "webrtc-runtime",
            "xiaoge-runtime"
        ]
    );
}

#[test]
fn native_action_selection_is_bound_to_the_same_private_job_source() {
    use openpilot_dev_tools::ci_policy::uses_native_action;
    let job =
        serde_json::json!({"env":{"CI296_ALIAS_BASENAME":".ci296-source-rust--planner-runtime"}});
    let good = serde_json::json!({"uses":"./.ci296-source-rust--planner-runtime/source/.github/actions/native-json11"});
    assert!(uses_native_action(&job, &good, "native-json11"));
    for called in [
        "./.github/actions/native-json11",
        "./.ci296-source-rust--radard-runtime/source/.github/actions/native-json11",
        "./.ci296-source-rust--planner-runtime/source/.github/actions/native-image-sdk",
    ] {
        assert!(!uses_native_action(
            &job,
            &serde_json::json!({"uses":called}),
            "native-json11"
        ));
    }
    assert!(uses_native_action(
        &serde_json::json!({}),
        &serde_json::json!({"uses":"./.github/actions/native-json11"}),
        "native-json11"
    ));
}

#[test]
fn private_ownership_exact_guards_reject_missing_seal_or_admission() {
    use openpilot_dev_tools::ci_policy::validate_private_job_layout;
    let (mut sources, mut arm) = (0, 0);
    for (_, text) in inputs() {
        let workflow = parse(&text).unwrap();
        for (name, job) in workflow["jobs"].as_object().unwrap() {
            if job["env"].get("CI296_ALIAS_BASENAME").is_none() {
                continue;
            }
            validate_private_job_layout(job).unwrap();
            sources += 1;
            let steps = job["steps"].as_array().unwrap();
            let at = |name: &str| steps.iter().position(|s| s["name"] == name).unwrap();
            let seal = at("Seal private source and index after maintained checks");
            let cleanup = at("Clean only uploaded owned source alias and target");
            let common = at("Clean common owned temporary paths after uploaded source");
            let original = steps[cleanup]["if"].as_str().unwrap();
            for condition in [
                original.replace(" && steps.ci296_source_seal.outcome == 'success'", ""),
                original.replace("always() &&", "always() || true ||"),
                original.replace("ci296_source_seal.outcome", "other_seal.outcome"),
            ] {
                assert_ne!(condition, original);
                let mut changed = job.clone();
                changed["steps"][cleanup]["if"] = condition.into();
                assert!(
                    validate_private_job_layout(&changed).is_err(),
                    "{name}: incomplete source seal may delete proof"
                );
            }
            let mut changed = job.clone();
            changed["steps"][seal]["id"] = "unbound_seal".into();
            assert!(validate_private_job_layout(&changed).is_err());
            let mut changed = job.clone();
            changed["steps"][seal]["run"] = "true".into();
            assert!(validate_private_job_layout(&changed).is_err());
            let mut changed = job.clone();
            changed["steps"][common]["if"] = steps[common]["if"]
                .as_str()
                .unwrap()
                .replace(" && steps.ci296_source_cleanup.outcome == 'success'", "")
                .into();
            assert!(
                validate_private_job_layout(&changed).is_err(),
                "{name}: common proof cleanup bypass"
            );
            if job["env"].get("LOCAL_CI_ARCH").is_some() {
                arm += 1;
                let admission = at("Admit exact private source and index");
                let finish = at(if job["env"]["CI296_NATIVE_ARM"] == "1" {
                    "Finish native ARM execution evidence"
                } else {
                    "Finish emulated architecture evidence"
                });
                let mut changed = job.clone();
                changed["steps"][finish]["if"] = steps[finish]["if"]
                    .as_str()
                    .unwrap()
                    .replace(" && steps.ci296_source_admission.outcome == 'success'", "")
                    .into();
                assert!(
                    validate_private_job_layout(&changed).is_err(),
                    "{name}: stale alias finalizer"
                );
                let mut changed = job.clone();
                changed["steps"][admission]["id"] = "unbound_admission".into();
                assert!(validate_private_job_layout(&changed).is_err());
            }
        }
    }
    assert_eq!((sources, arm), (79, 24));
}

// Evaluate the actual restricted workflow condition, not a separately invented
// cleanup predicate. These controls run only in the parent-published Actions gate.
// Shared transport and provider actions are never invoked by these tests.
fn ownership_condition_as_bash(condition: &str, outcomes: &BTreeMap<String, String>) -> String {
    let expression = condition
        .strip_prefix("${{ ")
        .unwrap()
        .strip_suffix(" }}")
        .unwrap();
    let expression = expression
        .replace("github.event_name", "'push'")
        .replace(
            "github.event.pull_request.head.repo.full_name",
            "'bin9208/openpilot-rust'",
        )
        .replace("github.repository", "'bin9208/openpilot-rust'")
        .replace("env.CI296_SOURCE_OWNER", "'owned-fixture-source'")
        .replace("always()", "1 == 1");
    let tokens = regex::Regex::new(
        r"steps\.([A-Za-z0-9_-]+)\.(outcome|outputs\.artifact-id|outputs\.artifact-digest)",
    )
    .unwrap();
    let expression = tokens
        .replace_all(&expression, |caps: &regex::Captures<'_>| {
            let value = if &caps[2] == "outcome" {
                outcomes
                    .get(&caps[1])
                    .map(String::as_str)
                    .unwrap_or("success")
            } else if &caps[2] == "outputs.artifact-id" {
                "123"
            } else {
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            };
            format!("'{value}'")
        })
        .into_owned();
    for unresolved in ["github.", "steps.", "env."] {
        assert!(!expression.contains(unresolved));
    }
    expression
}

#[test]
fn private_failed_seal_retains_source_alias_and_common_proof() {
    use std::os::unix::fs::{MetadataExt, symlink};
    let workflow =
        parse(&fs::read_to_string(root().join(".github/workflows/naver-ci.yml")).unwrap()).unwrap();
    let job = workflow["jobs"]
        .as_object()
        .unwrap()
        .values()
        .find(|job| job["env"].get("CI296_ALIAS_BASENAME").is_some())
        .unwrap();
    let steps = job["steps"].as_array().unwrap();
    let source_cleanup = steps
        .iter()
        .find(|s| s["name"] == "Clean only uploaded owned source alias and target")
        .unwrap();
    let common_cleanup = steps
        .iter()
        .find(|s| s["name"] == "Clean common owned temporary paths after uploaded source")
        .unwrap();
    for seal in ["failure", "cancelled", "skipped", "", "success"] {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        fs::create_dir(&workspace).unwrap();
        let workspace = fs::canonicalize(workspace).unwrap();
        let owner = workspace.join(".ci296-source-123-1.ABCDEF");
        let source = owner.join("source");
        fs::create_dir_all(&source).unwrap();
        fs::write(owner.join(".ci296-owner"), "123:1:ownership-fixture").unwrap();
        fs::write(
            source.join("held-fixture-input"),
            b"held original fixture bytes",
        )
        .unwrap();
        let alias_name = job["env"]["CI296_ALIAS_BASENAME"].as_str().unwrap();
        let alias = workspace.join(alias_name);
        symlink(&owner, &alias).unwrap();
        let identity = |path: &std::path::Path| {
            let m = fs::symlink_metadata(path).unwrap();
            format!("{}:{}", m.dev(), m.ino())
        };
        let proof = temp.path().join("proof");
        fs::create_dir(&proof).unwrap();
        fs::write(
            proof.join("partial-uploaded-receipt"),
            b"uploaded partial proof is insufficient",
        )
        .unwrap();
        let env = BTreeMap::from([
            ("GITHUB_WORKSPACE".into(), workspace.display().to_string()),
            (
                "CI296_ACTIONS_WORKSPACE".into(),
                workspace.display().to_string(),
            ),
            ("CI296_SOURCE_OWNER".into(), owner.display().to_string()),
            ("CI296_SOURCE".into(), source.display().to_string()),
            ("CI296_SOURCE_IDENTITY".into(), identity(&owner)),
            ("CI296_SOURCE_ALIAS".into(), alias.display().to_string()),
            ("CI296_ALIAS_BASENAME".into(), alias_name.into()),
            ("CI296_ALIAS_IDENTITY".into(), identity(&alias)),
            ("GITHUB_RUN_ID".into(), "123".into()),
            ("GITHUB_RUN_ATTEMPT".into(), "1".into()),
            ("GITHUB_JOB".into(), "ownership-fixture".into()),
            ("UPLOAD_ID".into(), "123".into()),
            ("UPLOAD_DIGEST".into(), "a".repeat(64)),
        ]);
        let mut outcomes = BTreeMap::from([("ci296_source_seal".into(), seal.into())]);
        if seal == "skipped" {
            outcomes.insert("ci296_source_admission".into(), "failure".into());
        }
        let expression =
            ownership_condition_as_bash(source_cleanup["if"].as_str().unwrap(), &outcomes);
        let script = format!(
            "if [[ {expression} ]]; then\n{}\nelse\nexit 17\nfi\n",
            source_cleanup["run"].as_str().unwrap()
        );
        let status = run_shell(&script, &env).unwrap();
        if seal == "success" {
            assert!(status.success());
            assert!(!owner.exists() && !alias.exists());
        } else {
            assert_eq!(status.code(), Some(17));
            assert_eq!(
                fs::read(source.join("held-fixture-input")).unwrap(),
                b"held original fixture bytes"
            );
            assert_eq!(fs::read_link(&alias).unwrap(), owner);
            outcomes.insert("ci296_source_cleanup".into(), "skipped".into());
            let common =
                ownership_condition_as_bash(common_cleanup["if"].as_str().unwrap(), &outcomes);
            assert_eq!(
                run_shell(
                    &format!("if [[ {common} ]]; then exit 0; else exit 17; fi"),
                    &env
                )
                .unwrap()
                .code(),
                Some(17)
            );
            assert!(proof.join("partial-uploaded-receipt").is_file());
        }
    }
}

#[test]
fn private_arm_finalizer_requires_fresh_admission_before_alias_load() {
    let mut workflow =
        parse(&fs::read_to_string(root().join(".github/workflows/rust.yml")).unwrap()).unwrap();
    let native =
        parse(&fs::read_to_string(root().join(".github/workflows/rust-native-arm.yml")).unwrap())
            .unwrap();
    for name in [
        "planner-runtime",
        "carrot-man-runtime",
        "webrtc-runtime",
        "webcam-runtime",
        "xiaoge-runtime",
        "radard-runtime",
        "radar-arm",
        "carrot-navi-arm",
    ] {
        workflow["jobs"][format!("native/{name}")] = native["jobs"][name].clone();
    }
    let mut covered = 0;
    for (name, job) in workflow["jobs"].as_object().unwrap() {
        if job["env"].get("LOCAL_CI_ARCH").is_none() {
            continue;
        }
        covered += 1;
        let finish = job["steps"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| {
                s["name"]
                    == if job["env"]["CI296_NATIVE_ARM"] == "1" {
                        "Finish native ARM execution evidence"
                    } else {
                        "Finish emulated architecture evidence"
                    }
            })
            .unwrap();
        for admission in ["failure", "cancelled", "skipped", "", "success"] {
            let temp = tempfile::tempdir().unwrap();
            let retained = temp.path().join("retained-old-action.yml");
            fs::write(&retained, b"inert retained action manifest").unwrap();
            let probe = temp.path().join("manifest-load-probe");
            let outcomes = BTreeMap::from([("ci296_source_admission".into(), admission.into())]);
            let condition = ownership_condition_as_bash(finish["if"].as_str().unwrap(), &outcomes);
            let env = BTreeMap::from([
                ("RETAINED_MANIFEST".into(), retained.display().to_string()),
                ("LOAD_PROBE".into(), probe.display().to_string()),
            ]);
            let script = format!(
                "if [[ {condition} ]]; then test -f \"$RETAINED_MANIFEST\"; printf admitted > \"$LOAD_PROBE\"; else exit 17; fi"
            );
            let status = run_shell(&script, &env).unwrap();
            assert_eq!(
                status.success(),
                admission == "success",
                "{name}/{admission}"
            );
            assert_eq!(probe.exists(), admission == "success");
            assert_eq!(
                fs::read(retained).unwrap(),
                b"inert retained action manifest"
            );
        }
    }
    assert_eq!(covered, 14);
}

#[test]
fn private_android_sdk_root_rejects_inherited_or_shared_storage() {
    use openpilot_dev_tools::ci_policy::validate_private_job_layout;
    let workflow =
        parse(&fs::read_to_string(root().join(".github/workflows/naver-apk-tools.yml")).unwrap())
            .unwrap();
    let mut covered = 0;
    for (name, job) in workflow["jobs"].as_object().unwrap() {
        validate_private_job_layout(job).unwrap();
        for (index, step) in job["steps"].as_array().unwrap().iter().enumerate() {
            if !step["uses"]
                .as_str()
                .is_some_and(|called| called.starts_with("android-actions/setup-android@"))
            {
                continue;
            }
            covered += 1;
            for path in [
                None,
                Some("${{ env.ANDROID_SDK_ROOT }}"),
                Some("/opt/android-sdk"),
            ] {
                let mut changed = job.clone();
                match path {
                    Some(path) => changed["steps"][index]["env"]["ANDROID_SDK_ROOT"] = path.into(),
                    None => {
                        changed["steps"][index]["env"]
                            .as_object_mut()
                            .unwrap()
                            .remove("ANDROID_SDK_ROOT");
                    }
                }
                assert!(
                    validate_private_job_layout(&changed).is_err(),
                    "{name}: inherited/shared SDK root"
                );
            }
        }
    }
    assert_eq!(covered, 2);
}
// #514: always-run uploads/seals do not establish native WebRTC success.
const WEBRTC_REQUIRED_OUTCOMES: [&str; 4] = [
    "webrtc_dependency_stager",
    "webrtc_original_bindings",
    "webrtc_native_checks",
    "webrtc_real_comparisons",
];

fn webrtc_retention_jobs() -> [serde_json::Value; 2] {
    ["rust.yml", "rust-native-arm.yml"].map(|file| {
        parse(&fs::read_to_string(root().join(".github/workflows").join(file)).unwrap()).unwrap()
            ["jobs"]["webrtc-runtime"]
            .clone()
    })
}

fn webrtc_cleanup_condition_passes(
    condition: &str,
    outcomes: &BTreeMap<String, String>,
    cancelled: bool,
    successful: bool,
) -> bool {
    let condition = condition
        .replace("!cancelled()", if cancelled { "1 == 0" } else { "1 == 1" })
        .replace("success()", if successful { "1 == 1" } else { "1 == 0" });
    let actual = ownership_condition_as_bash(&condition, outcomes);
    run_shell(
        &format!("if [[ {actual} ]]; then exit 0; else exit 17; fi"),
        &BTreeMap::new(),
    )
    .unwrap()
    .success()
}

#[test]
fn webrtc_required_native_failures_and_cancellation_retain_both_cleanup_roots() {
    for job in webrtc_retention_jobs() {
        let steps = job["steps"].as_array().unwrap();
        let success: BTreeMap<String, String> = WEBRTC_REQUIRED_OUTCOMES
            .iter()
            .map(|id| ((*id).into(), "success".into()))
            .collect();
        for cleanup in [
            "Clean only uploaded owned source alias and target",
            "Clean common owned temporary paths after uploaded source",
        ] {
            let condition = steps.iter().find(|s| s["name"] == cleanup).unwrap()["if"]
                .as_str()
                .unwrap();
            assert!(webrtc_cleanup_condition_passes(
                condition, &success, false, true
            ));
            // Keep receipts and native outcomes successful to isolate each status
            // guard, including cancellation after the native stages have completed.
            assert!(!webrtc_cleanup_condition_passes(
                condition, &success, true, true
            ));
            assert!(!webrtc_cleanup_condition_passes(
                condition, &success, false, false
            ));
            for id in WEBRTC_REQUIRED_OUTCOMES {
                for outcome in ["failure", "cancelled", "skipped", ""] {
                    let mut outcomes = success.clone();
                    outcomes.insert(id.into(), outcome.into());
                    // Also hold success() true to prove the explicit outcome guard
                    // independently rejects a failed/skipped producer or comparison.
                    for successful in [false, true] {
                        assert!(
                            !webrtc_cleanup_condition_passes(
                                condition, &outcomes, false, successful
                            ),
                            "{cleanup}: {id}={outcome:?} deleted failed evidence"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn webrtc_retention_policy_rejects_guard_removal_and_unbound_required_stages() {
    use openpilot_dev_tools::ci_policy::validate_private_job_layout;
    for job in webrtc_retention_jobs() {
        validate_private_job_layout(&job).unwrap();
        let steps = job["steps"].as_array().unwrap();
        for cleanup in [
            "Clean only uploaded owned source alias and target",
            "Clean common owned temporary paths after uploaded source",
        ] {
            let index = steps.iter().position(|s| s["name"] == cleanup).unwrap();
            let original = steps[index]["if"].as_str().unwrap();
            for clause in ["!cancelled()".to_owned(), "success()".to_owned()]
                .into_iter()
                .chain(
                    WEBRTC_REQUIRED_OUTCOMES.map(|id| format!("steps.{id}.outcome == 'success'")),
                )
            {
                let mut changed = job.clone();
                let condition = original.replacen(&format!(" && {clause}"), "", 1);
                assert_ne!(condition, original, "missing mutation anchor: {clause}");
                changed["steps"][index]["if"] = condition.into();
                assert!(
                    validate_private_job_layout(&changed).is_err(),
                    "{cleanup}: guard removed: {clause}"
                );
            }
            let mut changed = job.clone();
            changed["steps"][index]["if"] = original.replace(".outcome", ".conclusion").into();
            assert!(validate_private_job_layout(&changed).is_err());
        }
        for id in WEBRTC_REQUIRED_OUTCOMES {
            let index = steps.iter().position(|s| s["id"] == id).unwrap();
            let mut changed = job.clone();
            changed["steps"][index]
                .as_object_mut()
                .unwrap()
                .remove("id");
            assert!(
                validate_private_job_layout(&changed).is_err(),
                "missing {id}"
            );
            let mut changed = job.clone();
            changed["steps"][index]["continue-on-error"] = "true".into();
            assert!(validate_private_job_layout(&changed).is_err(), "COE {id}");
            let mut changed = job.clone();
            changed["steps"][index]["if"] = "${{ always() }}".into();
            assert!(
                validate_private_job_layout(&changed).is_err(),
                "not normal {id}"
            );
            let mut changed = job.clone();
            changed["steps"][0]["id"] = id.into();
            assert!(
                validate_private_job_layout(&changed).is_err(),
                "duplicate {id}"
            );
        }
    }
}
#[test]
fn flow_mapping_alias_keys_keep_an_explicit_colon_separator() {
    use openpilot_dev_tools::ci_policy::parse;
    let source = "{keys: [&key run, &body 'cargo test'], steps: [{*key : *body}]}";
    let value = parse(source).unwrap();
    assert_eq!(value["steps"][0]["run"], "cargo test");
    assert!(parse("{keys: [&key run], steps: [{*unknown : value}]}").is_err());
}

#[test]
fn hosted_arm_requires_explicit_intent_and_actual_runner_guards() {
    let original = parse(
        &fs::read_to_string(root().join(".github/workflows/python-rust-523-hosted-arm.yml"))
            .unwrap(),
    )
    .unwrap();
    validate_local_routing(&original).unwrap();
    for key in ["CI296_NATIVE_ARM", "SOURCE_ALLOWED"] {
        let mut changed = original.clone();
        let env = if key == "CI296_NATIVE_ARM" {
            &mut changed["jobs"]["inventory"]["env"]
        } else {
            &mut changed["jobs"]["inventory"]["steps"][0]["env"]
        };
        env.as_object_mut().unwrap().remove(key);
        assert!(validate_local_routing(&changed).is_err(), "missing {key}");
    }
    for guard in [
        "test \"$RUNNER_ENVIRONMENT\" = github-hosted\n",
        "test \"$RUNNER_ARCH\" = ARM64\n",
        "test \"$(uname -m)\" = aarch64\n",
        "test \"$GITHUB_REF\" = refs/heads/codex/feat-523-python-rust\n",
    ] {
        let mut changed = original.clone();
        let body = changed["jobs"]["inventory"]["steps"][0]["run"]
            .as_str()
            .unwrap();
        assert_eq!(body.matches(guard).count(), 1);
        changed["jobs"]["inventory"]["steps"][0]["run"] = body.replace(guard, "").into();
        assert!(validate_local_routing(&changed).is_err(), "missing {guard}");
    }
    let mut changed = original.clone();
    changed["jobs"]["inventory"]["runs-on"] = "${{ matrix.runner }}".into();
    assert!(validate_local_routing(&changed).is_err());
}

#[test]
fn generated_asset_cleanup_requires_completed_real_consumers() {
    use openpilot_dev_tools::ci_policy::validate_private_job_layout;
    let workflow =
        parse(&fs::read_to_string(root().join(".github/workflows/tests.yaml")).unwrap()).unwrap();
    let job = &workflow["jobs"]["build_release"];
    validate_private_job_layout(job).unwrap();
    let steps = job["steps"].as_array().unwrap();
    let generated = steps
        .iter()
        .position(|s| s["id"] == "generated_assets_check")
        .unwrap();
    let cleanup = steps
        .iter()
        .position(|s| s["id"] == "ci296_source_cleanup")
        .unwrap();
    let mut changed = job.clone();
    changed["steps"][cleanup]["if"] = steps[cleanup]["if"]
        .as_str()
        .unwrap()
        .replace(" && steps.generated_assets_check.outcome == 'success'", "")
        .into();
    assert!(validate_private_job_layout(&changed).is_err());
    for mode in ["renamed", "continue", "always"] {
        let mut changed = job.clone();
        match mode {
            "renamed" => changed["steps"][generated]["id"] = "unbound_assets".into(),
            "continue" => changed["steps"][generated]["continue-on-error"] = "true".into(),
            _ => changed["steps"][generated]["if"] = "${{ always() }}".into(),
        }
        assert!(validate_private_job_layout(&changed).is_err(), "{mode}");
    }
}

#[test]
fn native_arm_transport_rejects_unbound_or_shared_shell() {
    use openpilot_dev_tools::ci_policy::validate_private_job_layout;
    let workflow = parse(
        &fs::read_to_string(root().join(".github/workflows/python-rust-523-native-arm.yml"))
            .unwrap(),
    )
    .unwrap();
    validate_local_routing(&workflow).unwrap();
    let job = &workflow["jobs"]["carrot-man-runtime"];
    validate_private_job_layout(job).unwrap();
    let prepare = job["steps"]
        .as_array()
        .unwrap()
        .iter()
        .position(|s| s["name"] == "Prepare native ARM command transport")
        .unwrap();
    for body in ["true\n", "/usr/bin/bash tools/ci/native-arm prepare\n"] {
        let mut changed = job.clone();
        changed["steps"][prepare]["run"] = body.into();
        assert!(validate_private_job_layout(&changed).is_err());
    }
}

fn native_arm_reuse_inputs() -> (serde_json::Value, serde_json::Value) {
    let load = |name| {
        parse(&fs::read_to_string(root().join(format!(".github/workflows/{name}"))).unwrap())
            .unwrap()
    };
    (load("rust.yml"), load("rust-native-arm.yml"))
}

#[test]
fn native_arm_reuse_requires_exact_caller_leaf_roster_and_success_gate() {
    use openpilot_dev_tools::ci_policy::validate_native_arm_reuse;
    let (main, callee) = native_arm_reuse_inputs();
    validate_native_arm_reuse(&main, &callee).unwrap();
    for (key, value) in [
        ("uses", "./.github/workflows/missing.yml"),
        (
            "uses",
            "bin9208/openpilot-rust/.github/workflows/rust-native-arm.yml@dev",
        ),
        ("if", "${{ false }}"),
        ("continue-on-error", "true"),
    ] {
        let mut changed = main.clone();
        changed["jobs"]["native-arm"][key] = value.into();
        assert!(
            validate_native_arm_reuse(&changed, &callee).is_err(),
            "caller {key}"
        );
    }
    for name in ["PLANNER", "WORKSPACE", "NATIVE_ARM"] {
        for mode in ["binding", "test"] {
            let mut changed = main.clone();
            let step = &mut changed["jobs"]["fast"]["steps"][0];
            if mode == "binding" {
                step["env"].as_object_mut().unwrap().remove(name);
            } else {
                step["run"] = step["run"]
                    .as_str()
                    .unwrap()
                    .replace(&format!("test \"${name}\" = success\n"), "")
                    .into();
            }
            assert!(
                validate_native_arm_reuse(&changed, &callee).is_err(),
                "main {name}/{mode}"
            );
        }
    }
    for mode in ["missing", "duplicate"] {
        let mut changed = main.clone();
        let needs = changed["jobs"]["fast"]["needs"].as_array_mut().unwrap();
        if mode == "missing" {
            needs.retain(|v| v != "native-arm");
        } else {
            needs.push("native-arm".into());
        }
        assert!(
            validate_native_arm_reuse(&changed, &callee).is_err(),
            "main needs {mode}"
        );
    }
    let mut changed = callee.clone();
    changed["concurrency"]["group"] = "rust-${{ github.event_name }}-${{ github.ref }}".into();
    assert!(
        validate_native_arm_reuse(&main, &changed).is_err(),
        "caller concurrency collision"
    );
    for name in [
        "planner-runtime",
        "carrot-man-runtime",
        "webrtc-runtime",
        "webcam-runtime",
        "xiaoge-runtime",
        "radard-runtime",
        "radar-arm",
        "carrot-navi-arm",
        "native-arm-checks",
    ] {
        let mut changed = callee.clone();
        changed["jobs"].as_object_mut().unwrap().remove(name);
        assert!(
            validate_native_arm_reuse(&main, &changed).is_err(),
            "missing {name}"
        );
        for key in ["if", "continue-on-error"] {
            let mut changed = callee.clone();
            changed["jobs"][name][key] = "${{ false }}".into();
            assert!(
                validate_native_arm_reuse(&main, &changed).is_err(),
                "{name}/{key}"
            );
        }
    }
    for key in ["workflow_call", "extra-job"] {
        let mut changed = callee.clone();
        if key == "workflow_call" {
            changed["on"].as_object_mut().unwrap().remove(key);
        } else {
            changed["jobs"][key] = changed["jobs"]["planner-runtime"].clone();
        }
        assert!(validate_native_arm_reuse(&main, &changed).is_err(), "{key}");
    }
    for (name, stage) in [
        (
            "planner-runtime",
            "Compile and check the complete native planner",
        ),
        (
            "carrot-man-runtime",
            "Build owner and unchanged source bindings",
        ),
        (
            "webrtc-runtime",
            "Compile and check both native service entrypoints",
        ),
        (
            "webcam-runtime",
            "Build native daemon and check all webcam targets",
        ),
        (
            "xiaoge-runtime",
            "Compile and check all native Xiaoge targets",
        ),
        (
            "radard-runtime",
            "Require Rust types, tests, complete build and Clippy",
        ),
        (
            "radar-arm",
            "Build and execute generic ARM RadarCAN tests and artifacts",
        ),
        (
            "carrot-navi-arm",
            "Build and inspect the actual native ARM executable",
        ),
    ] {
        let mut changed = callee.clone();
        changed["jobs"][name]["timeout-minutes"] = "5".into();
        assert!(
            validate_native_arm_reuse(&main, &changed).is_err(),
            "timeout {name}"
        );
        let mut changed = callee.clone();
        changed["jobs"][name]["steps"]
            .as_array_mut()
            .unwrap()
            .retain(|s| s["name"] != stage);
        assert!(
            validate_native_arm_reuse(&main, &changed).is_err(),
            "functional stage {name}"
        );
        let position = callee["jobs"][name]["steps"]
            .as_array()
            .unwrap()
            .iter()
            .position(|s| s["name"] == stage)
            .unwrap();
        for mode in ["duplicate", "skip", "continue"] {
            let mut changed = callee.clone();
            let steps = changed["jobs"][name]["steps"].as_array_mut().unwrap();
            match mode {
                "duplicate" => steps.push(steps[position].clone()),
                "skip" => steps[position]["if"] = "${{ (github.event_name != 'pull_request' || github.event.pull_request.head.repo.full_name == github.repository) && (false) }}".into(),
                _ => steps[position]["continue-on-error"] = "true".into(),
            }
            assert!(
                validate_native_arm_reuse(&main, &changed).is_err(),
                "{name}/{mode}"
            );
        }
    }
}

#[test]
fn native_arm_main_context_rejects_forks_wrong_callers_and_event_refs() {
    use openpilot_dev_tools::ci_policy::NATIVE_ARM_MAIN_CONTEXT;
    let repository = "bin9208/openpilot-rust";
    let context = |event: &str, reference: &str, base: &str, head: &str| {
        BTreeMap::from([
            ("CI296_NATIVE_ARM_CALLER".into(), "rust.yml".into()),
            ("GITHUB_REPOSITORY".into(), repository.into()),
            ("GITHUB_EVENT_NAME".into(), event.into()),
            ("GITHUB_REF".into(), reference.into()),
            ("GITHUB_BASE_REF".into(), base.into()),
            ("CI296_PR_HEAD_REPOSITORY".into(), head.into()),
            (
                "GITHUB_WORKFLOW_REF".into(),
                format!("{repository}/.github/workflows/rust.yml@{reference}"),
            ),
        ])
    };
    for (event, reference, base, head) in [
        ("push", "refs/heads/dev", "", ""),
        ("push", "refs/heads/main", "", ""),
        ("pull_request", "refs/pull/343/merge", "dev", repository),
        ("pull_request", "refs/pull/343/merge", "main", repository),
        (
            "workflow_dispatch",
            "refs/heads/codex/feat-523-python-rust",
            "",
            "",
        ),
        ("workflow_dispatch", "refs/tags/test-native-arm", "", ""),
    ] {
        assert!(
            run_shell(
                NATIVE_ARM_MAIN_CONTEXT,
                &context(event, reference, base, head)
            )
            .unwrap()
            .success()
        );
    }
    for (event, reference, base, head) in [
        ("push", "refs/heads/other", "", ""),
        (
            "pull_request",
            "refs/pull/343/merge",
            "dev",
            "other/openpilot-rust",
        ),
        ("pull_request", "refs/pull/343/head", "dev", repository),
        ("pull_request", "refs/pull/343/merge", "other", repository),
        ("pull_request", "refs/pull/343/merge", "dev", ""),
        ("pull_request_target", "refs/heads/dev", "dev", repository),
        ("workflow_dispatch", "refs/heads/", "", ""),
    ] {
        let mut env = context(event, reference, base, head);
        env.insert("SOURCE_ALLOWED".into(), "true".into());
        assert!(!run_shell(NATIVE_ARM_MAIN_CONTEXT, &env).unwrap().success());
    }
    for key in [
        "GITHUB_WORKFLOW_REF",
        "CI296_NATIVE_ARM_CALLER",
        "GITHUB_REPOSITORY",
    ] {
        let mut env = context("push", "refs/heads/dev", "", "");
        env.insert(key.into(), "wrong".into());
        assert!(
            !run_shell(NATIVE_ARM_MAIN_CONTEXT, &env).unwrap().success(),
            "{key}"
        );
    }
    assert!(
        fs::read_to_string(root().join("tools/ci/native-arm"))
            .unwrap()
            .contains(NATIVE_ARM_MAIN_CONTEXT.trim_end())
    );
}

#[test]
fn native_arm_boundary_keeps_exact_host_and_main_or_candidate_context() {
    let (_, callee) = native_arm_reuse_inputs();
    validate_local_routing(&callee).unwrap();
    for (path, value) in [
        (vec!["runs-on"], "${{ matrix.runner }}"),
        (vec!["runs-on"], "ubuntu-24.04"),
        (vec!["env", "CI296_NATIVE_ARM"], "0"),
        (vec!["env", "CI296_NATIVE_ARM_CALLER"], "other.yml"),
        (
            vec!["env", "CI296_PR_HEAD_REPOSITORY"],
            "bin9208/openpilot-rust",
        ),
    ] {
        let mut changed = callee.clone();
        let mut slot = &mut changed["jobs"]["planner-runtime"];
        for key in path {
            slot = &mut slot[key];
        }
        *slot = value.into();
        assert!(validate_local_routing(&changed).is_err());
    }
    let original = callee["jobs"]["planner-runtime"]["steps"][0]["run"]
        .as_str()
        .unwrap();
    for (from, to) in [
        ("test \"$RUNNER_ARCH\" = ARM64", "true"),
        ("test \"$(uname -m)\" = aarch64", "true"),
        ("GITHUB_WORKFLOW_REF", "IGNORED_WORKFLOW_REF"),
    ] {
        let mut changed = callee.clone();
        assert!(original.contains(from));
        changed["jobs"]["planner-runtime"]["steps"][0]["run"] = original.replace(from, to).into();
        assert!(validate_local_routing(&changed).is_err());
    }
    for name in [
        "python-rust-523-native-arm.yml",
        "python-rust-523-native-arm-more.yml",
    ] {
        let candidate =
            parse(&fs::read_to_string(root().join(".github/workflows").join(name)).unwrap())
                .unwrap();
        validate_local_routing(&candidate).unwrap();
        for job in candidate["jobs"].as_object().unwrap().values() {
            let body = job["steps"][0]["run"].as_str().unwrap();
            assert!(body.contains("test \"$GITHUB_EVENT_NAME\" = push"));
            assert!(body.contains("refs/heads/codex/feat-523-python-rust"));
        }
    }
}

#[test]
fn native_arm_result_gate_rejects_each_non_success_or_missing_result() {
    use openpilot_dev_tools::ci_policy::validate_native_arm_reuse;
    let (main, callee) = native_arm_reuse_inputs();
    let gate = &callee["jobs"]["native-arm-checks"];
    let script = gate["steps"][0]["run"].as_str().unwrap();
    let success = BTreeMap::from([
        ("SOURCE_ALLOWED".into(), "true".into()),
        ("PLANNER".into(), "success".into()),
        ("CARROT_MAN".into(), "success".into()),
        ("WEBRTC".into(), "success".into()),
        ("WEBCAM".into(), "success".into()),
        ("XIAOGE".into(), "success".into()),
        ("RADARD".into(), "success".into()),
        ("RADAR_ARM".into(), "success".into()),
        ("CARROT_NAVI_ARM".into(), "success".into()),
    ]);
    assert!(run_shell(script, &success).unwrap().success());
    for key in success.keys() {
        for value in [
            Some("failure"),
            Some("cancelled"),
            Some("skipped"),
            Some(""),
            None,
        ] {
            let mut env = success.clone();
            if let Some(value) = value {
                env.insert(key.clone(), value.into());
            } else {
                env.remove(key);
            }
            assert!(
                !run_shell(script, &env).unwrap().success(),
                "{key}={value:?}"
            );
        }
    }
    for key in [
        "PLANNER",
        "CARROT_MAN",
        "WEBRTC",
        "WEBCAM",
        "XIAOGE",
        "RADARD",
        "RADAR_ARM",
        "CARROT_NAVI_ARM",
    ] {
        let mut changed = callee.clone();
        changed["jobs"]["native-arm-checks"]["steps"][0]["run"] = script
            .replace(&format!("test \"${key}\" = success\n"), "")
            .into();
        assert!(
            validate_native_arm_reuse(&main, &changed).is_err(),
            "removed {key} test"
        );
    }
}

#[test]
fn native_arm_required_workflow_cannot_be_absent() {
    let mut files = inputs();
    assert!(files.iter().any(|(name, _)| name == "rust-native-arm.yml"));
    files.retain(|(name, _)| name != "rust-native-arm.yml");
    assert!(
        check_workflow_policy(&files)
            .unwrap_err()
            .contains("missing workflow rust-native-arm.yml")
    );
}

#[test]
fn native_arm_proxies_require_the_complete_unconditional_callee_result() {
    use openpilot_dev_tools::ci_policy::validate_native_arm_reuse;
    let (main, callee) = native_arm_reuse_inputs();
    validate_native_arm_reuse(&main, &callee).unwrap();
    for name in ["radar-arm", "carrot-navi-arm"] {
        for mutation in [
            "needs",
            "optional",
            "skip",
            "success-only",
            "host",
            "binding",
            "source",
            "body",
            "step-skip",
        ] {
            let mut changed = main.clone();
            let job = &mut changed["jobs"][name];
            match mutation {
                "needs" => job["needs"] = serde_json::json!([]),
                "optional" => job["continue-on-error"] = true.into(),
                "skip" => job["if"] = "${{ false }}".into(),
                "success-only" => job["if"] = "${{ success() }}".into(),
                "host" => job["runs-on"] = "ubuntu-24.04-arm".into(),
                "binding" => {
                    job["steps"][0]["env"]["NATIVE_ARM"] = "${{ needs.workspace.result }}".into()
                }
                "source" => job["steps"][0]["env"]["SOURCE_ALLOWED"] = "true".into(),
                "body" => job["steps"][0]["run"] = "true\n".into(),
                _ => job["steps"][0]["if"] = "${{ false }}".into(),
            }
            assert!(
                validate_native_arm_reuse(&changed, &callee).is_err(),
                "{name}/{mutation}"
            );
        }
        let script = main["jobs"][name]["steps"][0]["run"].as_str().unwrap();
        let good = BTreeMap::from([
            ("SOURCE_ALLOWED".into(), "true".into()),
            ("NATIVE_ARM".into(), "success".into()),
        ]);
        assert!(run_shell(script, &good).unwrap().success());
        for key in good.keys() {
            for state in [
                Some("failure"),
                Some("cancelled"),
                Some("skipped"),
                Some(""),
                Some("unknown"),
                None,
            ] {
                let mut env = good.clone();
                if let Some(state) = state {
                    env.insert(key.clone(), state.into());
                } else {
                    env.remove(key);
                }
                assert!(
                    !run_shell(script, &env).unwrap().success(),
                    "{name}/{key}/{state:?}"
                );
            }
        }
    }
    for key in ["if", "continue-on-error"] {
        let mut changed = callee.clone();
        changed["jobs"]["native-arm-checks"]["steps"][0][key] = "${{ false }}".into();
        assert!(
            validate_native_arm_reuse(&main, &changed).is_err(),
            "aggregate step {key}"
        );
    }
}

#[test]
fn native_arm_lfs_main_and_candidate_contexts_stay_exact() {
    use openpilot_dev_tools::ci_policy::NATIVE_ARM_MAIN_CONTEXT;
    let helper = fs::read_to_string(root().join("tools/ci/prepare-lfs-source-535")).unwrap();
    let common = helper
        .split("  if [[ ${GITHUB_WORKFLOW_REF:-}")
        .next()
        .unwrap();
    for required in [
        "${RUNNER_ARCH:-} == ARM64",
        "${GITHUB_ACTIONS:-} == true",
        "${GITHUB_REPOSITORY:-} == bin9208/openpilot-rust",
        "${RUNNER_ENVIRONMENT:-} == github-hosted",
        "${RUNNER_OS:-} == Linux",
        "$(uname -s) == Linux",
        "$(uname -m) == aarch64",
        "$selection == xiaoge",
        "${GITHUB_JOB:-} == xiaoge-runtime",
        "${CI296_NATIVE_ARM:-} == 1",
        "${LOCAL_CI_ARCH:-} == aarch64",
    ] {
        assert!(
            common.contains(required),
            "missing common ARM LFS guard {required}"
        );
    }
    let context = helper
        .split_once("  if [[ ${GITHUB_WORKFLOW_REF:-}")
        .unwrap()
        .1
        .split_once("\n  fi\nelse\n")
        .unwrap()
        .0;
    let script = format!("fail() {{ return 1; }}\nif [[ ${{GITHUB_WORKFLOW_REF:-}}{context}\nfi\n");
    assert!(script.contains(NATIVE_ARM_MAIN_CONTEXT));
    let mut env = BTreeMap::from([
        ("GITHUB_REPOSITORY".into(), "bin9208/openpilot-rust".into()),
        ("CI296_NATIVE_ARM_CALLER".into(), "rust.yml".into()),
        ("GITHUB_EVENT_NAME".into(), "push".into()),
        ("GITHUB_REF".into(), "refs/heads/dev".into()),
        (
            "GITHUB_WORKFLOW_REF".into(),
            "bin9208/openpilot-rust/.github/workflows/rust.yml@refs/heads/dev".into(),
        ),
    ]);
    assert!(run_shell(&script, &env).unwrap().success());
    for (key, value) in [
        ("CI296_NATIVE_ARM_CALLER", ""),
        ("GITHUB_EVENT_NAME", "pull_request"),
        ("GITHUB_REF", "refs/heads/foreign"),
        ("GITHUB_REPOSITORY", "foreign/repository"),
        (
            "GITHUB_WORKFLOW_REF",
            "bin9208/openpilot-rust/.github/workflows/rust-native-arm.yml@refs/heads/dev",
        ),
    ] {
        let mut bad = env.clone();
        bad.insert(key.into(), value.into());
        assert!(
            !run_shell(&script, &bad).unwrap().success(),
            "{key}/{value}"
        );
    }
    env.insert("GITHUB_EVENT_NAME".into(), "pull_request".into());
    env.insert("GITHUB_REF".into(), "refs/pull/343/merge".into());
    env.insert(
        "GITHUB_WORKFLOW_REF".into(),
        "bin9208/openpilot-rust/.github/workflows/rust.yml@refs/pull/343/merge".into(),
    );
    env.insert("GITHUB_BASE_REF".into(), "dev".into());
    env.insert(
        "CI296_PR_HEAD_REPOSITORY".into(),
        "bin9208/openpilot-rust".into(),
    );
    assert!(run_shell(&script, &env).unwrap().success());
    env.insert(
        "CI296_PR_HEAD_REPOSITORY".into(),
        "foreign/repository".into(),
    );
    assert!(!run_shell(&script, &env).unwrap().success());
    env.insert("GITHUB_EVENT_NAME".into(), "push".into());
    env.insert(
        "GITHUB_REF".into(),
        "refs/heads/codex/feat-523-python-rust".into(),
    );
    env.insert("GITHUB_WORKFLOW_REF".into(), "bin9208/openpilot-rust/.github/workflows/python-rust-523-native-arm-remaining.yml@refs/heads/codex/feat-523-python-rust".into());
    env.remove("CI296_NATIVE_ARM_CALLER");
    assert!(run_shell(&script, &env).unwrap().success());
    env.insert("GITHUB_REF".into(), "refs/heads/dev".into());
    assert!(!run_shell(&script, &env).unwrap().success());
}

#[test]
fn native_arm_final_binary_evidence_requires_exact_trusted_always_condition() {
    use openpilot_dev_tools::ci_policy::{
        TRUSTED_ALWAYS, TRUSTED_SOURCE, validate_native_arm_reuse,
    };
    let (main, callee) = native_arm_reuse_inputs();
    validate_native_arm_reuse(&main, &callee).unwrap();
    let label = "Preserve bounded final binaries and their identities";
    let index = callee["jobs"]["radard-runtime"]["steps"]
        .as_array()
        .unwrap()
        .iter()
        .position(|step| step["name"] == label)
        .unwrap();
    assert_eq!(
        callee["jobs"]["radard-runtime"]["steps"][index]["if"],
        TRUSTED_ALWAYS
    );
    for condition in [
        None,
        Some(TRUSTED_SOURCE),
        Some("${{ always() }}"),
        Some("${{ false }}"),
        Some("${{ success() }}"),
    ] {
        let mut changed = callee.clone();
        let step = &mut changed["jobs"]["radard-runtime"]["steps"][index];
        if let Some(condition) = condition {
            step["if"] = condition.into();
        } else {
            step.as_object_mut().unwrap().remove("if");
        }
        assert!(
            validate_native_arm_reuse(&main, &changed).is_err(),
            "{condition:?}"
        );
    }
}

#[test]
fn native_arm_eight_prepared_sdks_require_before_after_binding_and_cleanup_success() {
    use openpilot_dev_tools::ci_policy::validate_private_job_layout;
    let (main, callee) = native_arm_reuse_inputs();
    // main X64/다른 옛 후보는 그대로 두고 main8과 #572 final-two2를 모두 검증한다.
    validate_private_job_layout(&main["jobs"]["webcam-runtime"]).unwrap();
    let candidate = parse(
        &fs::read_to_string(
            root().join(".github/workflows/python-rust-523-native-arm-final-two.yml"),
        )
        .unwrap(),
    )
    .unwrap();
    let main_roles = [
        "planner-runtime",
        "carrot-man-runtime",
        "webrtc-runtime",
        "webcam-runtime",
        "xiaoge-runtime",
        "radard-runtime",
        "radar-arm",
        "carrot-navi-arm",
    ];
    let contexts = main_roles
        .iter()
        .map(|name| (*name, &callee["jobs"][*name]))
        .chain(
            ["radar-arm", "carrot-navi-arm"]
                .into_iter()
                .map(|name| (name, &candidate["jobs"][name])),
        );
    for (name, original) in contexts {
        validate_private_job_layout(original).unwrap();
        let stages = original["steps"].as_array().unwrap();
        let before = stages.iter().position(|s| s["id"] == "arm530_sdk").unwrap();
        let after = stages
            .iter()
            .position(|s| s["id"] == "arm530_sdk_final")
            .unwrap();
        let dependency_name = match name {
            "planner-runtime" => "Reserve capacity and install original planner dependencies",
            "carrot-man-runtime" => "Reserve runner capacity and install pinned dependencies",
            "webrtc-runtime" => "Reserve capacity and install pinned source and native providers",
            "webcam-runtime" => "Install exact source closure and native capture providers",
            "xiaoge-runtime" => "Reserve capacity and install Xiaoge build prerequisites",
            "radard-runtime" => "Reserve capacity and install original oracle dependencies",
            "radar-arm" => "Reserve runner capacity and install generic ARM dependencies",
            "carrot-navi-arm" => {
                "Reserve runner capacity and install native ARM build dependencies"
            }
            _ => unreachable!(),
        };
        let dependency = stages
            .iter()
            .position(|s| s["name"] == dependency_name)
            .unwrap();
        let first_maintained = stages
            .iter()
            .position(|s| s["shell"] == "openpilot-local-shell {0}")
            .unwrap();
        let source_cleanup = stages
            .iter()
            .position(|s| s["id"] == "ci296_source_cleanup")
            .unwrap();
        let common = stages
            .iter()
            .position(|s| s["name"] == "Clean common owned temporary paths after uploaded source")
            .unwrap();
        for mutation in [
            "missing-before",
            "missing-after",
            "duplicate-before",
            "before-skip",
            "after-success-only",
            "after-optional",
            "before-timeout",
            "pin",
            "command",
            "order",
            "source-cleanup",
            "common-cleanup",
            "reinstall",
            "missing-verify",
        ] {
            let mut changed = original.clone();
            let steps = changed["steps"].as_array_mut().unwrap();
            match mutation {
                "missing-before" => {
                    steps.remove(before);
                }
                "missing-after" => {
                    steps.remove(after);
                }
                "duplicate-before" => steps.push(steps[before].clone()),
                "before-skip" => steps[before]["if"] = "${{ false }}".into(),
                "after-success-only" => {
                    steps[after]["if"] = openpilot_dev_tools::ci_policy::TRUSTED_SOURCE.into()
                }
                "after-optional" => steps[after]["continue-on-error"] = true.into(),
                "before-timeout" => steps[before]["timeout-minutes"] = "1".into(),
                "pin" => steps[before]["env"]["ARM530_HELPER_SHA256"] = "0".repeat(64).into(),
                "command" => steps[before]["run"] = "true\n".into(),
                "order" => steps.swap(before, first_maintained),
                "source-cleanup" | "common-cleanup" => {
                    let at = if mutation == "source-cleanup" {
                        source_cleanup
                    } else {
                        common
                    };
                    let condition = steps[at]["if"].as_str().unwrap();
                    assert_eq!(
                        condition
                            .matches(" && steps.arm530_sdk_final.outcome == 'success'")
                            .count(),
                        1
                    );
                    steps[at]["if"] = condition
                        .replace(" && steps.arm530_sdk_final.outcome == 'success'", "")
                        .into();
                }
                "reinstall" => {
                    steps[dependency]["run"] = format!(
                        "{}\nrustup target add aarch64-unknown-linux-gnu\n",
                        steps[dependency]["run"].as_str().unwrap()
                    )
                    .into()
                }
                "missing-verify" => {
                    steps[dependency]["run"] = steps[dependency]["run"]
                        .as_str()
                        .unwrap()
                        .replace(
                            "prepare-readonly-arm-rust-530\" verify",
                            "prepare-readonly-arm-rust-530\" ignored",
                        )
                        .into()
                }
                _ => unreachable!(),
            }
            assert!(
                validate_private_job_layout(&changed).is_err(),
                "{name}/{mutation}"
            );
        }
    }
}
