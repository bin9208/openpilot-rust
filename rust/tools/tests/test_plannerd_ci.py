from __future__ import annotations

import json
import os
from pathlib import Path
import subprocess

import pytest
import yaml

from check_plannerd_ci import Check, ROOT, run_checks


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
  assert jobs["planner-runtime"]["strategy"]["matrix"]["runner"] == ["ubuntu-24.04", "ubuntu-24.04-arm"]
  assert {"planner-runtime", "planner-memory"} <= set(jobs["fast"]["needs"])
  gate = jobs["fast"]["steps"][0]
  environment = dict(os.environ, **dict.fromkeys(gate["env"], "success"))
  assert subprocess.run(["bash", "-e", "-c", gate["run"]], env=environment, check=False).returncode == 0
  for name in ("PLANNER", "PLANNER_MEMORY"):
    for state in ("failure", "cancelled", "skipped", ""):
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
