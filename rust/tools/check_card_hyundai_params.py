# /// script
# requires-python = ">=3.12"
# dependencies = ["pycapnp"]
# ///
# Run after hyundai_parameters: PYTHONPATH=.:opendbc_repo:rust/tools python rust/tools/check_card_hyundai_params.py FIXTURE_DIR
import json
from pathlib import Path
import sys

from can_source import load


def main() -> None:
    load()
    from opendbc.car import structs
    fixture = Path(sys.argv[1])
    actual_root = Path((fixture / "native-params-path.txt").read_text())
    cases = json.loads((fixture / "detection.json").read_text())
    for index, case in enumerate(cases):
        with structs.CarParams.from_bytes((fixture / "source-params" / f"{index}.bin").read_bytes()) as source:
            with structs.CarParams.from_bytes((actual_root / f"{index}.bin").read_bytes()) as native:
                expected = source.to_dict()
                actual = native.to_dict()
                assert actual == expected, (case["candidate"], index, {
                    name:(expected.get(name), actual.get(name)) for name in expected.keys() | actual.keys()
                    if expected.get(name) != actual.get(name)})
    (fixture / "parameters-result.json").write_text(json.dumps({"matched_complete_schema":len(cases),
        "source_root":str(fixture / "source-params"),"native_root":str(actual_root),
        "source_error_identity":"KIA_K5_DL3_24_HEV"}) + "\n")
    print(f"Matched complete CarParams schemas for {len(cases)} Hyundai scenarios")


if __name__ == "__main__":
    main()
