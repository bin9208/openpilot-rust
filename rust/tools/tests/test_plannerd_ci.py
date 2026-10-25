from __future__ import annotations

import json
import ast
import hashlib
from copy import deepcopy
import os
from pathlib import Path
import subprocess

import pytest
import yaml

from check_plannerd_ci import Check, ROOT, run_checks


def ipc_target():
  # Execute the actual scheduling function without importing C++ IPC bindings.
  tree = ast.parse((ROOT / 'rust/tools/plannerd_ipc.py').read_text())
  function = next((node for node in tree.body if isinstance(node, ast.FunctionDef)
                   and node.name == 'next_model_target'), None)
  assert function is not None, 'IPC replay needs send-paced scheduling'
  namespace = {}
  exec(compile(ast.Module([function], type_ignores=[]), '<ipc scheduling>', 'exec'), namespace)
  return namespace['next_model_target']


def test_ipc_replay_keeps_normal_frequency_independent_of_solver_duration() -> None:
  target = ipc_target()
  for work in (0.003, 0.013, 0.025, 0.049):
    assert target(100.05, 100.0, 100.0 + work) == pytest.approx(100.05)


def test_ipc_replay_does_not_catch_up_after_delayed_model_processing() -> None:
  target = ipc_target()
  # Solver work spanning a complete period still gets a full recovery interval.
  assert target(100.05, 100.0, 100.08) == pytest.approx(100.13)
  # Late host wakeup cannot send two model inputs at the same nominal deadline.
  assert target(100.05, 100.04, 100.045) == pytest.approx(100.09)


def test_failed_comparison_is_recorded_and_stops_before_the_next_check(tmp_path: Path) -> None:
  failing = tmp_path / "failing.py"
  failing.write_text("raise SystemExit(42)\n")
  sentinel = tmp_path / "sentinel"
  later = tmp_path / "later.py"
  later.write_text(f"from pathlib import Path\nPath({str(sentinel)!r}).touch()\n")
  output = tmp_path / "result"
  with pytest.raises(subprocess.CalledProcessError) as failure:
    run_checks((Check("first", failing), Check("later", later)), output)
  assert failure.value.returncode == 42
  assert not sentinel.exists()
  rows = json.loads((output / "commands.json").read_text())
  assert [(row["name"], row["returncode"]) for row in rows] == [("first", 42)]


TRUSTED_SOURCE = "${{ github.event_name != 'pull_request' || github.event.pull_request.head.repo.full_name == github.repository }}"
NATIVE_ARM_FAMILIES = {'planner-runtime': 90, 'carrot-man-runtime': 60, 'webrtc-runtime': 90, 'webcam-runtime': 90, 'xiaoge-runtime': 75, 'radard-runtime': 90, 'radar-arm': 90, 'carrot-navi-arm': 60}
NATIVE_ARM_RESULTS = {'PLANNER': 'planner-runtime', 'CARROT_MAN': 'carrot-man-runtime', 'WEBRTC': 'webrtc-runtime', 'WEBCAM': 'webcam-runtime', 'XIAOGE': 'xiaoge-runtime', 'RADARD': 'radard-runtime', 'RADAR_ARM': 'radar-arm', 'CARROT_NAVI_ARM': 'carrot-navi-arm'}


def named_step(job: dict, name: str) -> dict:
  matches = [step for step in job["steps"] if step.get("name", step.get("uses")) == name]
  assert len(matches) == 1, name
  return matches[0]


def functional_steps(job: dict) -> list[dict]:
  return [step for step in job["steps"] if step.get("shell") == "openpilot-local-shell {0}"
          or step.get("uses", "").endswith(tuple("/.github/actions/" + name for name in ("native-json11", "native-eigen", "native-image-sdk")))]


def assert_planner_native_contract(main: dict, native: dict) -> None:
  jobs, native_jobs = main["jobs"], native["jobs"]
  assert main["env"]["RUSTUP_TOOLCHAIN"] == native["env"]["RUSTUP_TOOLCHAIN"] == "1.94.0"
  assert jobs["native-arm"] == {"uses": "./.github/workflows/rust-native-arm.yml"}
  assert {"planner-runtime", "planner-memory", "native-arm"} <= set(jobs["fast"]["needs"])
  assert jobs["fast"]["if"] == "${{ always() }}" and "continue-on-error" not in jobs["fast"]
  for result, family in (("PLANNER", "planner-runtime"), ("PLANNER_MEMORY", "planner-memory"), ("NATIVE_ARM", "native-arm")):
    assert jobs["fast"]["steps"][0]["env"][result] == "${{ needs." + family + ".result }}"
  assert set(native_jobs) == {*NATIVE_ARM_FAMILIES, "native-arm-checks"}
  for name, timeout in NATIVE_ARM_FAMILIES.items():
    job = native_jobs[name]
    assert "if" not in job and "continue-on-error" not in job
    assert job["runs-on"] == "ubuntu-24.04-arm"
    assert job["timeout-minutes"] == timeout
    assert job["strategy"] == {"fail-fast": False, "matrix": {"include": [{"arch": "aarch64", "runner": "hosted-arm64"}]}}
    assert job["env"]["LOCAL_CI_ARCH"] == "${{ matrix.arch }}"
    assert job["env"]["CI296_NATIVE_ARM"] == "1"
    assert job["env"]["CI296_NATIVE_ARM_CALLER"] == "rust.yml"
    assert job["env"]["CI296_PR_HEAD_REPOSITORY"] == "${{ github.event.pull_request.head.repo.full_name }}"
    assert job["env"]["CI296_ALIAS_BASENAME"] == f".ci296-source-rust--{name}"
    assert job["defaults"]["run"]["shell"] == "openpilot-local-shell {0}"
    boundary = job["steps"][0]
    assert "if" not in boundary and boundary["env"]["SOURCE_ALLOWED"] == TRUSTED_SOURCE
    for marker in ('test "$GITHUB_ACTIONS" = true', 'test "$GITHUB_REPOSITORY" = bin9208/openpilot-rust',
                   'test "$RUNNER_ENVIRONMENT" = github-hosted', 'test "$RUNNER_OS" = Linux',
                   'test "$RUNNER_ARCH" = ARM64', 'test "$(uname -s)" = Linux', 'test "$(uname -m)" = aarch64',
                   'test "${CI296_NATIVE_ARM_CALLER:-}" = rust.yml',
                   'test "${GITHUB_WORKFLOW_REF:-}" = "bin9208/openpilot-rust/.github/workflows/rust.yml@${GITHUB_REF:-}"'):
      assert marker in boundary["run"].splitlines()
    steps = job["steps"]
    assert all("continue-on-error" not in step for step in steps)
    prepare = named_step(job, "Prepare native ARM command transport")
    finish = named_step(job, "Finish native ARM execution evidence")
    assert prepare["run"] == '/usr/bin/bash "$CI296_SOURCE/tools/ci/native-arm" prepare\n'
    assert prepare["shell"] == finish["shell"] == "bash"
    assert prepare["if"] == TRUSTED_SOURCE
    assert finish["run"] == '/usr/bin/bash "$CI296_SOURCE/tools/ci/native-arm" finish\n'
    assert finish["if"] == ("${{ (github.event_name != 'pull_request' || github.event.pull_request.head.repo.full_name == github.repository) "
                            + "&& (always() && steps.ci296_source_admission.outcome == 'success') }}")
    python = named_step(job, "actions/setup-python@v6")
    assert python["with"]["python-version"] == "3.12.14" and python["if"] == TRUSTED_SOURCE
    venv = named_step(job, "Keep original Python installations private")
    assert venv["shell"] == "bash" and venv["if"] == TRUSTED_SOURCE
    if name in ("radar-arm", "carrot-navi-arm"):
      reference = yaml.safe_load((ROOT / ".github/workflows/python-rust-523-native-arm-final-two.yml").read_text())["jobs"][name]
    else:
      reference = jobs[name]
    assert venv["run"] == named_step(reference, "Keep original Python installations private")["run"]
    for line in ('((available >= 512 * 1024 * 1024))', 'python -m venv "$CI_JOB_TEMP/venv"',
                 'printf \'%s\\n\' "$CI_JOB_TEMP/venv/bin" >> "$GITHUB_PATH"'):
      assert line in venv["run"].splitlines()
    admission = next(step for step in steps if step.get("id") == "ci296_source_admission")
    # 기존 기능 step의 순서·조건·명령 전체를 x64 원문과 비교한다. ARM의 기존 ASan skip도 그대로다.
    actual, original = functional_steps(job), deepcopy(functional_steps(reference))
    if name == "radard-runtime":
      for label in ("Reserve capacity and install original oracle dependencies", "Require Rust types, tests, complete build and Clippy"):
        row = next(step for step in original if step["name"] == label)
        assert row["run"].count("${{ matrix.machine }}") == 1
        row["run"] = row["run"].replace("${{ matrix.machine }}", "${{ matrix.arch }}")
    if name in NATIVE_ARM_FAMILIES:
      # 각 role의 기존 SDK 검증/설치 명령만 바꾸고 모든 기능 명령의 동등 비교를 유지한다.
      clippy = 'rustup toolchain install 1.94.0 --profile minimal --component clippy\n'
      minimal = 'rustup toolchain install 1.94.0 --profile minimal\n'
      target = 'rustup target add --toolchain 1.94.0 aarch64-unknown-linux-gnu\n'
      reference_sdk = {
          "planner-runtime": clippy,
          "carrot-man-runtime": ('test "$CI_PREPARED_RUST_ROOT" = "$OPENPILOT_CI_PREPARED_RUST_ROOT"\n'
                                 'test "$CI_PREPARED_RUST_MANIFEST" = "$OPENPILOT_CI_PREPARED_RUST_MANIFEST"\n'
                                 'sha256sum --check --strict --quiet -- "$CI_PREPARED_RUST_MANIFEST"\n'),
          "webrtc-runtime": clippy,
          "webcam-runtime": clippy,
          "xiaoge-runtime": clippy,
          "radard-runtime": 'rustup toolchain install 1.94.0 --profile minimal --component clippy --component rustfmt\n',
          "radar-arm": minimal + target,
          "carrot-navi-arm": minimal + target,
      }[name]
      dependency_name = {
          "planner-runtime": "Reserve capacity and install original planner dependencies",
          "carrot-man-runtime": "Reserve runner capacity and install pinned dependencies",
          "webrtc-runtime": "Reserve capacity and install pinned source and native providers",
          "webcam-runtime": "Install exact source closure and native capture providers",
          "xiaoge-runtime": "Reserve capacity and install Xiaoge build prerequisites",
          "radard-runtime": "Reserve capacity and install original oracle dependencies",
          "radar-arm": "Reserve runner capacity and install generic ARM dependencies",
          "carrot-navi-arm": "Reserve runner capacity and install native ARM build dependencies",
      }[name]
      dependency = next(step for step in original if step["name"] == dependency_name)
      verify = '/usr/bin/bash "$CI296_SOURCE/tools/ci/prepare-readonly-arm-rust-530" verify\n'
      if name == "carrot-man-runtime":
        # x64도 이미 sealed SDK다. ci_policy::validate_carrot_x64_sdk530_steps의
        # 정식 before/after 본문 pin으로 fresh 경로·manifest·자동 설치 금지를 확인한다.
        x64_before = named_step(reference, "Prepare exact fresh x64 SDK before CarrotMan cold bootstrap")
        x64_after = named_step(reference, "Verify whole x64 SDK after every attempted CarrotMan check")
        for step, identifier, condition, digest in (
            (x64_before, "carrot530_sdk", TRUSTED_SOURCE,
             "851b90c7b9c20c34a75827b7f76eb6539bc115726d26932b66ccbe60195eac55"),
            (x64_after, "carrot530_sdk_final",
             "${{ (github.event_name != 'pull_request' || github.event.pull_request.head.repo.full_name == github.repository) "
             + "&& (always() && steps.carrot530_sdk.outcome != 'skipped' && env.CI296_SOURCE != '') }}",
             "4d7305f25cbb9f9de394305c1aa4514c222aa2d6f3d02b623d0ad201bac161c7")):
          assert step["id"] == identifier and step["if"] == condition
          assert step["shell"] == "bash" and step["working-directory"] == "${{ env.CI296_SOURCE }}"
          assert step["env"] == {"CI296_HOST_CONTROL": "1"}
          assert "continue-on-error" not in step and "uses" not in step
          assert hashlib.sha256(step["run"].encode()).hexdigest() == digest
        reference_steps = reference["steps"]
        x64_venv = named_step(reference, "Keep original Python installations private")
        seal = next(step for step in reference_steps if step.get("id") == "ci296_source_seal")
        assert reference_steps.index(x64_venv) < reference_steps.index(x64_before)
        assert reference_steps.index(x64_before) < reference_steps.index(original[0])
        assert reference_steps.index(original[-1]) < reference_steps.index(x64_after) < reference_steps.index(seal)
        assert dependency["run"].endswith(reference_sdk)
        for step in (*reference_steps, *steps):
          assert 'rustup toolchain install' not in step.get("run", "")
          assert 'rustup target add' not in step.get("run", "")
      if name in ("radar-arm", "carrot-navi-arm"):
        # #572 candidate도 이미 canonical SDK를 사용하므로 중복 투영하지 않는다.
        assert dependency["run"].count(verify) == 1
        assert 'rustup toolchain install' not in dependency["run"]
        assert 'rustup target add' not in dependency["run"]
      else:
        assert dependency["run"].count(reference_sdk) == 1
        dependency["run"] = dependency["run"].replace(reference_sdk, verify)
      before = named_step(job, "Prepare exact fresh ARM 1.94 SDK for all eight admitted roles")
      after = named_step(job, "Verify ARM SDK after every attempted maintained command")
      assert before["if"] == TRUSTED_SOURCE
      assert after["if"] == ("${{ (github.event_name != 'pull_request' || github.event.pull_request.head.repo.full_name == github.repository) "
                             + "&& (always() && steps.arm530_sdk.outcome != 'skipped' && env.CI296_SOURCE != '') }}")
      for step, phase, identifier, timeout in ((before, "before", "arm530_sdk", 20), (after, "after", "arm530_sdk_final", 10)):
        assert step["id"] == identifier and step["timeout-minutes"] == timeout
        assert step["shell"] == "bash" and step["working-directory"] == "${{ env.CI296_SOURCE }}"
        assert step["env"] == {"CI296_HOST_CONTROL": "1", "ARM530_HELPER_SHA256":
                               "faa58b5a058a927824870ac2df8e9aeed3f33830d62c8bb6407c6e6540277b9b"}
        assert step["run"] == '/usr/bin/bash "$CI296_SOURCE/tools/ci/prepare-readonly-arm-rust-530" ' + phase + '\n'
      assert steps.index(venv) < steps.index(before) == steps.index(actual[0]) - 1
      assert steps.index(actual[-1]) < steps.index(after) < steps.index(finish)
    assert actual == original and len(actual) == {"planner-runtime": 6, "carrot-man-runtime": 6, "webrtc-runtime": 5,
        "webcam-runtime": 4, "xiaoge-runtime": 6, "radard-runtime": 10, "radar-arm": 8, "carrot-navi-arm": 2}[name]
    assert steps.index(admission) < steps.index(prepare) < steps.index(python) < steps.index(venv) < steps.index(actual[0])
  planner = native_jobs["planner-runtime"]
  assert planner["env"]["CARGO_BUILD_JOBS"] == "2" and planner["env"]["OPENBLAS_NUM_THREADS"] == "1"
  stages = ["Reserve capacity and install original planner dependencies", "Build current native dependency stager",
            "Build pinned solvers and unchanged original bindings", "Compile and check the complete native planner",
            "Compare exact policies, main loops, real IPC and lifecycle"]
  positions = [planner["steps"].index(named_step(planner, name)) for name in stages]
  assert positions == sorted(positions)
  for name, budgets in ((stages[0], [8192, 1024, 512, 1024]), (stages[2], [1024, 1024]),
                        (stages[3], [1024, 4096, 1024, 1024])):
    lines = named_step(planner, name)["run"].splitlines()
    assert [int(line.rsplit(" ", 1)[1]) for line in lines
            if line.startswith('"$GITHUB_WORKSPACE/rust/tools/check-disk-space" --path "$GITHUB_WORKSPACE" --growth-mib ')] == budgets
  assert [line for line in named_step(planner, stages[3])["run"].splitlines()
          if line.startswith(("python ", "cargo "))] == [
      "python -m pytest -c /dev/null --rootdir . --confcutdir=rust/tools/tests -q rust/tools/tests/test_plannerd_ci.py",
      'cargo test --manifest-path rust/Cargo.toml -p openpilot-dev-tools --locked --target "$NATIVE_TOOLS_HOST" --test native_build_closure',
      "cargo test --manifest-path rust/Cargo.toml -p openpilot-plannerd --all-targets --locked",
      "cargo build --manifest-path rust/Cargo.toml -p openpilot-plannerd --bins --examples --locked",
      "cargo clippy --manifest-path rust/Cargo.toml -p openpilot-plannerd --all-targets --locked -- -D warnings"]
  assert named_step(planner, stages[4])["run"] == ('python -P rust/tools/check_plannerd_ci.py --bin-dir rust/target/debug '
      + '--artifact "$RUNNER_TEMP/planner-artifact" --source-native "$RUNNER_TEMP/planner-source" '
      + '--binding "$PLANNER_PARAMS_BINDING" --output "$RUNNER_TEMP/planner-comparison"\n')
  aggregate = native_jobs["native-arm-checks"]
  assert aggregate["needs"] == list(NATIVE_ARM_FAMILIES)
  assert aggregate["if"] == "${{ always() }}" and "continue-on-error" not in aggregate
  assert aggregate["runs-on"] == "ubuntu-24.04" and aggregate["timeout-minutes"] == 2
  assert len(aggregate["steps"]) == 1
  gate = aggregate["steps"][0]
  assert "if" not in gate and "continue-on-error" not in gate
  assert gate["env"] == {"SOURCE_ALLOWED": TRUSTED_SOURCE, **{
      variable: "${{ needs." + family + ".result }}" for variable, family in NATIVE_ARM_RESULTS.items()}}
  assert gate["run"].splitlines() == [
      'test "${SOURCE_ALLOWED:-}" = true || { echo \'Fork PR source denied on persistent bin-actions\' >&2; exit 1; }',
      *[f'test "${variable}" = success' for variable in NATIVE_ARM_RESULTS]]
  for name in ("radar-arm", "carrot-navi-arm"):
    assert jobs[name] == {"name": {"radar-arm": "rust RadarCAN native ARM functional validation",
        "carrot-navi-arm": "rust Carrot Navi native ARM build"}[name], "needs": ["native-arm"],
        "if": "${{ always() }}", "runs-on": "ubuntu-24.04", "timeout-minutes": 2,
        "steps": [{"name": "Require native ARM family result", "env": {"SOURCE_ALLOWED": TRUSTED_SOURCE,
            "NATIVE_ARM": "${{ needs.native-arm.result }}"}, "run": gate["run"].splitlines(keepends=True)[0] + 'test "$NATIVE_ARM" = success\n'}]}



def test_planner_host_arm_and_memory_results_are_required() -> None:
  main = yaml.safe_load((ROOT / ".github/workflows/rust.yml").read_text())
  native = yaml.safe_load((ROOT / ".github/workflows/rust-native-arm.yml").read_text())
  assert_planner_native_contract(main, native)
  jobs = main["jobs"]
  runtime = jobs["planner-runtime"]
  assert runtime["strategy"] == {"fail-fast": False, "matrix": {"runner": ["local-x64"]}}
  assert runtime["env"]["LOCAL_CI_ARCH"] == "${{ matrix.runner == 'local-arm64-qemu' && 'aarch64' || 'x86_64' }}"
  trusted = "${{ github.event_name != 'pull_request' || github.event.pull_request.head.repo.full_name == github.repository }}"
  for name in ("planner-runtime", "planner-memory"):
    assert jobs[name]["runs-on"] == "ubuntu-24.04"
    boundary = jobs[name]["steps"][0]
    assert "if" not in boundary and boundary["env"]["SOURCE_ALLOWED"] == trusted
    for marker in ('test "$GITHUB_ACTIONS" = true', 'test "$GITHUB_REPOSITORY" = bin9208/openpilot-rust',
                   'test "$RUNNER_ENVIRONMENT" = github-hosted', 'test "$RUNNER_OS" = Linux',
                   'test "$RUNNER_ARCH" = X64', 'test "$(uname -m)" = x86_64'):
      assert marker in boundary["run"].splitlines()
  alias = ".ci296-source-rust--planner-runtime"
  assert runtime["env"]["CI296_ALIAS_BASENAME"] == alias
  transport = [index for index, step in enumerate(runtime["steps"])
               if step.get("uses") == f"./{alias}/source/.github/actions/local-arm"
               and step.get("with", {}).get("cleanup") != "true"]
  assert len(transport) == 1
  admission = next(index for index, step in enumerate(runtime["steps"]) if step.get("id") == "ci296_source_admission")
  preflight = next(index for index, step in enumerate(runtime["steps"])
                   if 'test -x /usr/bin/qemu-aarch64-static' in step.get("run", ""))
  assert admission < preflight < transport[0]
  assert runtime["steps"][transport[0]]["if"] == trusted
  assert any(step.get("shell") == "openpilot-local-shell {0}" for step in runtime["steps"][transport[0] + 1:])
  assert {"planner-runtime", "planner-memory", "native-arm"} <= set(jobs["fast"]["needs"])
  gate = jobs["fast"]["steps"][0]
  assert gate["env"]["SOURCE_ALLOWED"] == trusted
  environment = dict(os.environ, **dict.fromkeys(gate["env"], "success"))
  environment["SOURCE_ALLOWED"] = "true"
  assert subprocess.run(["bash", "-e", "-c", gate["run"]], env=environment, check=False).returncode == 0
  for name in ("PLANNER", "PLANNER_MEMORY", "NATIVE_ARM", "SOURCE_ALLOWED"):
    for state in ("failure", "cancelled", "skipped", "", "false", "unknown"):
      result = subprocess.run(["bash", "-e", "-c", gate["run"]], env=dict(environment, **{name: state}), check=False)
      assert result.returncode != 0, (name, state)
  native_gate = native["jobs"]["native-arm-checks"]["steps"][0]
  environment = dict(os.environ, **dict.fromkeys(native_gate["env"], "success"))
  environment["SOURCE_ALLOWED"] = "true"
  assert subprocess.run(["bash", "-e", "-c", native_gate["run"]], env=environment, check=False).returncode == 0
  for name in (*NATIVE_ARM_RESULTS, "SOURCE_ALLOWED"):
    for state in ("failure", "cancelled", "skipped", "", "false", "unknown"):
      result = subprocess.run(["bash", "-e", "-c", native_gate["run"]], env=dict(environment, **{name: state}), check=False)
      assert result.returncode != 0, (name, state)
  for job in [jobs["planner-runtime"], jobs["planner-memory"], *native["jobs"].values()]:
    for step in job["steps"]:
      if "run" in step:
        result = subprocess.run(["bash", "-n"], input=step["run"], text=True, capture_output=True, check=False)
        assert result.returncode == 0, result.stderr


@pytest.mark.parametrize(("scope", "path", "value"), [
    ("main", ("jobs", "radar-arm", "steps", "Require native ARM family result", "run"), "true\n"),
    ("main", ("jobs", "carrot-navi-arm", "needs"), []),
    ("native", ("jobs", "xiaoge-runtime", "timeout-minutes"), 70),
    ("native", ("jobs", "radar-arm", "steps", "Require actual original and native ARM IPC and signal lifecycle", "if"), "${{ false }}"),
    ("native", ("jobs", "native-arm-checks", "steps", "Require native ARM validation", "env", "RADARD"), "${{ needs.webrtc-runtime.result }}"),
    ("main", ("jobs", "native-arm", "if"), "${{ false }}"),
    ("main", ("jobs", "native-arm", "uses"), "./.github/workflows/unknown.yml"),
    ("main", ("jobs", "fast", "needs"), ["planner-runtime", "planner-memory"]),
    ("native", ("jobs", "planner-runtime", "if"), "${{ false }}"),
    ("native", ("jobs", "planner-runtime", "runs-on"), "ubuntu-24.04"),
    ("native", ("jobs", "planner-runtime", "strategy", "matrix", "include"), []),
    ("native", ("jobs", "planner-runtime", "env", "CI296_NATIVE_ARM_CALLER"), "unknown.yml"),
    ("native", ("jobs", "planner-runtime", "steps", "actions/setup-python@v6", "with", "python-version"), "3.12"),
    ("native", ("jobs", "planner-runtime", "steps", "Build current native dependency stager", "if"), "${{ false }}"),
    ("native", ("jobs", "native-arm-checks", "needs"), ["carrot-man-runtime", "webrtc-runtime"]),
    ("native", ("jobs", "native-arm-checks", "if"), "${{ success() }}"),
    ("native", ("jobs", "native-arm-checks", "steps", "Require native ARM validation", "continue-on-error"), True),
    ("both", ("jobs", "planner-runtime", "steps", "Reserve capacity and install original planner dependencies", "run"), "true\n"),
    ("both", ("jobs", "planner-runtime", "steps", "Compare exact policies, main loops, real IPC and lifecycle", "run"), "true\n"),
])
def test_planner_native_arm_contract_rejects_missing_skipped_or_weakened_gates(scope, path, value) -> None:
  main = yaml.safe_load((ROOT / ".github/workflows/rust.yml").read_text())
  native = yaml.safe_load((ROOT / ".github/workflows/rust-native-arm.yml").read_text())
  for workflow in ([main, native] if scope == "both" else [main if scope == "main" else native]):
    node = workflow
    for key in path[:-1]:
      node = next(step for step in node if step.get("name", step.get("uses")) == key) if isinstance(node, list) else node[key]
    node[path[-1]] = deepcopy(value)
  with pytest.raises(AssertionError):
    assert_planner_native_contract(main, native)


def test_planner_miri_failure_cannot_be_hidden_by_log_capture(tmp_path: Path) -> None:
  jobs = yaml.safe_load((ROOT / ".github/workflows/rust.yml").read_text())["jobs"]
  step = next(step for step in jobs["planner-memory"]["steps"] if "MIRIFLAGS=" in step.get("run", ""))
  commands = tmp_path / "bin"
  commands.mkdir()
  for name, body in {
    "python": "#!/bin/sh\nexit 0\n",
    "cargo": '#!/bin/sh\nprintf "failed\\n" >> "$MIRI_FAILURE_CALLS"\nexit 42\n',
  }.items():
    path = commands / name
    path.write_text(body)
    path.chmod(0o755)
  calls = tmp_path / "calls"
  environment = dict(os.environ, PATH=f"{commands}:{os.environ['PATH']}", RUNNER_TEMP=str(tmp_path),
                     GITHUB_WORKSPACE=str(ROOT), MIRI_FAILURE_CALLS=str(calls))
  result = subprocess.run(["bash", "-e", "-c", step["run"]], env=environment, capture_output=True, text=True, check=False)
  assert result.returncode == 42, (result.returncode, result.stdout, result.stderr)
  assert calls.read_text().splitlines() == ["failed"]
