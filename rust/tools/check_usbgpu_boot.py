"""Compare original installed-model boot decisions using real owned worker children."""

from __future__ import annotations

import argparse
from enum import StrEnum
import json
import os
from pathlib import Path
import shutil
from typing import assert_never

from usbgpu_boot_fixture import Json, native, setup, source


class Case(StrEnum):
  INACTIVE = "inactive"
  USB2 = "usb2"
  PRESENT = "present"
  REUSE = "cache-reuse"
  IDENTITY = "identity-missing"
  SAVE = "save-error"
  STATUS = "compiled-status-error"
  TRANSIENT = "transient"
  FATAL = "fatal"
  DESCRIPTIVE = "irrelevant-field-invalid"


def main() -> None:
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument("--binary", type=Path, required=True)
  parser.add_argument("--worker", type=Path, required=True)
  parser.add_argument("--metadata", type=Path, required=True)
  parser.add_argument("--installed", type=Path, required=True)
  parser.add_argument("--package", type=Path, required=True)
  parser.add_argument("--evidence", type=Path, required=True)
  parser.add_argument("--reuse-source", type=Path)
  parser.add_argument(
    "--case",
    action="append",
    type=Case,
    choices=tuple(Case),
  )
  args = parser.parse_args()
  args.evidence.mkdir(parents=True)
  source_rows = {row["name"]: row["source"] for row in json.loads(args.reuse_source.read_text())["rows"]} if args.reuse_source else {}
  rows: list[dict[str, Json]] = []
  for name in args.case or tuple(Case):
    pair = {}
    saved = {key: os.environ.get(key) for key in ("USBGPU_WORKER_TEST_LOAD_ERROR", "USBGPU_WORKER_TEST_NONFINITE")}
    try:
      for side in ("source", "native"):
        if side == "source" and name in source_rows:
          pair[side] = source_rows[name]
          continue
        fixture = setup(args.evidence / name / side, args.installed, args.worker, args.metadata, speed=480 if name == "usb2" else 5000)
        match name:
          case Case.DESCRIPTIVE:
            (fixture.devices / "owned-usb/manufacturer").write_bytes(b"\xff")
          case Case.INACTIVE:
            (fixture.cache / "state.json").unlink()
          case Case.IDENTITY:
            shutil.rmtree(fixture.identity)
          case Case.SAVE:
            (fixture.installed.parent / "boot_validation.json").mkdir()
          case Case.STATUS:
            (fixture.cache / "status.json").mkdir()
            (fixture.installed.parent / "boot_validation.json").write_text('{"key":"stale"}')
          case Case.TRANSIENT:
            os.environ["USBGPU_WORKER_TEST_LOAD_ERROR"] = "pcie link not up"
          case Case.FATAL:
            os.environ["USBGPU_WORKER_TEST_NONFINITE"] = "1"
          case Case.USB2 | Case.PRESENT | Case.REUSE:
            pass
          case unreachable:
            assert_never(unreachable)
        observe = (lambda fixture=fixture: source(fixture)) if side == "source" else (lambda fixture=fixture: native(args.binary, args.package, fixture))
        actual = observe()
        if name == "cache-reuse":
          actual = {"first": actual, "second": observe()}
        pair[side] = actual
    finally:
      for key, value in saved.items():
        if value is None:
          os.environ.pop(key, None)
        else:
          os.environ[key] = value
    expected, actual = pair["source"], pair["native"]
    if name == "cache-reuse":
      expected, actual = expected["second"], actual["second"]
      assert expected["launches"] == [] and not actual["new_worker_inputs"]
    if name == "compiled-status-error":
      assert expected["error"] == "IsADirectoryError"
      assert actual["exit"] == 1 and actual["stderr"].strip() == "Is a directory (os error 21)"
    else:
      assert actual["exit"] == 0
    equal = (
      expected["prepared"] == actual["prepared"] and (expected["error"] is None) == (actual["error"] is None) and expected["observable"] == actual["observable"]
    )
    row = {"name": name, **pair, "equal": equal}
    rows.append(row)
    (args.evidence / "result.json").write_text(json.dumps({"rows": rows, "differences": sum(not row["equal"] for row in rows)}, indent=2) + "\n")
    assert equal, row
  print(json.dumps({"cases": len(rows), "differences": 0}))


if __name__ == "__main__":
  main()
