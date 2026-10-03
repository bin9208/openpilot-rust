from __future__ import annotations

import argparse
from contextlib import redirect_stdout
import hashlib
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import uuid
from typing import TypedDict

from card_runtime_source import load_binding
from card_prepare_assets import prepare_assets


class FactoryInput(TypedDict):
  candidate: str
  fingerprints: list[tuple[int, list[tuple[int, int]]]]


def run(binary: Path, output: Path, binding: Path, numerics: Path, brand: str | None = None) -> None:
  free = shutil.disk_usage(output.parent).free
  assert free >= 25 * 1024**3 + 16 * 1024**2, free
  output.mkdir(parents=True, exist_ok=True)
  load_binding(binding)
  from openpilot.common.params import Params
  from opendbc.car.car_helpers import interfaces
  from opendbc.can.dbc import DBC
  cases: list[FactoryInput] = [{"candidate": candidate, "fingerprints": fingerprints} for candidate, fingerprints in [
    ("COMMA_BODY", []), ("MOCK", []), ("GENESIS_G70", []), ("HYUNDAI_IONIQ_5", []),
    ("TESLA_MODEL_3", []), ("TESLA_MODEL_Y", [(1, [(0x3df, 8)])]),
    ("MAZDA_CX5_2022", []), ("NISSAN_XTRAIL", []), ("NISSAN_LEAF", []), ("NISSAN_ALTIMA", []),
    ("COMMA_BODY", []), ("TESLA_MODEL_3", []), ("HYUNDAI_IONIQ_5", []), ("NISSAN_XTRAIL", []),
    ("CHRYSLER_PACIFICA_2018_HYBRID", []), ("CHRYSLER_PACIFICA_2019_HYBRID", []),
    ("CHRYSLER_PACIFICA_2018", []), ("CHRYSLER_PACIFICA_2020", []), ("DODGE_DURANGO", []),
    ("JEEP_GRAND_CHEROKEE", []), ("JEEP_GRAND_CHEROKEE_2019", []), ("RAM_1500_5TH_GEN", []), ("RAM_HD_5TH_GEN", []),
    ("CHRYSLER_PACIFICA_2018", []), ("RAM_1500_5TH_GEN", []), ("RAM_HD_5TH_GEN", []),
    ("RIVIAN_R1_GEN1", []), ("RIVIAN_R1_GEN1", []),
  ]]
  from opendbc.car.ford.values import CAR as FORD
  from opendbc.car.subaru.values import CAR as SUBARU
  from opendbc.car.toyota.values import CAR as TOYOTA
  from opendbc.car.gm.values import CAR as GM
  from opendbc.car.honda.values import CAR as HONDA
  from opendbc.car.volkswagen.values import CAR as VOLKSWAGEN
  cases.extend({"candidate": str(candidate), "fingerprints": []} for candidate in FORD
               if str(candidate) not in {"FORD_ESCAPE_MK4_5", "FORD_EXPEDITION_MK4"})
  cases.extend({"candidate": str(candidate), "fingerprints": []} for candidate in SUBARU)
  cases.extend([{"candidate": "FORD_F_150_MK14", "fingerprints": []}, {"candidate": "SUBARU_ASCENT", "fingerprints": []}])
  cases.extend({"candidate": str(candidate), "fingerprints": []} for candidate in TOYOTA)
  cases.append({"candidate": "TOYOTA_PRIUS", "fingerprints": []})
  cases.extend({"candidate": str(candidate), "fingerprints": []} for candidate in GM if str(candidate) != "GMC_YUKON_CC")
  cases.append({"candidate": "CHEVROLET_VOLT", "fingerprints": []})
  cases.extend({"candidate": str(candidate), "fingerprints": []} for candidate in HONDA)
  cases.extend([
    {"candidate": "HONDA_CIVIC", "fingerprints": []},
    {"candidate": "HONDA_ACCORD", "fingerprints": [(1, [(0x191, 8)])]},
    {"candidate": "HONDA_CRV_5G", "fingerprints": [(0, [(0x12f8bfa7, 8)])]},
    {"candidate": "HONDA_CRV_5G", "fingerprints": [(0, [(0x12f8bfa7, 8)])]},
  ])
  cases.extend({"candidate": str(candidate), "fingerprints": []} for candidate in VOLKSWAGEN)
  cases.extend({"candidate": candidate, "fingerprints": []}
               for candidate in ("VOLKSWAGEN_GOLF_MK7", "VOLKSWAGEN_PASSAT_NMS", "VOLKSWAGEN_ID4_MK1", "VOLKSWAGEN_ID4_MK2"))
  if brand is not None:
    cases = [case for case in cases if interfaces[case["candidate"]].__module__.split(".")[-2] == brand]
    assert cases, f"no factory cases for {brand}"
  dbc_root = output / "dbc"
  prepare_assets(Path(__file__).resolve().parents[2], dbc_root)
  import opendbc.can.dbc as dbc_module
  dbc_module.DBC_PATH = str(dbc_root.resolve())
  DBC.cache_clear()
  source = io.StringIO()
  with tempfile.TemporaryDirectory(prefix="card-factory-") as settings_root, redirect_stdout(source):
    os.environ["PARAMS_ROOT"] = settings_root
    for index, case in enumerate(cases):
      os.environ["OPENPILOT_PREFIX"] = f"factory-{uuid.uuid4()}"
      print(f"CASE {index}")
      fingerprints = {bus: {} for bus in range(8)}
      fingerprints.update({bus: dict(entries) for bus, entries in case["fingerprints"]})
      settings = Params()
      settings.put("FingerPrints", repr(fingerprints))
      interface = interfaces[case["candidate"]]
      cp = interface.get_params(case["candidate"], fingerprints, [], False, True, False)
      interface(cp)
  (output / "source.stdout").write_text(source.getvalue())
  (output / "inputs.json").write_text(json.dumps(cases, indent=2) + "\n")
  command = [str(binary), str(dbc_root.resolve()), str(Path("opendbc_repo/opendbc/car/torque_data").resolve()), str(numerics)]
  native = subprocess.run(command, input=json.dumps(cases).encode(), capture_output=True, check=False)
  (output / "native.stdout").write_bytes(native.stdout)
  (output / "native.stderr").write_bytes(native.stderr)
  result = {"command": command, "cases": len(cases), "source_cache_cleared_before_first_constructor": True,
            "returncode": native.returncode, "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
            "equal": source.getvalue() == native.stdout.decode()}
  (output / "result.json").write_text(json.dumps(result, indent=2) + "\n")
  assert native.returncode == 0, native.stderr
  assert result["equal"], "factory stdout differs; inspect retained source/native captures"
  print(f"PASS {len(cases)} fresh/repeated factory constructors")


if __name__ == "__main__":
  parser = argparse.ArgumentParser()
  for name in ("binary", "output", "binding", "numerics"):
    parser.add_argument(f"--{name}", type=Path, required=True)
  parser.add_argument("--brand")
  args = parser.parse_args()
  run(args.binary.resolve(), args.output.resolve(), args.binding.resolve(), args.numerics.resolve(), args.brand)
