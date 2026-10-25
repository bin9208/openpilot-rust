//! #523: 구조 양성 1개·변조 19개·Miri 실패 전파 1개. 실행 회귀 3개는 별도다.
#[path = "planner_ci_policy/contract.rs"]
mod contract;

use contract::{assert_contract, by_id, index, inputs, load, step_mut, steps, RESULTS};
use openpilot_dev_tools::{
    ci_policy::{run_shell, TRUSTED_SOURCE},
    qa_process::{CommandSpec, NativeRunner, Runner},
};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap, ffi::OsString, fs, os::unix::fs::PermissionsExt, panic::catch_unwind,
    time::Duration,
};

#[test]
fn planner_host_arm_and_memory_results_are_required() {
    let (main, native) = inputs();
    assert_contract(&main, &native);
    let jobs = &main["jobs"];
    let runtime = &jobs["planner-runtime"];
    assert_eq!(
        runtime["strategy"],
        json!({"fail-fast":"false", "matrix":{"runner":["local-x64"]}})
    );
    assert_eq!(
        runtime["env"]["LOCAL_CI_ARCH"],
        "${{ matrix.runner == 'local-arm64-qemu' && 'aarch64' || 'x86_64' }}"
    );
    for name in ["planner-runtime", "planner-memory"] {
        let job = &jobs[name];
        assert_eq!(job["runs-on"], "ubuntu-24.04");
        let boundary = &steps(job)[0];
        assert!(boundary.get("if").is_none());
        assert_eq!(boundary["env"]["SOURCE_ALLOWED"], TRUSTED_SOURCE);
        for marker in [
            "test \"$GITHUB_ACTIONS\" = true",
            "test \"$GITHUB_REPOSITORY\" = bin9208/openpilot-rust",
            "test \"$RUNNER_ENVIRONMENT\" = github-hosted",
            "test \"$RUNNER_OS\" = Linux",
            "test \"$RUNNER_ARCH\" = X64",
            "test \"$(uname -m)\" = x86_64",
        ] {
            assert!(boundary["run"]
                .as_str()
                .unwrap()
                .lines()
                .any(|line| line == marker));
        }
    }
    let alias = ".ci296-source-rust--planner-runtime";
    assert_eq!(runtime["env"]["CI296_ALIAS_BASENAME"], alias);
    let transport = steps(runtime)
        .iter()
        .enumerate()
        .filter(|(_, step)| {
            step["uses"] == format!("./{alias}/source/.github/actions/local-arm")
                && step["with"]["cleanup"] != "true"
        })
        .map(|(at, _)| at)
        .collect::<Vec<_>>();
    assert_eq!(transport.len(), 1);
    let admission = index(runtime, by_id(runtime, "ci296_source_admission"));
    let preflight = steps(runtime)
        .iter()
        .position(|s| {
            s["run"]
                .as_str()
                .unwrap_or("")
                .contains("test -x /usr/bin/qemu-aarch64-static")
        })
        .unwrap();
    assert!(admission < preflight && preflight < transport[0]);
    assert_eq!(steps(runtime)[transport[0]]["if"], TRUSTED_SOURCE);
    assert!(steps(runtime)[transport[0] + 1..]
        .iter()
        .any(|s| s["shell"] == "openpilot-local-shell {0}"));
    // 실제 gate shell의 성공/실패 전파까지 원래 양성 검사의 범위에 포함한다.
    assert_gate_results(
        &jobs["fast"]["steps"][0],
        &["PLANNER", "PLANNER_MEMORY", "NATIVE_ARM", "SOURCE_ALLOWED"],
    );
    let variables = RESULTS
        .map(|(variable, _)| variable)
        .into_iter()
        .chain(["SOURCE_ALLOWED"])
        .collect::<Vec<_>>();
    assert_gate_results(&native["jobs"]["native-arm-checks"]["steps"][0], &variables);
    for job in [&jobs["planner-runtime"], &jobs["planner-memory"]]
        .into_iter()
        .chain(native["jobs"].as_object().unwrap().values())
    {
        for step in steps(job) {
            if let Some(body) = step.get("run") {
                let mut spec =
                    CommandSpec::new(vec!["bash".into(), "-n".into()], Duration::from_secs(5));
                spec.input = Some(body.as_str().unwrap().as_bytes().to_vec());
                let output = NativeRunner.run(&spec).unwrap();
                assert_eq!(
                    output.code,
                    0,
                    "{}: {}",
                    step["name"],
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }
    }
}

fn assert_gate_results(gate: &Value, variables: &[&str]) {
    assert_eq!(gate["env"]["SOURCE_ALLOWED"], TRUSTED_SOURCE);
    let mut environment = std::env::vars().collect::<BTreeMap<_, _>>();
    for key in gate["env"].as_object().unwrap().keys() {
        environment.insert(key.clone(), "success".into());
    }
    environment.insert("SOURCE_ALLOWED".into(), "true".into());
    let script = gate["run"].as_str().unwrap();
    assert!(run_shell(script, &environment).unwrap().success());
    for name in variables {
        for state in ["failure", "cancelled", "skipped", "", "false", "unknown"] {
            let mut changed = environment.clone();
            changed.insert((*name).into(), state.into());
            assert!(
                !run_shell(script, &changed).unwrap().success(),
                "{name}/{state}"
            );
        }
    }
}

#[test]
fn planner_miri_failure_cannot_be_hidden_by_log_capture() {
    let main = load("rust.yml");
    let step = steps(&main["jobs"]["planner-memory"])
        .iter()
        .find(|step| step["run"].as_str().unwrap_or("").contains("MIRIFLAGS="))
        .unwrap();
    let temp = tempfile::tempdir().unwrap();
    let commands = temp.path().join("bin");
    let workspace = temp.path().join("workspace");
    let tools = workspace.join("rust/tools");
    fs::create_dir(&commands).unwrap();
    fs::create_dir_all(&tools).unwrap();
    // 원문 Bash는 그대로 둔다. guard bootstrap이 가짜 cargo를 먼저 실행하는
    // 오탐과 실제 SDK/build 의존을 막기 위해 admission 경계만 성공 fixture로 둔다.
    for (path, body) in [
        (commands.join("python"), "#!/bin/sh\nexit 0\n"),
        (
            commands.join("cargo"),
            "#!/bin/sh\nprintf \"failed\\n\" >> \"$MIRI_FAILURE_CALLS\"\nexit 42\n",
        ),
        (tools.join("check-disk-space"), "#!/bin/sh\nexit 0\n"),
    ] {
        fs::write(&path, body).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let calls = temp.path().join("calls");
    let inherited_path = std::env::var_os("PATH").unwrap();
    let path = std::env::join_paths(
        std::iter::once(commands).chain(std::env::split_paths(&inherited_path)),
    )
    .unwrap();
    let mut spec = CommandSpec::new(
        vec![
            "bash".into(),
            "-e".into(),
            "-c".into(),
            step["run"].as_str().unwrap().into(),
        ],
        Duration::from_secs(10),
    );
    spec.cwd = Some(workspace.clone());
    spec.set_os_env(BTreeMap::from([
        (OsString::from("PATH"), path),
        (
            OsString::from("RUNNER_TEMP"),
            temp.path().as_os_str().to_owned(),
        ),
        (
            OsString::from("GITHUB_WORKSPACE"),
            workspace.into_os_string(),
        ),
        (
            OsString::from("MIRI_FAILURE_CALLS"),
            calls.as_os_str().to_owned(),
        ),
    ]));
    let output = NativeRunner.run(&spec).unwrap();
    assert_eq!(output.code, 42, "{output:?}");
    assert_eq!(
        fs::read_to_string(calls)
            .unwrap()
            .lines()
            .collect::<Vec<_>>(),
        ["failed"]
    );
}

fn assert_rejected(change: impl FnOnce(&mut Value, &mut Value)) {
    let (mut main, mut native) = inputs();
    // baseline과 변조 적용은 catch 밖이다. 깨진 원문/fixture를 거부 성공으로 세지 않는다.
    assert_contract(&main, &native);
    change(&mut main, &mut native);
    assert!(
        catch_unwind(|| assert_contract(&main, &native)).is_err(),
        "weakened Planner contract accepted"
    );
}

// 원본 parametrize의 순서와 scope를 그대로 유지하면서 각각 독립 selector를 제공한다.
macro_rules! rejected {
    ($name:ident, $change:expr) => {
        #[test]
        fn $name() {
            assert_rejected($change);
        }
    };
}

rejected!(planner_rejects_radar_proxy_noop, |main, _native| {
    step_mut(
        &mut main["jobs"]["radar-arm"],
        "Require native ARM family result",
    )["run"] = "true\n".into();
});
rejected!(planner_rejects_navi_proxy_missing_needs, |main, _native| {
    main["jobs"]["carrot-navi-arm"]["needs"] = json!([]);
});
rejected!(planner_rejects_xiaoge_timeout_change, |_main, native| {
    native["jobs"]["xiaoge-runtime"]["timeout-minutes"] = "70".into();
});
rejected!(planner_rejects_radar_lifecycle_skip, |_main, native| {
    step_mut(
        &mut native["jobs"]["radar-arm"],
        "Require actual original and native ARM IPC and signal lifecycle",
    )["if"] = "${{ false }}".into();
});
rejected!(
    planner_rejects_radard_result_cross_binding,
    |_main, native| {
        step_mut(
            &mut native["jobs"]["native-arm-checks"],
            "Require native ARM validation",
        )["env"]["RADARD"] = "${{ needs.webrtc-runtime.result }}".into();
    }
);
rejected!(planner_rejects_native_caller_skip, |main, _native| {
    main["jobs"]["native-arm"]["if"] = "${{ false }}".into();
});
rejected!(planner_rejects_unknown_native_workflow, |main, _native| {
    main["jobs"]["native-arm"]["uses"] = "./.github/workflows/unknown.yml".into();
});
rejected!(
    planner_rejects_fast_missing_native_result,
    |main, _native| {
        main["jobs"]["fast"]["needs"] = json!(["planner-runtime", "planner-memory"]);
    }
);
rejected!(planner_rejects_native_planner_skip, |_main, native| {
    native["jobs"]["planner-runtime"]["if"] = "${{ false }}".into();
});
rejected!(planner_rejects_native_planner_x64_host, |_main, native| {
    native["jobs"]["planner-runtime"]["runs-on"] = "ubuntu-24.04".into();
});
rejected!(planner_rejects_empty_native_matrix, |_main, native| {
    native["jobs"]["planner-runtime"]["strategy"]["matrix"]["include"] = json!([]);
});
rejected!(planner_rejects_unknown_native_caller, |_main, native| {
    native["jobs"]["planner-runtime"]["env"]["CI296_NATIVE_ARM_CALLER"] = "unknown.yml".into();
});
rejected!(planner_rejects_unpinned_python, |_main, native| {
    step_mut(
        &mut native["jobs"]["planner-runtime"],
        "actions/setup-python@v6",
    )["with"]["python-version"] = "3.12".into();
});
rejected!(planner_rejects_dependency_stager_skip, |_main, native| {
    step_mut(
        &mut native["jobs"]["planner-runtime"],
        "Build current native dependency stager",
    )["if"] = "${{ false }}".into();
});
rejected!(
    planner_rejects_incomplete_native_aggregate,
    |_main, native| {
        native["jobs"]["native-arm-checks"]["needs"] =
            json!(["carrot-man-runtime", "webrtc-runtime"]);
    }
);
rejected!(
    planner_rejects_success_only_native_aggregate,
    |_main, native| {
        native["jobs"]["native-arm-checks"]["if"] = "${{ success() }}".into();
    }
);
rejected!(planner_rejects_optional_native_gate, |_main, native| {
    step_mut(
        &mut native["jobs"]["native-arm-checks"],
        "Require native ARM validation",
    )["continue-on-error"] = "true".into();
});
rejected!(planner_rejects_both_dependency_noops, |main, native| {
    for workflow in [main, native] {
        step_mut(
            &mut workflow["jobs"]["planner-runtime"],
            "Reserve capacity and install original planner dependencies",
        )["run"] = "true\n".into();
    }
});
rejected!(planner_rejects_both_comparison_noops, |main, native| {
    for workflow in [main, native] {
        step_mut(
            &mut workflow["jobs"]["planner-runtime"],
            "Compare exact policies, main loops, real IPC and lifecycle",
        )["run"] = "true\n".into();
    }
});
