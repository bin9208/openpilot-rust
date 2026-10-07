# /// script
# requires-python = ">=3.12"
# dependencies = ["pycapnp"]
# ///
# Run after hyundai_state: PYTHONPATH=.:opendbc_repo:rust/tools python rust/tools/check_card_hyundai_state.py FIXTURE_DIR
import json
from pathlib import Path
import sys

from can_source import load


def main()->None:
    load()
    from opendbc.car import structs
    root=Path(sys.argv[1])
    native=Path((root / "native-state-path.txt").read_text())
    cases=json.loads((root / "state.json").read_text())
    count=0
    for index,case in enumerate(cases):
        results=structs.CarState.read_multiple_bytes((native / str(index) / "states.bin").read_bytes())
        for tick,(_,result) in enumerate(zip(case["steps"],results,strict=True)):
            with structs.CarState.from_bytes((Path(case["source_dir"]) / f"{tick}.bin").read_bytes()) as source:
                expected=source.to_dict();actual=result.to_dict()
                assert actual==expected,(case["candidate"],tick,{key:(expected.get(key),actual.get(key))
                    for key in expected.keys() | actual.keys() if expected.get(key)!=actual.get(key)})
            count+=1
    (root / "state-result.json").write_text(json.dumps(dict(complete_schema_ticks=count,
        scenarios=len(cases),native_root=str(native)))+"\n")
    print(f"Matched complete CarState schemas for {count} ticks")


if __name__=="__main__":
    main()
