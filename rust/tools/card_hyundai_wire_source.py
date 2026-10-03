# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy", "pycapnp"]
# ///
# Run with the existing oracle Python: PYTHONPATH=.:opendbc_repo:rust/tools python rust/tools/card_hyundai_wire_source.py FIXTURE_DIR DBC_DIR
import itertools
import json
from pathlib import Path
import sys
from types import SimpleNamespace

from can_source import load


def main() -> None:
    DBC, CANPacker, _ = load()
    from opendbc.car.hyundai import hyundaicanfd, hyundaican
    from opendbc.car.hyundai.values import CAR
    fixture = Path(sys.argv[1])
    dbcs = Path(sys.argv[2])
    path = str(dbcs / "hyundai_canfd_generated.dbc")
    definition = DBC(path)
    messages = {name:{signal:0. for signal in definition.name_to_msg[name].sigs}
        for name in ("MDPS","STEER_TOUCH_2AF","LFA","LFA_ALT","ADRV_0x161")}
    for data in messages.values():
        if "COUNTER" in data:
            data["COUNTER"] = 13.
    messages["MDPS"]["STEERING_COL_TORQUE"] = 12.
    messages["LFA"]["STEER_REQ"] = 1.
    messages["LFA_ALT"]["LKAS_ANGLE_ACTIVE"] = 2.
    cases = []
    for camera, angle, active, enabled, long, flags, frame, emergency in itertools.product(
        (False,True),(False,True),(False,True),(False,True),(False,True),
        (0,1,513,9),(0,10,40,1000),(False,True)):
        bus = dict(ecan=0,acan=1,cam=2)
        input_values = dict(bus=bus,flags=8192|flags,frame=frame,enabled=enabled,lat_active=active,
            cc_lat_active=active,longitudinal=long,torque=-127.,angle=18.75,max_torque=167.,angle_control=angle)
        source = dict(mdps=messages["MDPS"],touch=messages["STEER_TOUCH_2AF"],lfa=messages["LFA"],
            lfa_alt=messages["LFA_ALT"],adrv_161=messages["ADRV_0x161"].copy())
        source["adrv_161"]["ALERTS_1"] = 12. if emergency else 0.
        cp = SimpleNamespace(flags=input_values["flags"],openpilotLongitudinalControl=long)
        can = SimpleNamespace(ECAN=0,ACAN=1,CAM=2)
        writer = CANPacker(path)
        if camera:
            cs = SimpleNamespace(mdps=source["mdps"],steer_touch_2af=source["touch"],
                lfa=source["lfa"],lfa_alt=source["lfa_alt"],adrv_0x161=source["adrv_161"])
            result = hyundaicanfd.create_steering_messages_camera_scc(frame,writer,cp,can,
                SimpleNamespace(latActive=active),active,-127.,cs,18.75,167.,angle)
        else:
            result = hyundaicanfd.create_steering_messages(writer,cp,can,enabled,active,-127.,18.75,167.,angle)
        frames = [dict(address=address,data=list(data),bus=bus) for address,data,bus in result]
        cases.append(dict(camera=camera,dbc=path,input=input_values,source=source,expected=frames))
    (fixture / "steering.json").write_text(json.dumps(cases) + "\n")
    path = str(dbcs / "hyundai_kia_generic.dbc")
    definition = DBC(path)
    source = {name:0. for name in definition.name_to_msg["LKAS11"].sigs}
    cases = []
    for candidate, flags, active, warning, ldws, frame in itertools.product(
        (CAR.HYUNDAI_SANTA_FE,CAR.KIA_RAY_EV,CAR.KIA_OPTIMA_G4,CAR.KIA_OPTIMA_G4_FL,CAR.HYUNDAI_GENESIS,CAR.KIA_SORENTO),
        (0,128,65536,131072),(False,True),(False,True),(False,True),(0,16,31)):
        writer = CANPacker(path)
        cp = SimpleNamespace(flags=flags,carFingerprint=candidate)
        result = hyundaican.create_lkas11(writer,frame,cp,-127.,active,False,source,warning,
            3.,active,True,False,2.,0.,ldws)
        address,data,bus = result
        cases.append(dict(candidate=str(candidate),flags=flags,enabled=active,warning=warning,
            ldws=ldws,frame=frame,dbc=path,source=source,expected=dict(address=address,data=list(data),bus=bus)))
    (fixture / "legacy-steering.json").write_text(json.dumps(cases) + "\n")
    print(f"Generated CAN-FD and {len(cases)} legacy steering byte scenarios")


if __name__ == "__main__":
    main()
