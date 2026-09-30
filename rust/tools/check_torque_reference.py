#!/usr/bin/env python3
"""Differential validation against original torqued.py (NumPy2.5.3, seeded RNG)."""

from __future__ import annotations
import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import subprocess
import numpy as np
from torque_reference import Parameters, original, close, compare, ROOT
from torque_cases import histories
from openpilot.cereal import car, log


def check(binary: Path, numerics: Path, output: Path) -> None:
  output.mkdir(parents=True, exist_ok=False)
  assert binary.is_file(), "torqued estimator executable missing"
  assert np.__version__ == "2.5.3", "source oracle must use the repository-locked NumPy2.5.3"
  scenarios = Counter()
  fields = errors = 0
  with subprocess.Popen([binary, numerics], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True) as process, (output / "trace.jsonl").open("w") as trace:
    assert process.stdin is not None and process.stdout is not None
    for scenario, command in histories():
      process.stdin.write(json.dumps(command) + "\n")
      process.stdin.flush()
      actual = json.loads(process.stdout.readline())
      expected_error = None
      try:
        match command["action"]:
          case "sample":
            np.random.seed(command["seed"])  # noqa: NPY002 - exact source RandomState contract.
            expected = [np.random.choice(np.arange(n), min(n, command["count"]), replace=False).tolist() for n in command["populations"]]  # noqa: NPY002
            assert actual["samples"] == expected
          case "new":
            params = Parameters(
              None if command["previous"] is None else bytes(command["previous"]), None if command["saved"] is None else bytes(command["saved"])
            )
            source = original(params)
            np.random.seed(command["seed"])  # noqa: NPY002 - exact source RandomState contract.
            with car.CarParams.from_bytes(bytes(command["car"])) as cp:
              estimator = source.TorqueEstimator(cp, decimated=command["decimated"], track_all_points=command["track_all"])
            original_estimate = estimator.estimate_params

            def capture_estimate(bound=original_estimate, target=estimator):
              result = bound()
              target.observed_raw = result
              return result

            estimator.estimate_params = capture_estimate
            assert actual["removed"] == params.removed
          case "add":
            for x, y in command["points"]:
              estimator.filtered_points.add_point(float(x), float(y))
            assert actual["counts"] == [len(b) for b in estimator.filtered_points.buckets.values()]
          case "event":
            before = len(estimator.all_torque_points)
            with log.Event.from_bytes(bytes(command["bytes"])) as event:
              estimator.handle_log(event.logMonoTime * 1e-9, event.which(), getattr(event, event.which()))
            point = estimator.all_torque_points[-1] if len(estimator.all_torque_points) > before else None
            assert (actual["point"] is None) == (point is None)
            if point is not None:
              assert all(close(float(a), b, tolerance=2e-12) for a, b in zip(actual["point"], point, strict=True)), (actual["point"], point)
            assert actual["counts"] == [len(b) for b in estimator.filtered_points.buckets.values()]
            assert actual["all"] == len(estimator.all_torque_points)
          case "message":
            estimator.observed_raw = [0.0, 0.0, 0.0]
            wanted = estimator.get_msg(valid=command["valid"], with_points=command["points"])
            wanted.logMonoTime = 0
            with log.Event.from_bytes(bytes(actual["packet"])) as event:
              fields += compare(event.to_dict(), wanted.to_dict())
            assert all(close(float(a), b) for a, b in zip(actual["raw"], estimator.observed_raw, strict=True))
            expected = [estimator.filtered_params[key].x for key in ("latAccelFactor", "latAccelOffset", "frictionCoefficient")]
            assert all(close(float(a), b) for a, b in zip(actual["filtered"], expected, strict=True))
            assert close(float(actual["decay"]), estimator.decay)
      except (ValueError, IndexError, TypeError) as error:
        expected_error = type(error).__name__
        assert "error" in actual, (expected_error, actual)
        errors += 1
      except (AssertionError, KeyError):
        (output / "failure.json").write_text(json.dumps({"scenario": scenario, "command": command, "actual": actual}, indent=2) + "\n")
        raise
      trace.write(json.dumps({"scenario": scenario, "actual": actual, "expected_error": expected_error}) + "\n")
      scenarios[scenario] += 1
    process.stdin.close()
    assert process.wait(timeout=5) == 0
  report = {
    "result": "pass",
    "numpy": np.__version__,
    "scenarios": scenarios,
    "steps": sum(scenarios.values()),
    "packet_fields": fields,
    "expected_errors": errors,
    "pose_history_tolerance": "2e-12 abs+rel",
    "fit_filter_tolerance": "2e-10 abs+rel",
    "wire": "1 Float32 ULP or absolute2e-10 near zero; discrete/nonfinite exact",
    "source_sha256": {
      str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest()
      for p in [ROOT / "openpilot/selfdrive/locationd/torqued.py", ROOT / "openpilot/selfdrive/locationd/helpers.py"]
    },
  }
  (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
  print(json.dumps(report, indent=2))


if __name__ == "__main__":
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument("--binary", type=Path, required=True)
  parser.add_argument("--numerics", type=Path, required=True)
  parser.add_argument("--output", type=Path, required=True)
  args = parser.parse_args()
  check(args.binary.resolve(), args.numerics.resolve(), args.output.resolve())
