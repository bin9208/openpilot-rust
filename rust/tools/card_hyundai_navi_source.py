# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy", "pycapnp"]
# ///
# Run with the existing oracle Python: PYTHONPATH=.:opendbc_repo:rust/tools python rust/tools/card_hyundai_navi_source.py FIXTURE_DIR
import itertools
import json
from pathlib import Path
import sys
from types import SimpleNamespace as NS

from can_source import load


class Settings:
    def __init__(self,mode:int,school:bool)->None:
        self.mode=mode
        self.school=school

    def get_int(self,key:str)->int:
        return {"VehicleNaviCanControl":self.mode,"VehicleSpeedCameraDistanceTime":40}.get(key,0)

    def get_bool(self,key:str)->bool:
        return self.school if key=="VehicleNaviSchoolZoneControl" else False

    def get(self,key:str)->str|None:
        return "{0:{},1:{},2:{},3:{},4:{},5:{},6:{},7:{}}" if key=="FingerPrints" else None

    def put_bool(self,key:str,value:bool)->None:
        return None


def main()->None:
    load()
    from opendbc.car import structs
    from opendbc.car.hyundai import carstate,hyundaicanfd
    root=Path(sys.argv[1])
    cases=[]
    fields={"speed_limit":"speedLimit","speed_limit_distance":"speedLimitDistance","bump_distance":"speedBumpDistance",
        "school_zone":"schoolZoneActive","active":"vehicleNaviActive","section_active":"vehicleNaviSectionActive",
        "available":"vehicleNaviAvailable","speed":"vehicleNaviSpeed"}
    for wrapped,mode,scenario in itertools.product((False,True),(0,1,2,3),range(10)):
        settings=Settings(mode,True)
        carstate.Params=hyundaicanfd.Params=lambda:settings
        cp=structs.CarParams(carFingerprint="KIA_PV5" if wrapped else "HYUNDAI_IONIQ_5",flags=8192|2048,
            safetyConfigs=[dict(safetyModel="hyundaiCanfd")],wheelSpeedFactor=1.)
        cs=carstate.CarState(cp)
        ticks=[]
        profile_timestamp=segment_timestamp=1_000_000_000
        for tick in range(300):
            now=1_000_000_000+tick*10_000_000
            speed=30. if scenario==3 else 60.
            warning=scenario!=0 and (tick<150 or scenario in (1,3,5,6))
            hda=dict(SPEED_LIMIT=speed,MapSource=2. if warning else 0.,LinkClass=1. if scenario==4 else 0.)
            status=dict(SPEED_LIMIT=speed,SECTION_ALERT=1. if scenario==7 and 30<=tick<60 else 0.,CAMERA_STATUS=64. if tick%100<50 else 4.)
            profile=dict(PROLONG_VALUE=6. if scenario==2 else 119. if scenario==3 else 208.,
                PROLONG_OFFSET=0. if scenario==3 else 35.,PROLONG_CYCLIC_COUNTER=0.,PROLONG_UPDATE=0.,
                PROLONG_PATH_INDEX=2.,PROLONG_PROFILE_TYPE=16.)
            if tick==80 and scenario==5:
                profile_timestamp=now
                profile["PROLONG_OFFSET"]=500.
            if scenario==5 and tick>80:
                profile["PROLONG_OFFSET"]=500.
            route=2 if scenario==6 and tick>=90 else 1
            if scenario==6 and tick==90:
                segment_timestamp=now
            raw=(route<<22)|(2<<13)|(1<<24 if scenario==4 else 7<<24)
            segment={f"BYTE_{index+1}":float((raw>>(8*index))&255) for index in range(8)}
            position=dict(POS_RANGE_AVG_SPEED=50. if scenario==8 and tick<200 else 0.)
            timestamps=dict(profile_timestamp=profile_timestamp,position_timestamp=now if scenario==8 else 0,
                segment_timestamp=segment_timestamp,hda_timestamp=now if scenario!=9 or tick<150 else 1_000_000_000,
                status_timestamp=now if scenario!=9 or tick<150 else 1_000_000_000,last_update=now)
            source_input=dict(hda=hda,position=None if wrapped else position,segment=None if wrapped else segment,
                profile=profile,status=status if wrapped else None,pt_timeout=False,alt_timeout=False,
                hda_size=16,status_size=24,metric=True,**timestamps)
            cs.hda_info_4a3=hda
            cs.navi_profile_4be=profile
            cs.navi_segment_4b9=source_input["segment"]
            cs.navi_position_4b4=source_input["position"]
            cs.navi_status_380=source_input["status"]
            names={cs.navi_profile_msg:{"x":profile_timestamp},"NEW_MSG_4B4":{"x":timestamps["position_timestamp"]},
                "NEW_MSG_4B9":{"x":segment_timestamp},"CANFD_HDA_INFO_364":{"x":timestamps["hda_timestamp"]}}
            parser=NS(ts_nanos=names,_last_update_nanos=now,bus_timeout=False,dat={0x364:bytes(16)})
            alt=NS(ts_nanos={"CANFD_NAVI_STATUS_380":{"x":timestamps["status_timestamp"]}},
                _last_update_nanos=now,bus_timeout=False,dat={0x380:bytes(24)})
            ret=structs.CarState(vEgo=20.,speedLimit=speed)
            cs.is_metric=True
            changed=cs._update_vehicle_speed_camera_params()
            camera=cs._update_pv5_camera_warning(parser,alt) if wrapped else warning
            camera=cs._update_vehicle_navi_events(parser,ret,camera,alt) or camera
            cs.update_speed_limit(ret,camera,changed)
            expected={name:getattr(ret,source) for name,source in fields.items()}
            ticks.append(dict(input=source_input,speed=20.,initial_limit=speed,warning=warning,expected=expected,
                total_distance=cs.totalDistance,camera_target=cs.vehicleNaviCameraTarget,
                status_target=cs.vehicleNaviCameraStatusTarget))
        cases.append(dict(wrapped=wrapped,mode=mode,school=True,ticks=ticks))
    (root / "navigation.json").write_text(json.dumps(cases)+"\n")
    print(f"Generated {len(cases)*300} exact source navigation ticks")


if __name__=="__main__":
    main()
