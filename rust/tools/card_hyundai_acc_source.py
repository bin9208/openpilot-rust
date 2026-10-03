# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy", "pycapnp"]
# ///
# Run with the existing oracle Python: PYTHONPATH=.:opendbc_repo:rust/tools python rust/tools/card_hyundai_acc_source.py FIXTURE_DIR DBC_DIR
import itertools
import json
from pathlib import Path
import sys
from types import SimpleNamespace as NS

from can_source import load


def main() -> None:
    DBC,CANPacker,_ = load()
    from opendbc.car.hyundai import hyundaicanfd,hyundaican
    from opendbc.car.hyundai.stopping import CanfdStopping
    fixture=Path(sys.argv[1])
    dbcs=Path(sys.argv[2])
    fd_path=str(dbcs / "hyundai_canfd_generated.dbc")
    generic_path=str(dbcs / "hyundai_kia_generic.dbc")
    scc={key:0. for key in DBC(fd_path).name_to_msg["SCC_CONTROL"].sigs}
    scc["COUNTER"]=13.
    fd_cases=[]
    for camera,control,scenario in itertools.product((False,True),(False,True),range(12)):
        writer=CANPacker(fd_path)
        controller=CanfdStopping() if control else None
        previous=target_last=-0.3
        steps=[]
        for tick in range(100):
            speed=0.3 if scenario==0 else 0. if scenario==1 else max(0.,1.-tick*0.02)
            state=dict(original=scc,wheels=[speed]*4,v_ego=speed,v_ego_raw=speed,a_ego=-0.1,
                brake=scenario==2,gas=scenario==3,brake_hold=scenario==4,parking_brake=scenario==5,
                can_valid=scenario!=6,drive=scenario!=7,available=scenario!=8,standstill=speed==0.,
                paddle=1 if scenario==9 else 0,soft_hold=scenario==10,scc_hold=speed==0.)
            input_values=dict(bus=0,enabled=scenario!=11,accel_last=target_last,value_last=previous,
                accel=-0.5 if tick<70 else -1.5,stopping=tick<80,gas_override=scenario==3,
                set_speed=30.,gap=2.,jerk_u=2.,jerk_l=1.,carrot_cruise=0,carrot_accel=0.,
                lead=[204.6,0.,239.4,0.])
            out=NS(vEgo=speed,vEgoRaw=speed,aEgo=state["a_ego"],wheelSpeeds=NS(fl=speed,fr=speed,rl=speed,rr=speed),
                brakePressed=state["brake"],gasPressed=state["gas"],brakeHoldActive=state["brake_hold"],
                parkingBrake=state["parking_brake"],canValid=state["can_valid"],
                gearShifter="drive" if state["drive"] else "reverse",cruiseState=NS(available=state["available"],standstill=state["standstill"]))
            cs=NS(out=out,softHoldActive=int(state["soft_hold"]),paddle_button_prev=state["paddle"],
                canfdSccHoldActive=state["scc_hold"],scc_control=scc)
            can=NS(ECAN=0)
            hud=NS(leadDistanceBars=2)
            if camera:
                frame,value=hyundaicanfd.create_acc_control_scc2(writer,can,input_values["enabled"],previous,
                    input_values["accel"],input_values["stopping"],input_values["gas_override"],30.,hud,
                    NS(carrot_cruise=0,jerk_u=2.,jerk_l=1.),cs,controller)
            else:
                frame,value=hyundaicanfd.create_acc_control(writer,can,input_values["enabled"],target_last,
                    input_values["accel"],input_values["stopping"],input_values["gas_override"],30.,hud,
                    2.,1.,cs,controller,accel_value_last=previous)
            expected=dict(frame=dict(address=frame[0],data=list(frame[1]),bus=frame[2]),value=value,
                phase=str(controller.phase) if controller is not None else None)
            steps.append(dict(input=input_values,state=state,expected=expected))
            previous=value
            target_last=input_values["accel"]
        fd_cases.append(dict(camera=camera,control=control,dbc=fd_path,steps=steps))
    (fixture / "acc.json").write_text(json.dumps(fd_cases)+"\n")
    legacy_cases=[]
    definition=DBC(generic_path)
    source={name:{key:0. for key in definition.name_to_msg[name].sigs} for name in ("SCC11","SCC12","SCC14","FCA11")}
    source["FCA11"].update(FCA_Failinfo=1.,FCA_Status=3.)
    for camera,enabled,hold,brake,override,stopping,use_fca,carrot in itertools.product(
        (False,True),(False,True),(False,True),(False,True),(False,True),(False,True),(False,True),(0,1,2)):
        input_values=dict(enabled=enabled,accel=-0.7,index=16,gap=3.,lead_visible=True,
            lead_distance=23.3,lead_speed=-0.3,set_speed=53.,stopping=stopping,long_override=override,
            available=True,brake_hold=hold,brake_pressed=brake,paddle=0,soft_hold=2,soft_hold_mode=2,
            carrot_cruise=carrot,carrot_accel=-0.2,band_upper=0.8,band_lower=0.7,jerk_u=2.,jerk_l=1.,
            use_fca=use_fca,flags=8 if camera else 0,casper_fca=True)
        cs=NS(out=NS(cruiseState=NS(available=True),brakeHoldActive=hold,brakePressed=brake),
            paddle_button_prev=0,softHoldActive=2,scc11=source["SCC11"],scc12=source["SCC12"],
            scc14=source["SCC14"],fca11=source["FCA11"])
        hud=NS(leadDistanceBars=3.,leadVisible=True,leadDistance=23.3,leadRelSpeed=-0.3)
        jerk=NS(cb_upper=0.8,cb_lower=0.7,jerk_u=2.,jerk_l=1.,carrot_cruise=carrot,carrot_cruise_accel=-0.2)
        writer=CANPacker(generic_path)
        if camera:
            frames=hyundaican.create_acc_commands_scc(writer,enabled,-0.7,jerk,16,hud,53.,stopping,override,True,cs,2)
        else:
            frames=hyundaican.create_acc_commands(writer,enabled,-0.7,jerk,16,hud,53.,stopping,override,use_fca,NS(flags=0),cs,2)
        legacy_cases.append(dict(camera=camera,dbc=generic_path,input=input_values,source=source,
            expected=[dict(address=a,data=list(d),bus=b) for a,d,b in frames]))
    (fixture / "legacy-acc.json").write_text(json.dumps(legacy_cases)+"\n")
    print(f"Generated {len(fd_cases)*100} CAN-FD SCC ticks and {len(legacy_cases)} legacy SCC scenarios")


if __name__=="__main__":
    main()
