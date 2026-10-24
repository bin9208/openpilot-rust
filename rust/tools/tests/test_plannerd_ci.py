from __future__ import annotations

import json
import ast
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


def test_planner_host_arm_and_memory_results_are_required() -> None:
  jobs = yaml.safe_load((ROOT / ".github/workflows/rust.yml").read_text())["jobs"]
  runtime = jobs["planner-runtime"]
  assert runtime["strategy"]["matrix"]["runner"] == ["local-x64", "local-arm64-qemu"]
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
  assert {"planner-runtime", "planner-memory"} <= set(jobs["fast"]["needs"])
  gate = jobs["fast"]["steps"][0]
  assert gate["env"]["SOURCE_ALLOWED"] == trusted
  environment = dict(os.environ, **dict.fromkeys(gate["env"], "success"))
  environment["SOURCE_ALLOWED"] = "true"
  assert subprocess.run(["bash", "-e", "-c", gate["run"]], env=environment, check=False).returncode == 0
  for name in ("PLANNER", "PLANNER_MEMORY", "SOURCE_ALLOWED"):
    for state in ("failure", "cancelled", "skipped", "", "false"):
      result = subprocess.run(["bash", "-e", "-c", gate["run"]], env=dict(environment, **{name: state}), check=False)
      assert result.returncode != 0, (name, state)
  for job in ("planner-runtime", "planner-memory"):
    for step in jobs[job]["steps"]:
      if "run" in step:
        result = subprocess.run(["bash", "-n"], input=step["run"], text=True, capture_output=True, check=False)
        assert result.returncode == 0, result.stderr


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
