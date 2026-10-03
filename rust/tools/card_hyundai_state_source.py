# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy", "pycapnp"]
# ///
# Run with the existing oracle Python: PYTHONPATH=.:opendbc_repo:rust/tools python rust/tools/card_hyundai_state_source.py FIXTURE_DIR DBC_DIR
import contextlib
import io
import json
import logging
from pathlib import Path
import sys

from can_source import load

CASES=(
    ("KIA_SORENTO",0,0,False),("HYUNDAI_SANTA_FE",0,0,True),("HYUNDAI_CASPER_EV",1,0,True),
    ("HYUNDAI_NEXO",0,0,False),("HYUNDAI_ELANTRA",0,0,False),("KIA_K7",1,0,True),
    ("KIA_EV6",0,0,True),("HYUNDAI_IONIQ_5",2,1,True),("HYUNDAI_TUCSON_4TH_GEN",0,0,False),
    ("KIA_PV5",2,0,True),("GENESIS_GV70_1ST_GEN",0,1,False),("KIA_CARNIVAL_4TH_GEN",2,1,True),
    ("KIA_EV6",0,0,False),("HYUNDAI_IONIQ_5",2,1,True),("KIA_CARNIVAL_4TH_GEN",0,1,True),
    ("HYUNDAI_TUCSON_4TH_GEN",0,0,True),("HYUNDAI_TUCSON_4TH_GEN",0,0,True),
)


class WarningCapture(logging.Handler):
    def __init__(self)->None:
        super().__init__();self.lines=[]

    def emit(self,record:logging.LogRecord)->None:
        self.lines.append(record.getMessage())


class Settings:
    def __init__(self,camera:int,hda:int)->None:
        self.values={"HyundaiCameraSCC":camera,"CanfdHDA2":hda,"AutoEngage":2,"ControlsReady":1,
            "VehicleNaviCanControl":1,"VehicleSpeedCameraDistanceTime":40,"VehicleNaviSchoolZoneControl":1}
        self.fingerprint={i:{} for i in range(8)}

    def get_int(self,key:str)->int:
        return self.values.get(key,0)

    def get_bool(self,key:str)->bool:
        return bool(self.values.get(key,0))

    def get(self,key:str)->str|None:
        return repr(self.fingerprint) if key=="FingerPrints" else None

    def put_bool(self,key:str,value:bool)->None:
        self.values[key]=int(value)

    def put_bool_nonblocking(self,key:str,value:bool)->None:
        self.put_bool(key,value)


def main()->None:
    DBC,CANPacker,_=load()
    from opendbc.car import Bus
    from opendbc.car import interfaces
    from opendbc.car.hyundai import interface,carstate,hyundaicanfd
    from opendbc.car.hyundai.values import CAR,HyundaiFlags
    from opendbc.can import parser as parser_module
    from opendbc.car.carlog import carlog
    warnings=WarningCapture();carlog.addHandler(warnings)
    root=Path(sys.argv[1]).resolve()
    dbcs=Path(sys.argv[2])
    rows=[]
    existing={case[0] for case in CASES}
    cases=(*CASES,*((str(candidate),0,0,True) for candidate in CAR if str(candidate) not in existing and str(candidate)!="KIA_K5_DL3_24_HEV"))
    for index,(candidate,camera,hda,alpha) in enumerate(cases):
        settings=Settings(camera,hda)
        if index==13:settings.fingerprint[4][0x2ff]=8
        interface.Params=interfaces.Params=carstate.Params=hyundaicanfd.Params=lambda:settings
        fd=CAR[candidate].config.flags & HyundaiFlags.CANFD
        definition=DBC(str(dbcs / ("hyundai_canfd_generated.dbc" if fd else "hyundai_kia_generic.dbc")))
        names=("CRUISE_BUTTONS_ALT","CRUISE_BUTTONS","TCS","MDPS","STEERING_SENSORS","WHEEL_SPEEDS",
            "ESP_STATUS","DOORS_SEATBELTS","BLINKERS","BLINKERS_ALT","BLINDSPOTS_REAR_CORNERS","TPMS","LOCAL_TIME",
            "LFA","LFAHDA_CLUSTER","SCC_CONTROL","ADRV_0x161","ADRV_0x200","ADRV_0x1ea","ADRV_0x160",
            "CCNC_0x162","STEER_TOUCH_2AF","CAM_0x2a4","HDA_INFO_4A3","NEW_MSG_4B4","NEW_MSG_4B9","NEW_MSG_4BE",
            "MANUAL_SPEED_LIMIT_ASSIST","TRAILER_STATUS") if fd else (
            "CLU11","CGW1","CGW2","WHL_SPD11","CLU15","SAS11","ESP12","MDPS12","TCS13","TCS15","TCS11",
            "ELECT_GEAR","E_EMS11","EMS16","EMS12","EMS20","TCU12","LVR12","LVR11","FCEV_ACCELERATOR",
            "TPMS11","SCC11","SCC12","FCA11","SCC13","SCC14","LCA11","EMS21","Navi_HU","LKAS11")
        if fd:
            accelerator="ACCELERATOR" if CAR[candidate].config.flags & HyundaiFlags.EV else "ACCELERATOR_ALT" if CAR[candidate].config.flags & HyundaiFlags.HYBRID else "ACCELERATOR_BRAKE_ALT"
            names=(*names,accelerator)
            if accelerator!="ACCELERATOR":
                names=(*names,"GEAR_SHIFTER")
            if camera or index%2:
                names=(*names,"LFA_ALT")
            if candidate=="KIA_PV5":
                names=tuple(name for name in names if name not in ("HDA_INFO_4A3","NEW_MSG_4B4","NEW_MSG_4B9","NEW_MSG_4BE"))
                names=(*names,"CANFD_HDA_INFO_364","CANFD_NAVI_PROFILE_093","CANFD_NAVI_STATUS_380")
            if index==12:names=tuple(name for name in names if name!="CRUISE_BUTTONS")
            if index==14:names=tuple(name for name in names if name!="CAM_0x2a4")+("CAM_0x362",)
            if index in (15,16):names=tuple(name for name in names if name!="GEAR_SHIFTER")+("GEAR_ALT" if index==15 else "GEAR_ALT_2",)
            if CAR[candidate].config.flags & HyundaiFlags.HYBRID:names=(*names,"HCU_STATUS_230")
        names=tuple(dict.fromkeys(name for name in names if name in definition.name_to_msg))
        can=hyundaicanfd.CanBus(None,settings.fingerprint,bool(hda))
        def bus_for(name):
            if not fd:
                return 2 if name in ("LKAS11",) or (camera and name in ("SCC11","SCC12","SCC13","SCC14","FCA11")) else 0
            if name in ("LFA","LFA_ALT","LFAHDA_CLUSTER","ADRV_0x161","ADRV_0x200","ADRV_0x1ea","ADRV_0x160","CCNC_0x162","CAM_0x2a4","CAM_0x362") or (camera and name=="SCC_CONTROL"):
                return can.CAM
            return can.ACAN if name=="CANFD_NAVI_STATUS_380" else can.ECAN
        for name in names:
            message=definition.name_to_msg[name]
            settings.fingerprint[bus_for(name)][message.address]=message.size
        with contextlib.redirect_stdout(io.StringIO()):
            cp=interface.CarInterface.get_params(candidate,settings.fingerprint,[],alpha,False,False)
        seed_now=2_000_000_000
        clock=NS(monotonic_ns=lambda:seed_now)
        parser_module.time=clock
        constructor=io.StringIO()
        with contextlib.redirect_stdout(constructor):
            state=carstate.CarState(cp)
            ci=interface.CarInterface.__new__(interface.CarInterface)
            ci.CP=cp;ci.CS=state;ci.can_parsers=state.get_can_parsers(cp);ci.v_ego_cluster_seen=False
        source_dir=root / f"source-state-{index}"
        source_dir.mkdir(exist_ok=True)
        (source_dir / "params.bin").write_bytes(cp.to_bytes())
        packer=CANPacker(str(dbcs / ("hyundai_canfd_generated.dbc" if fd else "hyundai_kia_generic.dbc")))
        steps=[]
        for tick in range(320):
            now=seed_now+(tick+1)*10_000_000
            clock.monotonic_ns=lambda:now
            frames=[]
            if tick<280:
                for name in names:
                    values={key:0. for key in definition.name_to_msg[name].sigs if key not in ("COUNTER","CHECKSUM")}
                    for key in values:
                        if "WHEEL_SPEED" in key or key.startswith("WHL_SPD_"):
                            values[key]=72.
                    for key,value in {"CF_Clu_Vanz":72.,"CLU_SPEED":72.,"CF_Gway_DrvSeatBeltSw":1.,"DRIVER_SEATBELT":1.,
                        "MainMode_ACC":1.,"ACCMode":1.,"ACC_REQ":1.,"VSetDis":85.,"SPEED_LIMIT":60.,"MapSource":2.,
                        "CountryCode":410.,"YEAR":26.,"MONTH":10.,"DATE":2.,"HOURS":13.,"MINUTES":2.,"SECONDS":49.,
                        "PROLONG_VALUE":208.,"PROLONG_OFFSET":35.,"PROLONG_PROFILE_TYPE":16.,"SECTION_ALERT":0.,
                        "SAS_Angle":15.,"STEERING_ANGLE":-15.,"STEERING_ANGLE_2":-15.,"CF_Clu_CruiseSwState":1. if tick%40<20 else 0.,
                        "CRUISE_BUTTONS":1. if tick%40<20 else 0.,"AVH_LAMP":2. if 200<=tick<230 else 3.,
                        "AVH_Sta":2. if 200<=tick<240 else 0.,"TRAILER_CONNECTED":1. if 170<=tick<190 else 0.,
                        "SCR_UREA_LEVEL":63.,"PRESSURE_FL":35.,"PRESSURE_FR":36.,"PRESSURE_RL":37.,"PRESSURE_RR":38.}.items():
                        if key in values:
                            values[key]=value
                    if "HYBRID_POWER_FLOW_MODE" in values:values["HYBRID_POWER_FLOW_MODE"]=1. if tick<200 else 3.
                    if fd and name==state.gear_msg_canfd:
                        values["GEAR"]=next((key for key,text in state.shifter_values.items() if text in ("D","DRIVE")),0)
                    if not fd:
                        for gear_message,signal in (("ELECT_GEAR","Elect_Gear_Shifter"),("LVR12","CF_Lvr_Gear"),("CLU15","CF_Clu_Gear"),("TCU12","CUR_GR"),("EMS20","HYDROGEN_GEAR_SHIFTER")):
                            if name==gear_message and signal in values:
                                values[signal]=next((key for key,text in state.shifter_values.items() if text in ("D","DRIVE")),0)
                    address,data,bus=packer.make_can_msg(name,bus_for(name),values)
                    frames.append(dict(address=address,data=list(data),bus=bus))
            packet=dict(mono_time=now,frames=frames)
            printed=io.StringIO();warnings.lines.clear()
            with contextlib.redirect_stdout(printed):result=ci.update([(now,[(f["address"],bytes(f["data"]),f["bus"]) for f in frames])])
            (source_dir / f"{tick}.bin").write_bytes(result.to_bytes())
            steps.append(dict(now=now,packets=[packet],diagnostics=printed.getvalue().splitlines(),warnings=list(warnings.lines)))
        rows.append(dict(candidate=candidate,camera=camera,hda2=hda,settings=settings.values,
            fingerprints=[(bus,list(values.items())) for bus,values in settings.fingerprint.items()],
            seed_now=seed_now,source_dir=str(source_dir),steps=steps,constructor=constructor.getvalue().splitlines()))
    (root / "state.json").write_text(json.dumps(rows)+"\n")
    print(f"Generated {len(rows)*320} complete source CarState ticks")


from types import SimpleNamespace as NS

if __name__=="__main__":
    main()
