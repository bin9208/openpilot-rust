from __future__ import annotations

import json
import os
from pathlib import Path
from copy import deepcopy
import subprocess

import pytest
import yaml

from check_xiaoge_native_memory import ROOT, artifacts


def receipts(tmp_path: Path) -> tuple[list[dict], list[dict]]:
  tests, examples = [], []
  for package, names, example in [("openpilot-opencv-runtime", ["contracts", "native"], "opencv_trace"),
                                   ("openpilot-jpeg", ["ownership", "options"], "jpeg_options")]:
    for name in [*names, example]:
      path = tmp_path / name
      path.write_bytes(b"fixture")
      is_test = name != example
      row = {"reason": "compiler-artifact", "package_id": f"path+fixture#{package}@0.1.0", "executable": str(path),
        "target": {"name": name, "kind": ["test" if is_test else "example"]}, "profile": {"test": is_test}}
      (tests if is_test else examples).append(row)
  for batch in [tests, examples]:
    batch.append({"reason": "build-finished", "success": True})
  return tests, examples


def encoded(rows: list[dict]) -> str:
  return "\n".join(json.dumps(row) for row in rows)


def test_only_successful_current_native_suites_and_real_examples_are_accepted(tmp_path: Path) -> None:
  tests, examples = receipts(tmp_path)
  selected, binaries = artifacts(encoded(tests), encoded(examples))
  assert len(selected) == 4 and set(binaries) == {"opencv_trace", "jpeg_options"}
  for index in range(4):
    with pytest.raises(ValueError):
      artifacts(encoded(tests[:index] + tests[index + 1:]), encoded(examples))
  examples[0]["profile"]["test"] = True
  with pytest.raises(ValueError):
    artifacts(encoded(tests), encoded(examples))


@pytest.mark.parametrize("batch", [0, 1])
def test_failed_or_missing_build_receipts_are_rejected(tmp_path: Path, batch: int) -> None:
  rows = list(receipts(tmp_path))
  rows[batch][-1]["success"] = False
  with pytest.raises(ValueError):
    artifacts(*(encoded(value) for value in rows))
  rows[batch].pop()
  with pytest.raises(ValueError):
    artifacts(*(encoded(value) for value in rows))


def assert_xiaoge_native_contract(main: dict, native: dict) -> None:
  expected = ["planner-runtime", "carrot-man-runtime", "webrtc-runtime", "webcam-runtime", "xiaoge-runtime",
              "radard-runtime", "radar-arm", "carrot-navi-arm"]
  assert main["jobs"]["native-arm"] == {"uses": "./.github/workflows/rust-native-arm.yml"}
  assert set(native["jobs"]) == {*expected, "native-arm-checks"}
  job = native["jobs"]["xiaoge-runtime"]
  assert job["runs-on"] == "ubuntu-24.04-arm" and job["timeout-minutes"] == 75
  assert "if" not in job and "continue-on-error" not in job
  assert job["strategy"] == {"fail-fast": False, "matrix": {"include": [{"arch": "aarch64", "runner": "hosted-arm64"}]}}
  assert job["env"]["LOCAL_CI_ARCH"] == "${{ matrix.arch }}" and job["env"]["CI296_NATIVE_ARM"] == "1"
  assert job["env"]["CI296_NATIVE_ARM_CALLER"] == "rust.yml"
  assert job["env"]["CI296_PR_HEAD_REPOSITORY"] == "${{ github.event.pull_request.head.repo.full_name }}"
  trusted = "${{ github.event_name != 'pull_request' || github.event.pull_request.head.repo.full_name == github.repository }}"
  boundary = job["steps"][0]
  assert "if" not in boundary and boundary["env"]["SOURCE_ALLOWED"] == trusted
  for marker in ('test "$RUNNER_ARCH" = ARM64', 'test "$(uname -m)" = aarch64',
                 'test "${CI296_NATIVE_ARM_CALLER:-}" = rust.yml',
                 'test "${GITHUB_WORKFLOW_REF:-}" = "bin9208/openpilot-rust/.github/workflows/rust.yml@${GITHUB_REF:-}"'):
    assert marker in boundary["run"].splitlines()
  selected = {}
  for name in ("Admit pinned LFS models before source baseline", "Admit exact private source and index",
               "Prepare native ARM command transport", "actions/setup-python@v6", "Keep original Python installations private",
               "Compare source pixels, model results, sockets, live IPC and lifecycle", "Finish native ARM execution evidence"):
    rows = [i for i, step in enumerate(job["steps"]) if step.get("name", step.get("uses")) == name]
    assert len(rows) == 1
    selected[name] = rows[0]
  assert list(selected.values()) == sorted(selected.values())
  lfs = job["steps"][selected["Admit pinned LFS models before source baseline"]]
  assert lfs["if"] == trusted and lfs["run"] == next(s for s in main["jobs"]["xiaoge-runtime"]["steps"] if s.get("name") == lfs["name"])["run"]
  python = job["steps"][selected["actions/setup-python@v6"]]
  assert python["with"]["python-version"] == "3.12.14" and python["if"] == trusted
  for mode, label in [("prepare", "Prepare native ARM command transport"), ("finish", "Finish native ARM execution evidence")]:
    step = job["steps"][selected[label]]
    assert step["run"] == '/usr/bin/bash "$CI296_SOURCE/tools/ci/native-arm" ' + mode + '\n'
  assert native["jobs"]["native-arm-checks"]["needs"] == expected
  gate = native["jobs"]["native-arm-checks"]
  assert gate["if"] == "${{ always() }}" and "continue-on-error" not in gate
  variables = ["PLANNER", "CARROT_MAN", "WEBRTC", "WEBCAM", "XIAOGE", "RADARD", "RADAR_ARM", "CARROT_NAVI_ARM"]
  for variable, family in zip(variables, expected, strict=True):
    assert gate["steps"][0]["env"][variable] == "${{ needs." + family + ".result }}"
    assert gate["steps"][0]["run"].splitlines().count(f'test "${variable}" = success') == 1


def test_both_host_architectures_and_native_memory_are_required_gates() -> None:
  jobs = yaml.safe_load((ROOT / ".github/workflows/rust.yml").read_text())["jobs"]
  runtime = jobs["xiaoge-runtime"]
  assert runtime["strategy"]["matrix"]["runner"] == ["local-x64"]
  native = yaml.safe_load((ROOT / ".github/workflows/rust-native-arm.yml").read_text())
  assert_xiaoge_native_contract({"jobs": jobs}, native)
  assert runtime["env"]["LOCAL_CI_ARCH"] == "${{ matrix.runner == 'local-arm64-qemu' && 'aarch64' || 'x86_64' }}"
  trusted = "${{ github.event_name != 'pull_request' || github.event.pull_request.head.repo.full_name == github.repository }}"
  for name in ("xiaoge-runtime", "xiaoge-memory"):
    assert jobs[name]["runs-on"] == "ubuntu-24.04"
    boundary = jobs[name]["steps"][0]
    assert "if" not in boundary and boundary["env"]["SOURCE_ALLOWED"] == trusted
    for marker in ('test "$GITHUB_ACTIONS" = true', 'test "$GITHUB_REPOSITORY" = bin9208/openpilot-rust',
                   'test "$RUNNER_ENVIRONMENT" = github-hosted', 'test "$RUNNER_OS" = Linux',
                   'test "$RUNNER_ARCH" = X64', 'test "$(uname -m)" = x86_64'):
      assert marker in boundary["run"].splitlines()
  alias = ".ci296-source-rust--xiaoge-runtime"
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
  assert {"xiaoge-runtime", "xiaoge-memory"} <= set(jobs["fast"]["needs"])
  gate = jobs["fast"]["steps"][0]
  assert gate["env"]["SOURCE_ALLOWED"] == trusted
  environment = dict(os.environ, **dict.fromkeys(gate["env"], "success"))
  environment["SOURCE_ALLOWED"] = "true"
  assert subprocess.run(["bash", "-e", "-c", gate["run"]], env=environment, check=False).returncode == 0
  for name in ["XIAOGE", "XIAOGE_MEMORY", "SOURCE_ALLOWED"]:
    for value in ["failure", "cancelled", "skipped", "", "false"]:
      run = subprocess.run(["bash", "-e", "-c", gate["run"]], env=dict(environment, **{name: value}), check=False)
      assert run.returncode != 0, (name, value)
  for job in ["xiaoge-runtime", "xiaoge-memory"]:
    for step in jobs[job]["steps"]:
      if "run" in step:
        result = subprocess.run(["bash", "-n"], input=step["run"], text=True, capture_output=True, check=False)
        assert result.returncode == 0, result.stderr


def test_miri_failure_stops_the_required_job_before_later_modes(tmp_path: Path) -> None:
  jobs = yaml.safe_load((ROOT / ".github/workflows/rust.yml").read_text())["jobs"]
  step = next(step for step in jobs["xiaoge-memory"]["steps"] if "MIRIFLAGS=" in step.get("run", ""))
  binaries = tmp_path / "bin"
  binaries.mkdir()
  for name, script in {
    "python": "#!/bin/sh\nexit 0\n",
    "cargo": '#!/bin/sh\nprintf "failed\\n" >> "$MIRI_FAILURE_CALLS"\nexit 42\n',
  }.items():
    command = binaries / name
    command.write_text(script)
    command.chmod(0o755)
  calls = tmp_path / "calls"
  environment = dict(os.environ, PATH=f"{binaries}:{os.environ['PATH']}", RUNNER_TEMP=str(tmp_path),
                     GITHUB_WORKSPACE=str(ROOT), MIRI_FAILURE_CALLS=str(calls))
  run = subprocess.run(["bash", "-e", "-c", step["run"]], env=environment, capture_output=True, text=True, check=False)
  assert run.returncode == 42, (run.returncode, run.stdout, run.stderr)
  assert calls.read_text().splitlines() == ["failed"]


@pytest.mark.parametrize(("scope", "path", "value"), [
  ("main", ("jobs", "native-arm", "uses"), "./.github/workflows/foreign.yml"),
  ("native", ("jobs", "xiaoge-runtime", "strategy", "matrix", "include"), []),
  ("native", ("jobs", "xiaoge-runtime", "env", "CI296_NATIVE_ARM_CALLER"), "other.yml"),
  ("native", ("jobs", "xiaoge-runtime", "timeout-minutes"), 60),
  ("native", ("jobs", "native-arm-checks", "needs"), ["xiaoge-runtime"]),
])
def test_xiaoge_native_contract_rejects_lost_architecture_or_caller(scope, path, value):
  main = yaml.safe_load((ROOT / ".github/workflows/rust.yml").read_text())
  native = yaml.safe_load((ROOT / ".github/workflows/rust-native-arm.yml").read_text())
  selected = main if scope == "main" else native
  for key in path[:-1]:
    selected = selected[key]
  selected[path[-1]] = deepcopy(value)
  with pytest.raises(AssertionError):
    assert_xiaoge_native_contract(main, native)
