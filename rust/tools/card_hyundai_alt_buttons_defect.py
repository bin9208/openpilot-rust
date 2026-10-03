# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy", "pycapnp"]
# ///
# Run with oracle Python: PYTHONPATH=.:opendbc_repo:rust/tools python rust/tools/card_hyundai_alt_buttons_defect.py FIXTURE_DIR OUTPUT_DIR
import contextlib
import hashlib
import io
import json
from pathlib import Path
import sys
import traceback
from types import SimpleNamespace as NS

from can_source import ROOT,load
from card_hyundai_controller_source import Settings


def main()->None:
    load()
    from opendbc.car import structs
    from opendbc.car.hyundai import carcontroller,carstate,hyundaicanfd,interface
    from opendbc.car.hyundai.values import DBC
    from opendbc.can import parser as parser_module
    root=Path(sys.argv[1]);output=Path(sys.argv[2]);output.mkdir(exist_ok=True)
    row=json.loads((root / "state.json").read_text())[12];settings=Settings(row)
    carstate.Params=carcontroller.Params=hyundaicanfd.Params=lambda:settings
    clock=NS(monotonic_ns=lambda:row["seed_now"]);parser_module.time=clock
    with structs.CarParams.from_bytes((Path(row["source_dir"]) / "params.bin").read_bytes()) as reader:cp=reader.as_builder()
    state=carstate.CarState(cp);ci=interface.CarInterface.__new__(interface.CarInterface)
    ci.CP=cp;ci.CS=state;ci.can_parsers=state.get_can_parsers(cp);ci.v_ego_cluster_seen=False
    controller=carcontroller.CarController(DBC[cp.carFingerprint],cp)
    for step in row["steps"][:125]:
        clock.monotonic_ns=lambda:step["now"]
        packets=[(packet["mono_time"],[(frame["address"],bytes(frame["data"]),frame["bus"]) for frame in packet["frames"]]) for packet in step["packets"]]
        ci.update(packets)
    assert cp.flags & 2 and not cp.openpilotLongitudinalControl
    cc=structs.CarControl.new_message();cc.cruiseControl.cancel=True;controller.frame=30
    printed=io.StringIO()
    with contextlib.redirect_stdout(printed):frames=controller.create_button_messages(cc.as_reader(),state,False)
    assert frames==[] and controller.cruise_buttons_msg_values is None and controller.cruise_buttons_msg_cnt==1
    try:dict((key,value[0]) for key,value in state.cruise_buttons_msg.items())
    except TypeError:
        (output / "scalar-index.traceback.txt").write_text(traceback.format_exc())
    source=ROOT / "opendbc_repo/opendbc/car/hyundai/carcontroller.py"
    result=dict(candidate=row["candidate"],flags=cp.flags,openpilot_longitudinal=cp.openpilotLongitudinalControl,
        value_types=sorted({type(value).__name__ for value in state.cruise_buttons_msg.values()}),
        cached_values=controller.cruise_buttons_msg_values,caught_scalar_errors=controller.cruise_buttons_msg_cnt,
        cancel_can=frames,printed=printed.getvalue().splitlines(),source_path=str(source),
        source_sha256=hashlib.sha256(source.read_bytes()).hexdigest(),traceback_sha256=hashlib.sha256((output / "scalar-index.traceback.txt").read_bytes()).hexdigest())
    (output / "result.json").write_text(json.dumps(result,indent=2)+"\n")
    print(json.dumps(result))


if __name__=="__main__":
    main()
