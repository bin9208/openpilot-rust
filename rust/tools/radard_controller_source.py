"""Record unchanged controller calls made by the existing source regression suite."""

import argparse
import hashlib
import inspect
import json
from pathlib import Path
import subprocess

import pytest

from openpilot.selfdrive.carrot.radar_motion import controller as original
from radard_compare import differences as difference
from radard_source_values import model, number, plain, point
from radard_cache_source import CacheObserver
from radard_selection_source import normalized

ROOT = Path(__file__).resolve().parents[2]
TESTS = ["test_radar_motion_predictor.py", "test_trajectory_cutin.py", "test_trajectory_cutout.py", "test_radard_dpath.py"]


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument("--output", type=Path, required=True)
  parser.add_argument("--binary", type=Path)
  parser.add_argument("--fixture", type=Path)
  parser.add_argument("--test", action="append")
  args = parser.parse_args()
  args.output = args.output.resolve()
  args.output.mkdir(parents=True, exist_ok=False)
  if args.fixture is not None:
    encoded = (args.fixture / "input.json").read_text()
    expected = json.loads((args.fixture / "expected.json").read_text())
    receipt = {**json.loads((args.fixture / "receipt.json").read_text()), "fixture": str(args.fixture.resolve())}
  else:
    owners, actions, expected = [], [], []
    primary_resets = set()
    inside_update = False
    cache = CacheObserver()
    initialize = original.DPathRadarController.__init__
    update = original.DPathRadarController.update
    reset = original.VisionRadarMatcher.reset

    def recorded_reset(self):
      if not inside_update:
        primary_resets.update(index for index, owner in enumerate(owners) if owner.primary_matcher is self)
      return reset(self)

    def recorded_initialize(self, *positional, **keywords):
      initialize(self, *positional, **keywords)
      bound = inspect.signature(initialize).bind(self, *positional, **keywords)
      bound.apply_defaults()
      options = {key: value for key, value in bound.arguments.items() if key != "self"}
      owners.append(self)
      actions.append({"op": "create", "owner": len(owners) - 1, "options": options})
      expected.append(None)

    def recorded_update(self, *positional, **keywords):
      nonlocal inside_update
      bound = inspect.signature(update).bind(self, *positional, **keywords)
      bound.apply_defaults()
      values = bound.arguments
      input_value = {
        "time_s": number(values["time_s"]), "v_ego": number(values["v_ego"]),
        "yaw_rate_rad_s": number(values["yaw_rate_rad_s"]),
        "radar_to_model_time_s": number(values["radar_to_model_time_s"]),
        "points": [point(value) for value in values["radar_points"]], "model": model(values["model"]),
      }
      predictor = self.primary_cut_out_predictor
      if isinstance(predictor, original.RadarMotionPredictor):
        predictor_override = None
      else:
        assert type(predictor).__name__ == "FixedPredictor", type(predictor)
        predictor_override = plain(predictor)
      cursor = cache.cursor()
      owner = next(index for index, value in enumerate(owners) if self is value)
      primary_reset = owner in primary_resets
      primary_resets.discard(owner)
      inside_update = True
      try:
        result = update(self, *positional, **keywords)
      finally:
        inside_update = False
      actions.append({"op": "update", "owner": owner, "input": input_value, "cache": cursor, "mode": self.enable_radar_tracks, "predictor_override": predictor_override, "primary_reset": primary_reset})
      expected.append({"output": plain(result), "state": plain(self), "cache": cache.fingerprints()})
      return result

    original.DPathRadarController.__init__ = recorded_initialize
    original.DPathRadarController.update = recorded_update
    original.VisionRadarMatcher.reset = recorded_reset
    status = pytest.main(  # noqa: TID251 - One isolated recorder runs the original regression suite once.
      ["-o", "addopts=", "-q", "--confcutdir=openpilot/selfdrive/carrot/tests",
       *[str(ROOT / "openpilot/selfdrive/carrot/tests" / name) for name in (args.test or TESTS)]]
    )
    assert status == 0, status
    actions, expected = normalized(actions), normalized(expected)
    encoded = json.dumps({"actions": actions, "cache": cache.table()}, allow_nan=False)
    (args.output / "input.json").write_text(encoded + "\n")
    (args.output / "expected.json").write_text(json.dumps(expected, allow_nan=False) + "\n")
    receipt = {
      "status": "SOURCE_ONLY", "actions": len(actions), "owners": len(owners),
      "tests": args.test or TESTS,
      "source_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
      "sources": {str(path.relative_to(ROOT)): hashlib.sha256(path.read_bytes()).hexdigest()
                  for path in sorted((ROOT / "openpilot/selfdrive/carrot/radar_motion").glob("*.py"))},
    }
  if args.binary is not None:
    binary_sha256 = hashlib.sha256(args.binary.read_bytes()).hexdigest()
    child = subprocess.run([str(args.binary.resolve())], input=encoded, text=True, capture_output=True, check=False)
    (args.output / "actual.json").write_text(child.stdout)
    (args.output / "stderr.txt").write_text(child.stderr)
    child.check_returncode()
    from radard_primary_source import canonical_maps
    mismatches = difference(canonical_maps(expected), canonical_maps(json.loads(child.stdout)))
    receipt.update(status="PASS" if not mismatches else "FAIL", differences=len(mismatches), first_differences=mismatches[:100])
    assert hashlib.sha256(args.binary.read_bytes()).hexdigest() == binary_sha256, "binary changed during replay"
    receipt["binary_sha256"] = binary_sha256
  (args.output / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
  assert receipt["status"] != "FAIL", receipt
  print(json.dumps(receipt))


if __name__ == "__main__":
  main()
