# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy", "pycapnp"]
# ///
# Run with oracle Python: PYTHONPATH=.:opendbc_repo:rust/tools python rust/tools/card_hyundai_controller_source.py FIXTURE_DIR DBC_DIR
import contextlib
import io
import json
from pathlib import Path
import sys
from types import SimpleNamespace as NS

from can_source import load
from card_hyundai_state_source import Settings as StateSettings,WarningCapture


class Settings(StateSettings):
    def __init__(self,row:dict)->None:
        super().__init__(row["camera"],row["hda2"])
        self.values.update(row["settings"])
        self.values.update(MaxAngleFrames=89,CruiseButtonTest1=8,CruiseButtonTest2=30,CruiseButtonTest3=1,
            HDPuse=1,CarrotCruiseDecel=-1,CarrotCruiseAtcDecel=-1,HapticFeedbackWhenSpeedCamera=2)
        self.fingerprint={int(bus):dict(messages) for bus,messages in row["fingerprints"]}

    def get_float(self,key:str)->float:
        import numpy as np
        return float(np.float32(self.values.get(key,0)))

    def get(self,key:str)->bytes|str|None:
        return repr(self.fingerprint) if key=="FingerPrints" else str(self.values[key]).encode() if key in self.values else None


def control(structs,tick:int):
    cc=structs.CarControl.new_message()
    cc.enabled=30<=tick<290;cc.latActive=40<=tick<280;cc.longActive=cc.enabled
    cc.actuators.torque=((tick%90)-45)/45
    cc.actuators.steeringAngleDeg=((tick%120)-60)*3
    cc.actuators.accel=-1.8 if tick<200 else 0.8
    cc.actuators.aTarget=cc.actuators.accel;cc.actuators.jerk=-0.35 if tick%80<40 else 0.2
    cc.actuators.longControlState="off" if not cc.enabled else "stopping" if 150<=tick<185 else "pid"
    cc.cruiseControl.cancel=260<=tick<265;cc.cruiseControl.resume=240<=tick<250;cc.cruiseControl.override=210<=tick<220
    hud=cc.hudControl;hud.setSpeed=24.0;hud.leadDistanceBars=3;hud.leadVisible=tick%100<70
    hud.leadDistance=25;hud.leadRelSpeed=-1.2;hud.leftLaneVisible=True;hud.rightLaneVisible=tick%30<25
    hud.leftLaneDepart=120<=tick<140;hud.rightLaneDepart=180<=tick<195
    hud.modelDesire=3 if 100<=tick<130 else 4 if 230<=tick<250 else 0
    hud.activeCarrot=3 if tick in (90,91,190) else 2 if tick%50<40 else 1
    hud.visualAlert="steerRequired" if tick%70<10 else "none"
    return cc


def main()->None:
    load()
    from opendbc.car import structs,interfaces
    from opendbc.car.hyundai import carcontroller,carstate,hyundaicanfd,interface
    from opendbc.car.hyundai.values import DBC
    from opendbc.car.carlog import carlog
    from opendbc.can import parser as parser_module
    from openpilot.cereal import log
    root=Path(sys.argv[1]).resolve()
    rows=json.loads((root / "state.json").read_text());outputs=[]
    warnings=WarningCapture();carlog.addHandler(warnings)
    for index,row in enumerate(rows):
        settings=Settings(row)
        for module in (interfaces,interface,carstate,carcontroller,hyundaicanfd):
            module.Params=lambda:settings
        clock=NS(monotonic_ns=lambda:row["seed_now"]);parser_module.time=clock
        with structs.CarParams.from_bytes((Path(row["source_dir"]) / "params.bin").read_bytes()) as reader:
            cp=reader.as_builder()
        constructor=io.StringIO()
        with contextlib.redirect_stdout(constructor):
            state=carstate.CarState(cp)
            ci=interface.CarInterface.__new__(interface.CarInterface)
            ci.CP=cp;ci.CS=state;ci.can_parsers=state.get_can_parsers(cp);ci.v_ego_cluster_seen=False
            controller=carcontroller.CarController(DBC[cp.carFingerprint],cp)
        output=root / f"source-controller-{index}";output.mkdir(exist_ok=True)
        streams={name:(output / f"{name}.bin").open("wb") for name in ("control","model","radar","actuators")}
        start_settings=dict(settings.values);records=[]
        for tick,step in enumerate(row["steps"]):
            clock.monotonic_ns=lambda:step["now"]
            packets=[(packet["mono_time"],[(frame["address"],bytes(frame["data"]),frame["bus"]) for frame in packet["frames"]]) for packet in step["packets"]]
            printed=io.StringIO();warnings.lines.clear()
            with contextlib.redirect_stdout(printed):result=ci.update(packets)
            result.latEnabled=True;result.carrotCruise=1 if 100<=tick<125 else 0
            result.activateCruise=-1 if 20<=tick<30 else 1 if 10<=tick<20 else 0
            result.steeringTorque=290 if 80<=tick<110 else -260 if 180<=tick<205 else 0
            result.steeringPressed=90<=tick<105 or 185<=tick<200
            result.steeringAngleDeg=90 if 140<=tick<250 else -12
            state.softHoldActive=2 if 160<=tick<190 else 0
            cc=control(structs,tick)
            model=log.ModelDataV2.new_message();model.position.x=[0,30,80];model.position.y=[0,0.4,1.0]
            model.position.yStd=[0.2]*11;model.meta.desire="laneChangeLeft" if 100<=tick<130 else "none"
            model.meta.desireState=[0.,0.,0.,1. if 100<=tick<130 else 0.,0.]
            model.meta.laneChangeAvailableLeft=True;model.meta.laneChangeAvailableRight=tick%40<20
            radar=log.RadarState.new_message();radar.leadOne.status=cc.hudControl.leadVisible
            radar.leadOne.radar=True;radar.leadOne.radarTrackId=3;radar.leadOne.dRel=25;radar.leadOne.yRel=-0.5;radar.leadOne.vRel=-1.2
            state.modelV2=model;state.radarState=radar
            updates={}
            if tick==150:updates={"CustomSteerMax":330,"CustomSteerDeltaUp":4,"CustomSteerDeltaDownLC":8,"PaddleMode":1,"LaneLineCheck":1}
            settings.values.update(updates)
            with contextlib.redirect_stdout(printed):actuators,frames=controller.update(cc.as_reader(),state,step["now"])
            for name,message in (("control",cc),("model",model),("radar",radar),("actuators",actuators)):
                streams[name].write(message.to_bytes())
            records.append(dict(soft_hold=state.softHoldActive,settings=updates,can=[dict(address=address,data=list(data),bus=bus) for address,data,bus in frames],
                diagnostics=printed.getvalue().splitlines(),warnings=list(warnings.lines)))
        outputs.append(dict(state_case=index,source_dir=str(output),settings=start_settings,steps=records,constructor=constructor.getvalue().splitlines()))
        for stream in streams.values():stream.close()
    (root / "controller.json").write_text(json.dumps(outputs)+"\n")
    print(f"Generated {sum(len(row['steps']) for row in outputs)} complete controller ticks")


if __name__=="__main__":
    main()
