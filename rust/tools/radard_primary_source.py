"""Observe the unchanged primary matcher and its process-wide cache boundaries."""

import argparse
import hashlib
import inspect
import json
from pathlib import Path
import subprocess

import pytest

from openpilot.selfdrive.carrot.radar_motion import primary
from plannerd_owner_source import difference
from radard_cache_source import CacheObserver
from radard_controller_source import ROOT, TESTS
from radard_source_values import model, plain


def canonical_maps(value):
  match value:
    case list():
      return [canonical_maps(item) for item in value]
    case dict():
      result = {key: canonical_maps(item) for key, item in value.items()}
      for name in ("_observed_since_s", "_observed_last_s"):
        if name in result and "entries" in result[name]:
          result[name]["entries"].sort(key=lambda entry: entry[0])
      return result
    case _:
      return value


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
    owners, actions, expected, depths = [], [], [], {}
    cache = CacheObserver()
    initialize = primary.VisionRadarMatcher.__init__

    def record_initialize(self):
      owner = len(owners)
      owners.append(self)
      depths[owner] = 1
      initialize(self)
      depths[owner] = 0
      actions.append({"op": "create", "owner": owner})
      expected.append(None)

    def install(name, operation):
      original = getattr(primary.VisionRadarMatcher, name)

      def record(self, *positional, **keywords):
        owner = next(index for index, value in enumerate(owners) if value is self)
        if depths[owner]:
          return original(self, *positional, **keywords)
        bound = inspect.signature(original).bind(self, *positional, **keywords)
        bound.apply_defaults()
        values = bound.arguments
        for field in ("points", "stationary_points"):
          if field in values and values[field] is not None:
            values[field] = tuple(values[field])
        inputs = {key: model(value) if key == "model" else plain(value) for key, value in values.items() if key != "self"}
        action = {"op": operation, "owner": owner, "input": inputs, "cache": cache.cursor()}
        depths[owner] += 1
        try:
          result = original(*bound.args, **bound.kwargs)
        finally:
          depths[owner] -= 1
        actions.append(action)
        expected.append({"result": plain(result), "state": plain(self), "cache": cache.fingerprints()})
        return result

      setattr(primary.VisionRadarMatcher, name, record)

    primary.VisionRadarMatcher.__init__ = record_initialize
    for name, operation in (("match", "update"), ("reset", "reset"), ("_stationary_closer_handoff_ready", "handoff")):
      install(name, operation)
    status = pytest.main(  # noqa: TID251 - Fresh process observes all existing radar-state regression cases unchanged.
      ["-o", "addopts=", "-q", "--confcutdir=openpilot/selfdrive/carrot/tests",
       *[str(ROOT / "openpilot/selfdrive/carrot/tests" / name) for name in TESTS]]
    )
    assert status == 0, status
    encoded = json.dumps({"actions": actions, "cache": cache.table()}, allow_nan=False) + "\n"
    (args.output / "input.json").write_text(encoded)
    (args.output / "expected.json").write_text(json.dumps(expected, allow_nan=False) + "\n")
    receipt = {"status": "SOURCE_ONLY", "actions": len(actions), "owners": len(owners),
               "input_sha256": hashlib.sha256(encoded.encode()).hexdigest(),
               "source_sha256": hashlib.sha256(Path(primary.__file__).read_bytes()).hexdigest()}
  if args.binary is not None:
    binary_sha256 = hashlib.sha256(args.binary.read_bytes()).hexdigest()
    child = subprocess.run([str(args.binary.resolve())], input=encoded, text=True, capture_output=True, check=False)
    (args.output / "actual.json").write_text(child.stdout)
    (args.output / "stderr.txt").write_text(child.stderr)
    child.check_returncode()
    mismatches = difference(canonical_maps(expected), canonical_maps(json.loads(child.stdout)))
    receipt.update(status="PASS" if not mismatches else "FAIL", differences=len(mismatches), first_differences=mismatches[:100])
    assert hashlib.sha256(args.binary.read_bytes()).hexdigest() == binary_sha256, "binary changed during replay"
    receipt["binary_sha256"] = binary_sha256
  (args.output / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
  assert receipt["status"] != "FAIL", receipt
  print(json.dumps(receipt))


if __name__ == "__main__":
  main()
