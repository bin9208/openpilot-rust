import argparse
import hashlib
import inspect
import json
from pathlib import Path
import subprocess

import pytest

from openpilot.selfdrive.carrot.radar_motion import lead_selection, trajectory_cutout
from plannerd_owner_source import difference
from radard_source_values import plain
from radard_cache_source import CacheObserver

ROOT = Path(__file__).resolve().parents[2]


def normalized(value):
  match value:
    case list():
      return [normalized(item) for item in value]
    case dict():
      result = {key: normalized(item) for key, item in value.items()}
      if "radarTrackId" in result and "status" in result:
        fields = {name: 0.0 for name in ("dRel", "yRel", "vRel", "aRel", "vLead", "aLead", "dPath", "vLat",
          "vLeadK", "aLeadK", "aLeadTau", "modelProb", "jLead", "score", "cutOutTime", "cutOutConfidence")}
        result = {**fields, "status": False, "fcw": False, "radar": False, "radarTrackId": -1, **result}
      return result
    case _:
      return value


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument("--output", type=Path, required=True)
  parser.add_argument("--binary", type=Path)
  args = parser.parse_args()
  args.output = args.output.resolve()
  args.output.mkdir(parents=True, exist_ok=False)
  owners, actions, expected = [], [], []
  depths = {}
  cache = CacheObserver()

  def install(kind, cls):
    initialize, update, reset = cls.__init__, cls.update, cls.reset

    def record_initialize(self, *positional, **keywords):
      owner = len(owners)
      owners.append(self)
      depths[owner] = 1
      initialize(self, *positional, **keywords)
      depths[owner] = 0
      actions.append({"op": "create", "kind": kind, "owner": owner})
      expected.append(None)

    def owner_id(value):
      return next(index for index, owner in enumerate(owners) if owner is value)

    def record_reset(self):
      owner = owner_id(self)
      reset(self)
      if depths[owner] == 0:
        actions.append({"op": "reset", "owner": owner})
        expected.append(None)

    def record_update(self, *positional, **keywords):
      owner = owner_id(self)
      bound = inspect.signature(update).bind(self, *positional, **keywords)
      bound.apply_defaults()
      values = {key: plain(value) for key, value in bound.arguments.items() if key != "self"}
      action = {"op": kind, "owner": owner, "input": values}
      if kind == "cutout":
        action["cache"] = cache.cursor()
      depths[owner] += 1
      try:
        result = update(self, *positional, **keywords)
      finally:
        depths[owner] -= 1
      actions.append(action)
      expected.append({"result": plain(result), "state": plain(self),
                       **({"cache": cache.fingerprints()} if kind == "cutout" else {})})
      return result

    cls.__init__, cls.reset, cls.update = record_initialize, record_reset, record_update

  for kind, cls in (("lead_two", lead_selection.DPathLeadTwoTracker),
                    ("shadow", lead_selection.DPathStationaryShadowTracker),
                    ("handoff", lead_selection.DPathStationaryPrimaryHandoffTracker),
                    ("cutout", trajectory_cutout.TrajectoryCutOutTracker)):
    install(kind, cls)
  status = pytest.main(  # noqa: TID251 - A single fresh process observes unchanged source-state regression cases.
    ["-o", "addopts=", "-q", "--confcutdir=openpilot/selfdrive/carrot/tests",
     "openpilot/selfdrive/carrot/tests/test_radar_motion_predictor.py", "openpilot/selfdrive/carrot/tests/test_trajectory_cutout.py"]
  )
  assert status == 0, status
  actions, expected = normalized(actions), normalized(expected)
  encoded = json.dumps({"actions": actions, "cache": cache.table()}, allow_nan=False)
  (args.output / "input.json").write_text(encoded + "\n")
  (args.output / "expected.json").write_text(json.dumps(expected, allow_nan=False) + "\n")
  receipt = {"status": "SOURCE_ONLY", "actions": len(actions), "owners": len(owners)}
  if args.binary is not None:
    child = subprocess.run([str(args.binary.resolve())], input=encoded, text=True, capture_output=True, check=False)
    (args.output / "actual.json").write_text(child.stdout)
    (args.output / "stderr.txt").write_text(child.stderr)
    child.check_returncode()
    mismatches = difference(expected, json.loads(child.stdout))
    receipt.update(status="PASS" if not mismatches else "FAIL", differences=len(mismatches), first_differences=mismatches[:100])
    receipt["binary_sha256"] = hashlib.sha256(args.binary.read_bytes()).hexdigest()
  (args.output / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
  assert receipt["status"] != "FAIL", receipt
  print(json.dumps(receipt))


if __name__ == "__main__":
  main()
