from __future__ import annotations

import json
import os
from pathlib import Path
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


def test_both_host_architectures_and_native_memory_are_required_gates() -> None:
  jobs = yaml.safe_load((ROOT / ".github/workflows/rust.yml").read_text())["jobs"]
  runtime = jobs["xiaoge-runtime"]
  assert runtime["strategy"]["matrix"]["runner"] == ["local-x64", "local-arm64-qemu"]
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
