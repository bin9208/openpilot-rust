#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy==2.5.3", "pycapnp==2.1.0"]
# ///
# How to run: verified oracle Python rust/tools/check_card_gm.py --binary PATH --numerics DIR --evidence DIR
from __future__ import annotations

import argparse
import contextlib
import hashlib
import io
import json
from pathlib import Path
import subprocess
from can_source import ROOT, load
from check_card_mazda import compare, decoded as common_decoded
from card_vehicle_source import normalize
from card_qa.gm.source import trace
from card_qa.gm.scenarios import cases


def decoded(value):
  if "error" in value:
    return value
  if "initial_state" in value:
    from openpilot.cereal import car

    with car.CarState.from_bytes(bytes(value["initial_state"])) as state:
      value["initial_state"] = normalize(state.to_dict())
  return common_decoded(value)


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument("--binary", type=Path, required=True)
  parser.add_argument("--numerics", type=Path, required=True)
  parser.add_argument("--evidence", type=Path, required=True)
  parser.add_argument("--op", choices=["params", "runtime", "before_update", "numeric_error", "state_error"])
  args = parser.parse_args()
  args.evidence.mkdir(parents=True, exist_ok=True)
  (args.evidence / "result.json").unlink(missing_ok=True)
  (args.evidence / "failure.txt").unlink(missing_ok=True)
  load()
  import opendbc.can.dbc as source_dbc

  dbc_root = ROOT / "opendbc_repo/opendbc/dbc"
  source_dbc.DBC_PATH = str(dbc_root.resolve())
  request = [c for c in cases(args.op) if args.op is None or c["op"] == args.op]
  assert request
  source_log = io.StringIO()
  with contextlib.redirect_stdout(source_log), contextlib.redirect_stderr(source_log):
    expected = [trace(case) for case in request]
  (args.evidence / "input.json").write_text(json.dumps(request) + "\n")
  (args.evidence / "source.json").write_text(json.dumps(expected) + "\n")
  (args.evidence / "source.log").write_text(source_log.getvalue() or "no source diagnostics\n")
  output = args.evidence / "native.json"
  output.unlink(missing_ok=True)
  command = [
    str(args.binary.resolve()),
    str(output.resolve()),
    str(dbc_root.resolve()),
    str(ROOT / "opendbc_repo/opendbc/car/torque_data"),
    str(args.numerics.resolve()),
  ]
  (args.evidence / "command.json").write_text(json.dumps(command) + "\n")
  child = subprocess.run(command, input=json.dumps(request), text=True, capture_output=True, check=False)
  (args.evidence / "process.log").write_text(child.stdout + child.stderr + f"\nEXIT {child.returncode}\n")
  child.check_returncode()
  actual = json.loads(output.read_text())
  common_prints = iter(child.stdout.splitlines())
  for case in actual:
    if "steps" in case:
      case["prints"] = [next(common_prints), *case["prints"]]
  assert list(common_prints) == [], "unexpected native constructor prints"
  left, right = [decoded(v) for v in expected], [decoded(v) for v in actual]
  errors = []
  for case, source, native in zip(request, left, right, strict=True):
    if case["op"] == "state_error":
      source_error, native_error = source.pop("pre_update_error"), native.pop("pre_update_error")
      assert source_error == {"kind": "AssertionError", "message": ""}
      assert native_error == {"kind": "MissingMessage", "message": "GAS_SENSOR"}
      errors.append({"name": case["name"], "source": source_error, "native": native_error})
    if case["op"] == "numeric_error":
      source_error, native_error = source.pop("pre_update_error"), native.pop("pre_update_error")
      assert source_error == {"kind": "ValueError", "message": "cannot convert float NaN to integer"}
      assert native_error == {"kind": "Numeric"}
      errors.append({"name": case["name"], "source": source_error, "native": native_error})
    if case["op"] == "before_update":
      source_error, native_error = source.pop("pre_update_error"), native.pop("pre_update_error")
      name = source_error["message"].split("'")[-2]
      assert name in {"pscm_status", "pcm_acc_status"}
      assert source_error == {"kind": "AttributeError", "message": f"'CarState' object has no attribute '{name}'"}
      assert native_error == {"kind": "Stock", "message": f"GM stock message unavailable before state update: {name}"}
      errors.append(
        {
          "name": case["name"],
          "source": source_error,
          "native": native_error,
          "source_controller": source["final_controller"],
          "native_controller": native["final_controller"],
        }
      )
  (args.evidence / "before-update-errors.json").write_text(json.dumps(errors) + "\n")
  from opendbc.can.dbc import DBC

  fresh = []
  for case in request:
    if case["op"] == "runtime" and not any(c["candidate"] == case["candidate"] for c in fresh):
      DBC.cache_clear()
      fresh.append({"candidate": case["candidate"], "result": decoded(trace({**case, "steps": []}))})
  (args.evidence / "fresh-source-constructors.json").write_text(json.dumps(fresh) + "\n")
  (args.evidence / "source-fields.json").write_text(json.dumps(left) + "\n")
  (args.evidence / "native-fields.json").write_text(json.dumps(right) + "\n")
  try:
    compare(left, right)
  except AssertionError as error:
    (args.evidence / "failure.txt").write_text(str(error) + "\n")
    raise
  files = (
    list((ROOT / "opendbc_repo/opendbc/car/gm").glob("*.py"))
    + [ROOT / f"opendbc_repo/opendbc/dbc/{name}.dbc" for name in ("gm_global_a_powertrain_volt", "gm_global_a_object", "gm_global_a_chassis")]
    + [
      ROOT / "opendbc_repo/opendbc/car/interfaces.py",
      ROOT / "opendbc_repo/opendbc/car/__init__.py",
      ROOT / "opendbc_repo/opendbc/car/crc.py",
    ]
  )
  report = {
    "status": "pass",
    "cases": len(request),
    "frames": sum(len(c["steps"]) for c in request if c["op"] == "runtime"),
    "before_update_failures": sum(c["op"] == "before_update" for c in request),
    "numeric_failures": sum(c["op"] == "numeric_error" for c in request),
    "pedal_missing_message_failures": sum(c["op"] == "state_error" for c in request),
    "observable": (
      "full CarParams/CarState/Actuator wire fields; exact CAN frames and cadence; temporal GM state/controller; "
      + "ordered warning logs; lifecycle/Common constructor Params effects"
    ),
    "runtime_python": False,
    "binary_sha256": hashlib.sha256(args.binary.read_bytes()).hexdigest(),
    "source_sha256": {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest() for p in files},
    "runtime_dbc_sha256": {"gm_global_a_powertrain_volt.dbc": hashlib.sha256((dbc_root / "gm_global_a_powertrain_volt.dbc").read_bytes()).hexdigest()},
  }
  (args.evidence / "result.json").write_text(json.dumps(report, indent=2) + "\n")
  print(json.dumps(report))


if __name__ == "__main__":
  main()
