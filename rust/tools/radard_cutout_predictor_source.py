import argparse
import hashlib
import inspect
import json
from pathlib import Path
import subprocess

import pytest

from openpilot.selfdrive.carrot.radar_motion import controller, predictor
from plannerd_owner_source import difference
from radard_cache_source import CacheObserver
from radard_controller_source import ROOT, TESTS
from radard_source_values import plain


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument("--output", type=Path, required=True)
  parser.add_argument("--binary", type=Path)
  parser.add_argument("--fixture", type=Path)
  args = parser.parse_args()
  args.output = args.output.resolve()
  args.output.mkdir(parents=True, exist_ok=False)
  if args.fixture is not None:
    encoded = (args.fixture / "input.json").read_text()
    expected = json.loads((args.fixture / "expected.json").read_text())
    source_receipt = json.loads((args.fixture / "receipt.json").read_text())
    receipt = {"status": "SOURCE_ONLY", "actions": source_receipt["actions"], "owners": source_receipt["owners"],
               "fixture": str(args.fixture.resolve()), "input_sha256": hashlib.sha256(encoded.encode()).hexdigest()}
  else:
    owners, actions, expected = [], [], []
    cache = CacheObserver()
    controller_update = controller.DPathRadarController.update
    update = predictor.RadarMotionPredictor.update
    controller_depth = 0

    def controller_context(self, *positional, **keywords):
      nonlocal controller_depth
      controller_depth += 1
      try:
        return controller_update(self, *positional, **keywords)
      finally:
        controller_depth -= 1

    def record(self, *positional, **keywords):
      if not controller_depth:
        return update(self, *positional, **keywords)
      assert self.cut_out_only and self.directional_min_consistency == 0.75
      if not any(owner is self for owner in owners):
        assert self._last_update_s is None
        actions.append({"op": "create", "owner": len(owners)})
        expected.append(None)
        owners.append(self)
      owner = next(index for index, value in enumerate(owners) if value is self)
      bound = inspect.signature(update).bind(self, *positional, **keywords)
      bound.apply_defaults()
      values = bound.arguments
      values["points"] = tuple(values["points"])
      assert all(point.source == "frontRadar" for point in values["points"])
      assert values["lead_one_d_rel"] is None and not values["allow_low_speed_identities"]
      assert values["prediction_identities"] is not None and values["scoped_points"] is not None
      inputs = {key: plain(value) for key, value in values.items() if key != "self"}
      action = {"op": "update", "owner": owner, "input": inputs, "cache": cache.cursor()}
      result = update(*bound.args, **bound.kwargs)
      actions.append(action)
      expected.append({"result": plain(result), "state": plain(self), "cache": cache.fingerprints()})
      return result

    controller.DPathRadarController.update = controller_context
    predictor.RadarMotionPredictor.update = record
    status = pytest.main(  # noqa: TID251
      ["-o", "addopts=", "-q", "--confcutdir=openpilot/selfdrive/carrot/tests",
       *[str(ROOT / "openpilot/selfdrive/carrot/tests" / name) for name in TESTS]]
    )
    assert status == 0, status
    encoded = json.dumps({"actions": actions, "cache": cache.table()}, allow_nan=False) + "\n"
    (args.output / "input.json").write_text(encoded)
    (args.output / "expected.json").write_text(json.dumps(expected, allow_nan=False) + "\n")
    receipt = {"status": "SOURCE_ONLY", "actions": len(actions), "owners": len(owners),
               "input_sha256": hashlib.sha256(encoded.encode()).hexdigest(),
               "source_sha256": hashlib.sha256(Path(predictor.__file__).read_bytes()).hexdigest()}
  if args.binary is not None:
    binary_sha256 = hashlib.sha256(args.binary.read_bytes()).hexdigest()
    child = subprocess.run([str(args.binary.resolve())], input=encoded, text=True, capture_output=True, check=False)
    (args.output / "actual.json").write_text(child.stdout)
    (args.output / "stderr.txt").write_text(child.stderr)
    child.check_returncode()
    mismatches = difference(expected, json.loads(child.stdout))
    receipt.update(status="PASS" if not mismatches else "FAIL", differences=len(mismatches), first_differences=mismatches[:100])
    assert hashlib.sha256(args.binary.read_bytes()).hexdigest() == binary_sha256, "binary changed during replay"
    receipt["binary_sha256"] = binary_sha256
  (args.output / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
  assert receipt["status"] != "FAIL", receipt
  print(json.dumps(receipt))


if __name__ == "__main__":
  main()
