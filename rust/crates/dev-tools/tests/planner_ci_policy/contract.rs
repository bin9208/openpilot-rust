//! maintained native Planner CI의 구조 계약. YAML scalar는 ci_policy::parse처럼 문자열이다.
use openpilot_dev_tools::ci_policy::{parse, validate_private_job_layout, TRUSTED_SOURCE};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, fs, path::PathBuf};

pub const FAMILIES: [(&str, &str, usize, &str); 8] = [
    (
        "planner-runtime",
        "90",
        6,
        "Reserve capacity and install original planner dependencies",
    ),
    (
        "carrot-man-runtime",
        "60",
        6,
        "Reserve runner capacity and install pinned dependencies",
    ),
    (
        "webrtc-runtime",
        "90",
        5,
        "Reserve capacity and install pinned source and native providers",
    ),
    (
        "webcam-runtime",
        "90",
        4,
        "Install exact source closure and native capture providers",
    ),
    (
        "xiaoge-runtime",
        "75",
        6,
        "Reserve capacity and install Xiaoge build prerequisites",
    ),
    (
        "radard-runtime",
        "90",
        10,
        "Reserve capacity and install original oracle dependencies",
    ),
    (
        "radar-arm",
        "90",
        8,
        "Reserve runner capacity and install generic ARM dependencies",
    ),
    (
        "carrot-navi-arm",
        "60",
        2,
        "Reserve runner capacity and install native ARM build dependencies",
    ),
];
pub const RESULTS: [(&str, &str); 8] = [
    ("PLANNER", "planner-runtime"),
    ("CARROT_MAN", "carrot-man-runtime"),
    ("WEBRTC", "webrtc-runtime"),
    ("WEBCAM", "webcam-runtime"),
    ("XIAOGE", "xiaoge-runtime"),
    ("RADARD", "radard-runtime"),
    ("RADAR_ARM", "radar-arm"),
    ("CARROT_NAVI_ARM", "carrot-navi-arm"),
];
const VERIFY: &str =
    "/usr/bin/bash \"$CI296_SOURCE/tools/ci/prepare-readonly-arm-rust-530\" verify\n";
const CARROT_MANIFEST: &str = "test \"$CI_PREPARED_RUST_ROOT\" = \"$OPENPILOT_CI_PREPARED_RUST_ROOT\"\ntest \"$CI_PREPARED_RUST_MANIFEST\" = \"$OPENPILOT_CI_PREPARED_RUST_MANIFEST\"\nsha256sum --check --strict --quiet -- \"$CI_PREPARED_RUST_MANIFEST\"\n";
const DENIAL: &str = "test \"${SOURCE_ALLOWED:-}\" = true || { echo 'Fork PR source denied on persistent bin-actions' >&2; exit 1; }\n";

pub fn load(name: &str) -> Value {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    parse(&fs::read_to_string(root.join(".github/workflows").join(name)).unwrap()).unwrap()
}
pub fn inputs() -> (Value, Value) {
    (load("rust.yml"), load("rust-native-arm.yml"))
}
pub fn steps(job: &Value) -> &[Value] {
    job["steps"].as_array().unwrap()
}
pub fn named_step<'a>(job: &'a Value, name: &str) -> &'a Value {
    let found = steps(job)
        .iter()
        .filter(|s| {
            s.get("name")
                .or_else(|| s.get("uses"))
                .and_then(Value::as_str)
                == Some(name)
        })
        .collect::<Vec<_>>();
    assert_eq!(found.len(), 1, "{name}");
    found[0]
}
pub fn step_mut<'a>(job: &'a mut Value, name: &str) -> &'a mut Value {
    let at = index(job, named_step(job, name));
    &mut job["steps"][at]
}
pub fn index(job: &Value, step: &Value) -> usize {
    steps(job).iter().position(|s| s == step).unwrap()
}
pub fn by_id<'a>(job: &'a Value, id: &str) -> &'a Value {
    steps(job).iter().find(|s| s["id"] == id).unwrap()
}
fn absent(node: &Value, keys: &[&str]) {
    for key in keys {
        assert!(node.get(*key).is_none(), "unexpected {key}");
    }
}
fn functional_steps(job: &Value) -> Vec<Value> {
    steps(job)
        .iter()
        .filter(|s| {
            s["shell"] == "openpilot-local-shell {0}"
                || s["uses"].as_str().is_some_and(|action| {
                    ["native-json11", "native-eigen", "native-image-sdk"]
                        .iter()
                        .any(|name| action.ends_with(&format!("/.github/actions/{name}")))
                })
        })
        .cloned()
        .collect()
}
fn row_mut<'a>(rows: &'a mut [Value], name: &str) -> &'a mut Value {
    rows.iter_mut().find(|s| s["name"] == name).unwrap()
}
fn replace_once(row: &mut Value, from: &str, to: &str) {
    let body = row["run"].as_str().unwrap();
    assert_eq!(body.matches(from).count(), 1, "{}", row["name"]);
    row["run"] = body.replace(from, to).into();
}

pub fn assert_contract(main: &Value, native: &Value) {
    // 기존 inline 명령이 위임된 helper의 검토된 원문까지 묶는다.
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let helper = fs::read(root.join("tools/ci/planner_native_controller_523.sh")).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(helper)),
        "32cfc4a9b57010c57bf3bdd112c4339ee22ea2ecd5618c39f6b1aa441de5ea63"
    );
    let jobs = &main["jobs"];
    let native_jobs = &native["jobs"];
    assert_eq!(main["env"]["RUSTUP_TOOLCHAIN"], "1.94.0");
    assert_eq!(native["env"]["RUSTUP_TOOLCHAIN"], "1.94.0");
    assert_eq!(
        jobs["native-arm"],
        json!({"uses":"./.github/workflows/rust-native-arm.yml"})
    );
    let fast = &jobs["fast"];
    for name in ["planner-runtime", "planner-memory", "native-arm"] {
        assert!(fast["needs"].as_array().unwrap().contains(&json!(name)));
    }
    assert_eq!(fast["if"], "${{ always() }}");
    absent(fast, &["continue-on-error"]);
    for (result, family) in [
        ("PLANNER", "planner-runtime"),
        ("PLANNER_MEMORY", "planner-memory"),
        ("NATIVE_ARM", "native-arm"),
    ] {
        assert_eq!(
            fast["steps"][0]["env"][result],
            format!("${{{{ needs.{family}.result }}}}")
        );
    }
    assert_eq!(
        native_jobs
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>(),
        FAMILIES
            .iter()
            .map(|(name, ..)| *name)
            .chain(["native-arm-checks"])
            .collect::<BTreeSet<_>>()
    );
    let candidate = load("python-rust-523-native-arm-final-two.yml");
    for (name, timeout, count, dependency_name) in FAMILIES {
        let job = &native_jobs[name];
        absent(job, &["if", "continue-on-error"]);
        assert_eq!(job["runs-on"], "ubuntu-24.04-arm");
        assert_eq!(job["timeout-minutes"], timeout);
        assert_eq!(
            job["strategy"],
            json!({"fail-fast":"false", "matrix":{"include":[{"arch":"aarch64", "runner":"hosted-arm64"}]}})
        );
        for (key, value) in [
            ("LOCAL_CI_ARCH", "${{ matrix.arch }}"),
            ("CI296_NATIVE_ARM", "1"),
            ("CI296_NATIVE_ARM_CALLER", "rust.yml"),
            (
                "CI296_PR_HEAD_REPOSITORY",
                "${{ github.event.pull_request.head.repo.full_name }}",
            ),
        ] {
            assert_eq!(job["env"][key], value, "{name}/{key}");
        }
        assert_eq!(
            job["env"]["CI296_ALIAS_BASENAME"],
            format!(".ci296-source-rust--{name}")
        );
        assert_eq!(job["defaults"]["run"]["shell"], "openpilot-local-shell {0}");
        let boundary = &steps(job)[0];
        absent(boundary, &["if"]);
        assert_eq!(boundary["env"]["SOURCE_ALLOWED"], TRUSTED_SOURCE);
        for marker in [
            "test \"$GITHUB_ACTIONS\" = true", "test \"$GITHUB_REPOSITORY\" = bin9208/openpilot-rust",
            "test \"$RUNNER_ENVIRONMENT\" = github-hosted", "test \"$RUNNER_OS\" = Linux",
            "test \"$RUNNER_ARCH\" = ARM64", "test \"$(uname -s)\" = Linux", "test \"$(uname -m)\" = aarch64",
            "test \"${CI296_NATIVE_ARM_CALLER:-}\" = rust.yml",
            "test \"${GITHUB_WORKFLOW_REF:-}\" = \"bin9208/openpilot-rust/.github/workflows/rust.yml@${GITHUB_REF:-}\"",
        ] { assert!(boundary["run"].as_str().unwrap().lines().any(|line| line == marker)); }
        for step in steps(job) {
            absent(step, &["continue-on-error"]);
        }
        let prepare = named_step(job, "Prepare native ARM command transport");
        let finish = named_step(job, "Finish native ARM execution evidence");
        assert_eq!(
            prepare["run"],
            "/usr/bin/bash \"$CI296_SOURCE/tools/ci/native-arm\" prepare\n"
        );
        assert_eq!(
            finish["run"],
            "/usr/bin/bash \"$CI296_SOURCE/tools/ci/native-arm\" finish\n"
        );
        assert_eq!(prepare["shell"], "bash");
        assert_eq!(finish["shell"], "bash");
        assert_eq!(prepare["if"], TRUSTED_SOURCE);
        assert_eq!(finish["if"], "${{ (github.event_name != 'pull_request' || github.event.pull_request.head.repo.full_name == github.repository) && (always() && steps.ci296_source_admission.outcome == 'success') }}");
        let python = named_step(job, "actions/setup-python@v6");
        assert_eq!(python["with"]["python-version"], "3.12.14");
        assert_eq!(python["if"], TRUSTED_SOURCE);
        let venv = named_step(job, "Keep original Python installations private");
        assert_eq!(venv["shell"], "bash");
        assert_eq!(venv["if"], TRUSTED_SOURCE);
        let reference = if matches!(name, "radar-arm" | "carrot-navi-arm") {
            &candidate["jobs"][name]
        } else {
            &jobs[name]
        };
        assert_eq!(
            venv["run"],
            named_step(reference, "Keep original Python installations private")["run"]
        );
        for marker in [
            "((available >= 512 * 1024 * 1024))",
            "python -m venv \"$CI_JOB_TEMP/venv\"",
            "printf '%s\\n' \"$CI_JOB_TEMP/venv/bin\" >> \"$GITHUB_PATH\"",
        ] {
            assert!(venv["run"]
                .as_str()
                .unwrap()
                .lines()
                .any(|line| line == marker));
        }
        let actual = functional_steps(job);
        let mut original = functional_steps(reference);
        if name == "radard-runtime" {
            for label in [
                "Reserve capacity and install original oracle dependencies",
                "Require Rust types, tests, complete build and Clippy",
            ] {
                replace_once(
                    row_mut(&mut original, label),
                    "${{ matrix.machine }}",
                    "${{ matrix.arch }}",
                );
            }
        }
        let reference_sdk = match name {
            "carrot-man-runtime" => CARROT_MANIFEST,
            "radard-runtime" => "rustup toolchain install 1.94.0 --profile minimal --component clippy --component rustfmt\n",
            "radar-arm" | "carrot-navi-arm" => "rustup toolchain install 1.94.0 --profile minimal\nrustup target add --toolchain 1.94.0 aarch64-unknown-linux-gnu\n",
            _ => "rustup toolchain install 1.94.0 --profile minimal --component clippy\n",
        };
        if name == "carrot-man-runtime" {
            // 정식 pin을 중복하지 않는다. 부모의 ERR trap 변경도 공유 policy가 검증한다.
            validate_private_job_layout(reference).unwrap();
            let before = named_step(
                reference,
                "Prepare exact fresh x64 SDK before CarrotMan cold bootstrap",
            );
            let after = named_step(
                reference,
                "Verify whole x64 SDK after every attempted CarrotMan check",
            );
            assert!(
                index(
                    reference,
                    named_step(reference, "Keep original Python installations private")
                ) < index(reference, before)
            );
            assert!(index(reference, before) < index(reference, &original[0]));
            assert!(index(reference, original.last().unwrap()) < index(reference, after));
            assert!(
                index(reference, after) < index(reference, by_id(reference, "ci296_source_seal"))
            );
            assert!(row_mut(&mut original, dependency_name)["run"]
                .as_str()
                .unwrap()
                .ends_with(CARROT_MANIFEST));
            for step in steps(reference).iter().chain(steps(job)) {
                let body = step["run"].as_str().unwrap_or("");
                assert!(
                    !body.contains("rustup toolchain install")
                        && !body.contains("rustup target add")
                );
            }
        }
        let dependency = row_mut(&mut original, dependency_name);
        if matches!(name, "radar-arm" | "carrot-navi-arm") {
            let body = dependency["run"].as_str().unwrap();
            assert_eq!(body.matches(VERIFY).count(), 1);
            assert!(
                !body.contains("rustup toolchain install") && !body.contains("rustup target add")
            );
        } else {
            replace_once(dependency, reference_sdk, VERIFY);
        }
        let before = named_step(
            job,
            "Prepare exact fresh ARM 1.94 SDK for all eight admitted roles",
        );
        let after = named_step(
            job,
            "Verify ARM SDK after every attempted maintained command",
        );
        assert_eq!(before["if"], TRUSTED_SOURCE);
        assert_eq!(after["if"], "${{ (github.event_name != 'pull_request' || github.event.pull_request.head.repo.full_name == github.repository) && (always() && steps.arm530_sdk.outcome != 'skipped' && env.CI296_SOURCE != '') }}");
        for (step, phase, id, timeout) in [
            (before, "before", "arm530_sdk", "20"),
            (after, "after", "arm530_sdk_final", "10"),
        ] {
            assert_eq!(step["id"], id);
            assert_eq!(step["timeout-minutes"], timeout);
            assert_eq!(step["shell"], "bash");
            assert_eq!(step["working-directory"], "${{ env.CI296_SOURCE }}");
            assert_eq!(
                step["env"],
                json!({"CI296_HOST_CONTROL":"1", "ARM530_HELPER_SHA256":"faa58b5a058a927824870ac2df8e9aeed3f33830d62c8bb6407c6e6540277b9b"})
            );
            assert_eq!(step["run"], format!("/usr/bin/bash \"$CI296_SOURCE/tools/ci/prepare-readonly-arm-rust-530\" {phase}\n"));
        }
        assert!(index(job, venv) < index(job, before));
        assert_eq!(index(job, before) + 1, index(job, &actual[0]));
        assert!(index(job, actual.last().unwrap()) < index(job, after));
        assert!(index(job, after) < index(job, finish));
        assert_eq!(actual, original, "{name}: all functional steps");
        assert_eq!(actual.len(), count, "{name}: functional step count");
        let order = [
            by_id(job, "ci296_source_admission"),
            prepare,
            python,
            venv,
            &actual[0],
        ]
        .map(|s| index(job, s));
        assert!(order.windows(2).all(|p| p[0] < p[1]));
    }
    let planner = &native_jobs["planner-runtime"];
    assert_eq!(planner["env"]["CARGO_BUILD_JOBS"], "2");
    assert_eq!(planner["env"]["OPENBLAS_NUM_THREADS"], "1");
    let stages = [
        "Reserve capacity and install original planner dependencies",
        "Build current native dependency stager",
        "Build pinned solvers and unchanged original bindings",
        "Compile and check the complete native planner",
        "Compare exact policies, main loops, real IPC and lifecycle",
    ];
    let positions = stages.map(|name| index(planner, named_step(planner, name)));
    assert!(positions.windows(2).all(|p| p[0] <= p[1]));
    for (name, expected) in [
        (stages[0], &[8192, 1024, 512, 1024][..]),
        (stages[2], &[1024, 1024][..]),
        (stages[3], &[1024, 4096, 1024, 1024][..]),
    ] {
        let budgets = named_step(planner, name)["run"].as_str().unwrap().lines()
            .filter(|line| line.starts_with("\"$GITHUB_WORKSPACE/rust/tools/check-disk-space\" --path \"$GITHUB_WORKSPACE\" --growth-mib "))
            .map(|line| line.rsplit_once(' ').unwrap().1.parse::<u64>().unwrap()).collect::<Vec<_>>();
        assert_eq!(budgets.as_slice(), expected);
    }
    assert_eq!(named_step(planner, stages[3])["run"].as_str().unwrap().lines().filter(|s| s.starts_with("python ") || s.starts_with("cargo ")).collect::<Vec<_>>(), [
        "cargo test --manifest-path rust/Cargo.toml -p openpilot-dev-tools --locked --target \"$NATIVE_TOOLS_HOST\" --test native_build_closure --test planner_ci_policy --test planner_ci_controller --test planner_controller_frozen --test planner_controller_retirement",
        "cargo test --manifest-path rust/Cargo.toml -p openpilot-plannerd --all-targets --locked",
        "cargo build --manifest-path rust/Cargo.toml -p openpilot-plannerd --bins --examples --locked",
        "cargo clippy --manifest-path rust/Cargo.toml -p openpilot-plannerd --all-targets --locked -- -D warnings",
    ]);
    assert_eq!(
        named_step(planner, stages[4])["run"],
        "/usr/bin/bash \"$CI296_SOURCE/tools/ci/planner_native_controller_523.sh\" runtime\n"
    );
    let sanitizer = named_step(
        planner,
        "Check generated C with ASan, UBSan and allocator leak detection",
    );
    assert_eq!(sanitizer["if"], "${{ (github.event_name != 'pull_request' || github.event.pull_request.head.repo.full_name == github.repository) && (matrix.runner == 'local-x64') }}");
    assert_eq!(
        sanitizer["run"],
        concat!(
            "python rust/tools/build_plannerd_acados.py --architecture x86_64 --cc gcc --sanitize ",
            "--python \"$(command -v python)\" --acados \"$PLANNER_GENERATOR\" --wheel \"$RUNNER_TEMP\"/planner-wheel/*.whl ",
            "--output \"$RUNNER_TEMP/planner-sanitized\"\n",
            "/usr/bin/bash \"$CI296_SOURCE/tools/ci/planner_native_controller_523.sh\" memory\n"
        )
    );
    assert!(index(planner, named_step(planner, stages[4])) < index(planner, sanitizer));
    let aggregate = &native_jobs["native-arm-checks"];
    assert_eq!(aggregate["needs"], json!(FAMILIES.map(|(name, ..)| name)));
    assert_eq!(aggregate["if"], "${{ always() }}");
    absent(aggregate, &["continue-on-error"]);
    assert_eq!(aggregate["runs-on"], "ubuntu-24.04");
    assert_eq!(aggregate["timeout-minutes"], "2");
    assert_eq!(steps(aggregate).len(), 1);
    let gate = &steps(aggregate)[0];
    absent(gate, &["if", "continue-on-error"]);
    let mut environment = json!({"SOURCE_ALLOWED":TRUSTED_SOURCE});
    let mut script = DENIAL.to_owned();
    for (variable, family) in RESULTS {
        environment[variable] = format!("${{{{ needs.{family}.result }}}}").into();
        script.push_str(&format!("test \"${variable}\" = success\n"));
    }
    assert_eq!(gate["env"], environment);
    assert_eq!(
        gate["run"].as_str().unwrap().lines().collect::<Vec<_>>(),
        script.lines().collect::<Vec<_>>()
    );
    for (name, label) in [
        (
            "radar-arm",
            "rust RadarCAN native ARM functional validation",
        ),
        ("carrot-navi-arm", "rust Carrot Navi native ARM build"),
    ] {
        assert_eq!(
            jobs[name],
            json!({"name":label, "needs":["native-arm"], "if":"${{ always() }}", "runs-on":"ubuntu-24.04", "timeout-minutes":"2",
            "steps":[{"name":"Require native ARM family result", "env":{"SOURCE_ALLOWED":TRUSTED_SOURCE, "NATIVE_ARM":"${{ needs.native-arm.result }}"}, "run":format!("{DENIAL}test \"$NATIVE_ARM\" = success\n")}]})
        );
    }
}
