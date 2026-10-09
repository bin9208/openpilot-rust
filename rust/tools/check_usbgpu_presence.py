"""Original/native startup presence on owned USB sysfs field boundaries."""

from __future__ import annotations

import argparse
from enum import Enum, auto
import json
from pathlib import Path
import subprocess
from typing import assert_never

from pytest import MonkeyPatch


class Effect(Enum):
  NORMAL = auto()
  BAD_UTF8 = auto()
  MISSING_SPEED = auto()
  UNRELATED = auto()
  MISSING_ROOT = auto()


def main() -> None:
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument("--binary", type=Path, required=True)
  parser.add_argument("--evidence", type=Path, required=True)
  args = parser.parse_args()
  args.evidence.mkdir(parents=True)
  from openpilot.selfdrive.modeld import helpers

  cases = (
    ("normal", "add1", "0001", "5000"),
    ("second-id", "3801", "0001", "10000"),
    ("usb2", "add1", "0001", "480"),
    ("fraction", "add1", "0001", "5000.5"),
    ("below", "add1", "0001", "4999.9"),
    ("exponent", "add1", "0001", "5e3"),
    ("underscore", "+0x_a_dd1", "0_001", "5_000.5"),
    ("invalid-underscore", "add1", "0001", "5__000"),
    ("nan", "add1", "0001", "NaN"),
    ("infinity", "add1", "0001", "iNFiNiTy"),
    ("wrong-id", "add2", "0001", "5000"),
    ("invalid-id", "add1_", "0001", "5000"),
    ("bad-utf8", "add1", "0001", "5000"),
    ("missing-speed", "add1", "0001", "5000"),
    ("unrelated-invalid", "add1", "0001", "5000"),
    ("missing-root", "add1", "0001", "5000"),
  )
  rows = []
  for name, vendor, product, speed in cases:
    root = args.evidence / name
    device = root / "owned"
    device.mkdir(parents=True)
    for filename, value in {"idVendor": vendor, "idProduct": product, "speed": speed}.items():
      (device / filename).write_text(value)
    effect = {
      "bad-utf8": Effect.BAD_UTF8,
      "missing-speed": Effect.MISSING_SPEED,
      "unrelated-invalid": Effect.UNRELATED,
      "missing-root": Effect.MISSING_ROOT,
    }.get(name, Effect.NORMAL)
    match effect:
      case Effect.BAD_UTF8:
        (device / "idVendor").write_bytes(b"\xff")
      case Effect.MISSING_SPEED:
        (device / "speed").unlink()
      case Effect.UNRELATED:
        (device / "manufacturer").write_bytes(b"\xff")
      case Effect.MISSING_ROOT:
        root = root / "absent"
      case Effect.NORMAL:
        pass
      case unreachable:
        assert_never(unreachable)
    with MonkeyPatch.context() as monkey:
      monkey.setattr(helpers, "Path", lambda _name, root=root: root)
      original = helpers.usbgpu_present()
    process = subprocess.run([str(args.binary)], input=json.dumps(str(root)), text=True, capture_output=True, check=False, timeout=5)
    actual = json.loads(process.stdout)
    row = {"name": name, "root": str(root), "source": original, "native": actual, "exit": process.returncode, "stderr": process.stderr}
    rows.append(row)
    (args.evidence / "result.json").write_text(json.dumps({"rows": rows}, indent=2) + "\n")
    assert process.returncode == 0 and actual == original, row
  print(json.dumps({"cases": len(rows), "differences": 0}))


if __name__ == "__main__":
  main()
