from __future__ import annotations

import itertools
import math
import random
from can_source import load


def cases():
    load()
    from opendbc.can import CANPacker
    from openpilot.cereal import car
    result=[]
    versions={'TESLA_MODEL_3':b'TeMYG4_Main_0.0.0 (77),E4HP015.04.5','TESLA_MODEL_Y':b'TeMYG4_Main_0.0.0 (77),Y4003.05.4'}
    for candidate,alpha,vehicle,das,radar,fsd,disable in itertools.product(versions,(False,True),(False,True),(False,True),(False,True),(False,True),(False,True)):
        fingerprints=[(1,[(0x3df,8)] if vehicle else []),(2,[(0x293,8)] if das else [])]
        if radar:fingerprints[0][1].append((0x410,8))
        firmware=[dict(ecu='eps',fw_version=list(versions[candidate]))] if fsd else []
        result.append(dict(name=f'params-{candidate}-{alpha}-{vehicle}-{das}-{radar}-{fsd}-{disable}',op='params',candidate=candidate,alpha_long=alpha,fingerprints=fingerprints,firmware=firmware,settings={'DisableMinSteerSpeed':str(int(disable)),'NNFF':'1'},now=2_000_000_000,steps=[]))
    result.append(dict(name='model-x-missing-torque',op='params',candidate='TESLA_MODEL_X',alpha_long=True,fingerprints=[],firmware=[],settings={},now=2_000_000_000,steps=[]))
    party=CANPacker('tesla_model3_party');vehicle_packer=CANPacker('tesla_model3_vehicle');rng=random.Random(177)
    def packed(name,bus,values,packer=party):
        address,data,source=packer.make_can_msg(name,bus,values)
        return dict(address=address,data=list(data),bus=source)
    for profile in range(8):
        candidate='TESLA_MODEL_3' if profile%2==0 else 'TESLA_MODEL_Y';long=profile%4<2;vehicle=profile%4!=3;fsd=profile%4==1;das=profile!=6;mph=profile>=4
        steps=[]
        for index in range(620):
            now=2_000_000_000+index*10_000_000
            raw_speed=0. if index<5 else 72.+math.sin(index/35.)*30.
            cruise_speed=60. if index<90 else 61. if index<170 else 50. if index<260 else 0. if index<280 else 65.
            acc_state=4 if index%100<70 else (0,1,2,12,13,14,15)[(index//100)%7]
            epas_status=2 if index%151<110 else 0 if index%151<130 else 3
            hands=3 if index%171>150 else 0
            error=9 if index%151>=120 else 0
            steering=math.sin(index/30.)*50.
            torque=(0.,.6,1.2,2.6,-.6,-2.6)[(index//20)%6]
            cruise=2 if index%210<180 else 1 if index%210<195 else 5
            autopark=3 if 290<=index<310 else 4 if 310<=index<315 else 0
            frames=[
                packed('DI_speed',0,dict(DI_vehicleSpeed=raw_speed,DI_uiSpeed=raw_speed/(1.609344 if mph else 1),DI_uiSpeedUnits=int(not mph))),
                packed('ESP_wheelSpeeds',0,{key:raw_speed+offset for key,offset in zip(('ESP_wheelSpeedFrL','ESP_wheelSpeedFrR','ESP_wheelSpeedReL','ESP_wheelSpeedReR'),(0,.1,.2,.3))}),
                packed('DI_systemStatus',0,dict(DI_accelPedalPos=(0.,.4,.8,1.,.6,.4)[index%6],DI_regenLight=index%3,DI_gear=(4,1,2,3,0,7)[(index//70)%6])),
                packed('DI_torque',0,dict(DI_axleSpeed=index*10)),
                packed('ESP_status',0,dict(ESP_driverBrakeApply=2 if 350<=index<365 else 0,ESP_brakeLamp=index%2,ESP_espFaultLamp=index%2,ESP_espModeActive=index%2)),
                packed('EPAS3S_sysStatus',0,dict(EPAS3S_handsOnLevel=hands,EPAS3S_internalSAS=-steering,EPAS3S_torsionBarTorque=-torque,EPAS3S_steeringRackForce=100,EPAS3S_eacStatus=epas_status,EPAS3S_eacErrorCode=error)),
                packed('SCCM_steeringAngleSensor',2,dict(SCCM_steeringAngleSpeed=-math.cos(index/30.)*20)),
                packed('DI_state',0,dict(DI_cruiseState=cruise,DI_speedUnits=int(not mph),DI_digitalSpeed=cruise_speed,DI_autoparkState=autopark,DI_parkBrakeState=3 if index%30==0 else 1,DI_vehicleHoldState=3 if index%31==0 else 1)),
                packed('DAS_control',2,dict(DAS_accState=acc_state,DAS_aebEvent=index%2,DAS_controlCounter=index%8)),
                packed('DAS_status',2,dict(DAS_fusedSpeedLimit=70 if index<170 else 50 if index<260 else 65,DAS_blindSpotRearLeft=index%3,DAS_blindSpotRearRight=index%4,DAS_forwardCollisionWarning=index%2)),
                packed('ESP_B',0,dict(ESP_vehicleStandstillSts=int(raw_speed==0))),
                packed('UI_warning',0,dict(anyDoorOpen=index%2,leftBlinkerBlinking=index%3,rightBlinkerBlinking=index%3,highBeam=index%2,buckleStatus=index%2)),
                packed('DAS_settings',2,dict(DAS_autosteerEnabled=1 if 580<=index<590 else 0)),
                packed('DAS_steeringControl',2,dict(DAS_steeringControlType=1 if 10<=index<20 or 510<=index<515 else 3 if 530<=index<535 else 0)),
            ]
            if vehicle:
                frames.extend([packed('UI_status2',1,dict(UI_activeTouchPoints=(0,3,2,1,0)[(index//7)%5]),vehicle_packer),packed('VCSEC_TPMSDisplay',1,{key:((index+offset)%256)*.025 for key,offset in zip(('VCSEC_TPMSDisplayPressureFL','VCSEC_TPMSDisplayPressureFR','VCSEC_TPMSDisplayPressureRL','VCSEC_TPMSDisplayPressureRR'),(0,60,120,180))},vehicle_packer)])
                template=[0xA5,0x11,0x22,0xC0,0x44,0x55,0x66,0x77]
                if index in (120,400):template[3]|=1
                elif index in (122,402):template[3]|=63
                elif index in (121,401):template[3]|=2
                if index in (123,403):template[0]=0xA7
                frames.append(dict(address=0x3c2,data=template,bus=1))
            if 420<=index<480:frames=[f for f in frames if f['address']!=party.dbc.name_to_msg['DAS_status'].address]
            if index%79==78:frames[0]['data'][0]^=1
            control=car.CarControl.new_message(enabled=index%220<200,latActive=index%150<140,longActive=index%220<180,cruiseControl=dict(cancel=index%100==85),actuators=dict(steeringAngleDeg=rng.uniform(-200,200),accel=rng.uniform(-5,3),torque=.5))
            packets=[] if index%67==66 else [dict(mono_time=now,frames=frames)]
            if profile==6 and 330<=index<390:packets=[dict(mono_time=now,frames=[])]
            steps.append(dict(now=now,packets=packets,control=list(control.to_bytes()),soft_hold=index%4,commit={'vCruise':70.,'activateCruise':index%2}))
        fingerprints=[(1,[(0x3df,8)] if vehicle else []),(2,[(0x293,8)] if das else [])]
        firmware=[dict(ecu='eps',fw_version=list(versions[candidate]))] if fsd else []
        result.append(dict(name=f'runtime-{profile}',op='runtime',candidate=candidate,alpha_long=long,fingerprints=fingerprints,firmware=firmware,settings={},now=2_000_000_000,steps=steps))
    return result
