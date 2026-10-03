# /// script
# requires-python = ">=3.12"
# dependencies = ["pycapnp"]
# ///
# Run after hyundai_controller: PYTHONPATH=.:opendbc_repo:rust/tools python rust/tools/check_card_hyundai_controller.py FIXTURE_DIR
import json
from pathlib import Path
import sys

from can_source import load


def main()->None:
    load()
    from opendbc.car import structs
    root=Path(sys.argv[1]);native=Path((root / "native-controller-path.txt").read_text())
    cases=json.loads((root / "controller.json").read_text());count=frames=0
    for index,case in enumerate(cases):
        sources=structs.CarControl.Actuators.read_multiple_bytes((Path(case["source_dir"]) / "actuators.bin").read_bytes())
        results=structs.CarControl.Actuators.read_multiple_bytes((native / str(index) / "actuators.bin").read_bytes())
        messages=(native / str(index) / "can.jsonl").read_text().splitlines()
        for tick,(step,source,result) in enumerate(zip(case["steps"],sources,results,strict=True)):
            expected=source.to_dict();actual=result.to_dict()
            assert actual==expected,(index,tick,{key:(expected.get(key),actual.get(key)) for key in expected.keys() | actual.keys() if expected.get(key)!=actual.get(key)})
            assert json.loads(messages[tick])==step["can"],(index,tick,"CAN mismatch")
            count+=1;frames+=len(step["can"])
    (root / "controller-result.json").write_text(json.dumps(dict(complete_actuator_ticks=count,can_frames=frames,scenarios=len(cases),native_root=str(native)))+"\n")
    print(f"Matched {count} complete actuator schemas and {frames} byte-exact CAN frames")


if __name__=="__main__":
    main()
