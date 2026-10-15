#!/usr/bin/env python3
"""Differential startup gate and nominal geometry against pinned final source (#338)."""
import argparse
import copy
import json
import math
from pathlib import Path
import subprocess
from types import SimpleNamespace
from lateral_readiness_source import module


def cases():
    service = dict(seen=True,valid=True,alive=True,checks=True)
    base = dict(reset=True,state=dict(services={key:dict(service) for key in ['boundary','model','live','pose','selfdrive','events','lane']},pose=[True]*3,initializing=False),
                live=dict(ratio=15.,stiffness=1.,offset=.2,roll=.03,valid=True,sensor_valid=True,posenet_valid=True),
                car=dict(can_valid=True,fault_temporary=False,fault_permanent=False,values=[15.,2.,.1,3.]),
                lane=dict(use_lane_lines=True,solution_valid=True,psis=[0.]*17,curvatures=[0.]*17,distances=[0.]*17),desired_curvature=.01,nominal=17.)
    yield copy.deepcopy(base)
    for service_name in base['state']['services']:
        for flag in service:
            value=copy.deepcopy(base);value['state']['services'][service_name][flag]=False;yield value
    for flag in ['valid','sensor_valid','posenet_valid']:
        value=copy.deepcopy(base);value['live'][flag]=False;yield value
    for ratio in [0.,1.,1.00001]:
        value=copy.deepcopy(base);value['live']['ratio']=ratio;yield value
    for stiffness in [-1.,0.,.001]:
        value=copy.deepcopy(base);value['live']['stiffness']=stiffness;yield value
    for flag in ['can_valid','fault_temporary','fault_permanent']:
        value=copy.deepcopy(base);value['car'][flag]=not value['car'][flag];yield value
    for index in range(3):
        value=copy.deepcopy(base);value['state']['pose'][index]=False;yield value
    value=copy.deepcopy(base);value['state']['initializing']=True;yield value
    for key in ['psis','curvatures','distances']:
        for length in [0,16,18]:
            value=copy.deepcopy(base);value['lane'][key]=[0.]*length;yield value
    value=copy.deepcopy(base);value['lane']['solution_valid']=False;yield value
    value=copy.deepcopy(base);value['lane']['use_lane_lines']=False;value['lane']['psis']=[];value['state']['services']['lane']['seen']=False;yield value
    for field in ['live.ratio','live.stiffness','live.offset','live.roll','car.speed','car.angle','car.rate','car.torque','model','lane.psis','lane.curvatures','lane.distances']:
        value=copy.deepcopy(base);value['nonfinite']=field;yield value
    # A valid process latch survives later failures, while a new process does not inherit it.
    yield copy.deepcopy(base)
    value=copy.deepcopy(base);value['reset']=False;value['state']['initializing']=True;value['live']['ratio']=0.;yield value
    value['reset']=True;yield value


def expected(case,gate,source):
    services=case['state']['services']
    names={'boundary':'carState','model':'modelV2','live':'liveParameters','pose':'livePose','selfdrive':'selfdriveState','events':'onroadEvents','lane':'lateralPlan'}
    class Messages(dict):
        seen={names[key]:value['seen'] for key,value in services.items()}
        valid={names[key]:value['valid'] for key,value in services.items()}
        alive={names[key]:value['alive'] for key,value in services.items()}
        def all_checks(self,requested):
            return all(services[next(key for key,name in names.items() if name==service)]['checks'] for service in requested)
    live=case['live'];car=case['car'];plan=case['lane']
    lp=SimpleNamespace(steerRatio=live['ratio'],stiffnessFactor=live['stiffness'],angleOffsetDeg=live['offset'],roll=live['roll'],valid=live['valid'],sensorValid=live['sensor_valid'],posenetValid=live['posenet_valid'])
    cs=SimpleNamespace(canValid=car['can_valid'],steerFaultTemporary=car['fault_temporary'],steerFaultPermanent=car['fault_permanent'],vEgo=car['values'][0],steeringAngleDeg=car['values'][1],steeringRateDeg=car['values'][2],steeringTorque=car['values'][3])
    lane=SimpleNamespace(useLaneLines=plan['use_lane_lines'],mpcSolutionValid=plan['solution_valid'],psis=plan['psis'][:],curvatures=plan['curvatures'][:],distances=plan['distances'][:])
    action=SimpleNamespace(desiredCurvature=case['desired_curvature'])
    if case.get('nonfinite'):
        field=case['nonfinite']
        if field.startswith('live.'):
            setattr(lp,{'ratio':'steerRatio','stiffness':'stiffnessFactor','offset':'angleOffsetDeg','roll':'roll'}[field.split('.')[1]],math.nan)
        elif field.startswith('car.'):
            setattr(cs,{'speed':'vEgo','angle':'steeringAngleDeg','rate':'steeringRateDeg','torque':'steeringTorque'}[field.split('.')[1]],math.nan)
        elif field.startswith('lane.'):
            getattr(lane,field.split('.')[1])[8]=math.nan
        else:action.desiredCurvature=math.nan
    sm=Messages(liveParameters=lp,livePose=SimpleNamespace(**dict(zip(['inputsOK','posenetOK','sensorsOK'],case['state']['pose'],strict=True))),modelV2=SimpleNamespace(action=action),lateralPlan=lane,onroadEvents=[SimpleNamespace(name='selfdriveInitializing')] if case['state']['initializing'] else [])
    parameters=source.lateral_vehicle_parameters(sm,SimpleNamespace(steerRatio=case['nominal']))
    return dict(ready=gate.update(sm,cs,'carState'),live_ready=source.live_parameters_ready(sm),geometry=[parameters.steerRatio,parameters.stiffnessFactor,parameters.angleOffsetDeg,parameters.roll])


def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--binary',required=True,type=Path);args=parser.parse_args()
    requests=list(cases());source=module();gate=source.LateralStartupGate();wanted=[]
    for request in requests:
        if request['reset']:gate=source.LateralStartupGate()
        wanted.append(expected(request,gate,source))
    result=subprocess.run([args.binary.resolve()],input=''.join(json.dumps(request)+'\n' for request in requests),text=True,capture_output=True,check=True)
    observed=[json.loads(line) for line in result.stdout.splitlines()]
    assert observed==wanted,next(((i,a,b) for i,(a,b) in enumerate(zip(observed,wanted,strict=True)) if a!=b), (len(observed),len(wanted)))
    print(f'PASS: {len(requests)} exact-source startup/latch/fallback/service/finite/lane differential cases')

if __name__=='__main__':main()
