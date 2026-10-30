//! Native counterparts of tools/ci/tests/test_workflow_policy.py.
use openpilot_dev_tools::ci_policy::{check_workflow_policy, parse, run_shell};
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
            seal["sha256"].as_str().unwrap(),
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
            seal["sha256"].as_str().unwrap(),
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
    assert_eq!(covered, 60, "maintained private source roster changed");
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
    let workflow =
        parse(&fs::read_to_string(root().join(".github/workflows/rust.yml")).unwrap()).unwrap();
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
    assert_eq!(routed, 8);
    calls.sort_unstable();
    assert_eq!(
        calls,
        [
            "carrot-man-runtime",
            "planner-runtime",
            "radar-arm",
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
                let finish = at("Finish emulated architecture evidence");
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
    assert_eq!((sources, arm), (60, 8));
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
    let workflow =
        parse(&fs::read_to_string(root().join(".github/workflows/rust.yml")).unwrap()).unwrap();
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
            .find(|s| s["name"] == "Finish emulated architecture evidence")
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
    assert_eq!(covered, 8);
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

fn webrtc_retention_job() -> serde_json::Value {
    parse(&fs::read_to_string(root().join(".github/workflows/rust.yml")).unwrap()).unwrap()["jobs"]
        ["webrtc-runtime"]
        .clone()
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
    let job = webrtc_retention_job();
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
                        !webrtc_cleanup_condition_passes(condition, &outcomes, false, successful),
                        "{cleanup}: {id}={outcome:?} deleted failed evidence"
                    );
                }
            }
        }
    }
}

#[test]
fn webrtc_retention_policy_rejects_guard_removal_and_unbound_required_stages() {
    use openpilot_dev_tools::ci_policy::validate_private_job_layout;
    let job = webrtc_retention_job();
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
            .chain(WEBRTC_REQUIRED_OUTCOMES.map(|id| format!("steps.{id}.outcome == 'success'")))
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
