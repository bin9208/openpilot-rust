# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy", "pycapnp"]
# ///
# Run with oracle Python: PYTHONPATH=.:opendbc_repo:rust/tools python rust/tools/card_hyundai_startup_source.py FIXTURE_DIR
from collections import deque
import contextlib
import io
import json
import logging
from pathlib import Path
import sys
from types import SimpleNamespace as NS

from can_source import load
from card_hyundai_controller_source import Settings as ControllerSettings


class Settings(ControllerSettings):
    def put_int(self,key:str,value:int)->None:
        self.values[key]=value


def trace(cp,settings,response:bool)->dict:
    from opendbc.car import uds,isotp_parallel_query
    from opendbc.car.hyundai import interface,hyundaicanfd
    from opendbc.car.can_definitions import CanData
    from opendbc.car.carlog import carlog
    pending=deque();output=dict(sent=[],receives=[],delays=[],now=0.,logs=[])
    class Capture(logging.Handler):
        def emit(self,record:logging.LogRecord)->None:
            output["logs"].append(("exception" if record.exc_info else record.levelname.lower(),record.getMessage()))
    capture=Capture();carlog.addHandler(capture)
    def clock()->float:
        output["now"]+=0.001
        return output["now"]
    def receive(wait_for_one:bool=False):
        output["receives"].append(wait_for_one)
        return [[frame] for frame in pending.popleft()] if pending else []
    def send(frames)->None:
        for frame in frames:
            output["sent"].append(dict(address=frame.address,data=list(frame.dat),bus=frame.src))
            if not response:continue
            data=frame.dat
            reply=None
            if data[0]==2 and data[1]==0x10:reply=bytes([2,0x50,data[2],0,0,0,0,0])
            elif data[0]&0xf0==0x10:reply=bytes([0x30,0,0,0,0,0,0,0])
            if reply is not None:pending.append([CanData(frame.address+8,reply,frame.src)])
    timer=NS(monotonic=clock,sleep=output["delays"].append)
    uds.time=isotp_parallel_query.time=timer
    interface.Params=hyundaicanfd.Params=lambda:settings
    printed=io.StringIO()
    with contextlib.redirect_stdout(printed):interface.CarInterface.init(cp,receive,send)
    carlog.removeHandler(capture);output["printed"]=printed.getvalue().splitlines()
    output["personality"]=settings.values["LongitudinalPersonalityMax"]
    output["radar_result"]=settings.values.get("EnableRadarTracksResult")
    return output


def main()->None:
    load()
    from opendbc.car import structs
    root=Path(sys.argv[1]).resolve();states=json.loads((root / "state.json").read_text());cases=[]
    for index,(state_index,tracks,extra_flags,response) in enumerate(((1,1,0,True),(5,1,0,True),(6,0,0,True),
            (7,0,32,True),(13,0,32,True),(1,1,0,False),(5,1,0,False),(0,0,0,True))):
        row=states[state_index];settings=Settings(row);settings.values["EnableRadarTracks"]=tracks
        with structs.CarParams.from_bytes((Path(row["source_dir"]) / "params.bin").read_bytes()) as reader:cp=reader.as_builder()
        cp.flags|=extra_flags
        output=root / f"startup-{index}-params.bin";output.write_bytes(cp.to_bytes())
        start=dict(settings.values)
        expected=trace(cp,settings,response)
        cases.append(dict(params=str(output),settings=start,fingerprints=row["fingerprints"],response=response,expected=expected))
    (root / "startup.json").write_text(json.dumps(cases)+"\n")
    print(f"Generated {len(cases)} source startup diagnostic transcripts")


if __name__=="__main__":
    main()
