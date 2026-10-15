"""Actual final owner signal methods, including post-coasting order and retained state."""
import argparse
import ast
import json
from pathlib import Path
import subprocess
from types import SimpleNamespace as NS
from signal_source import classes,compare
from planner_source import source


def main():
  parser=argparse.ArgumentParser();parser.add_argument('--binary',type=Path);parser.add_argument('--output',type=Path,required=True)
  args=parser.parse_args();args.output.mkdir(parents=True,exist_ok=False)
  helper=classes('openpilot/selfdrive/carrot/signal_stop_prepare.py',['SignalStopPrepare','PrepareAccel'])
  owner=next(n for n in ast.parse(source('openpilot/selfdrive/controls/lib/longitudinal_planner.py')).body if isinstance(n,ast.ClassDef) and n.name=='LongitudinalPlanner')
  methods=[n for n in owner.body if isinstance(n,ast.FunctionDef) and n.name in ['apply_signal_stop_prepare','apply_signal_start_guard']]
  scope={};exec(compile(ast.fix_missing_locations(ast.Module(body=[ast.ClassDef(name='Owner',bases=[],keywords=[],body=methods,decorator_list=[])],type_ignores=[])),'e8e152db:signal_longitudinal','exec'),scope)
  cases=[];expected=[]
  for variant in range(32):
    native=scope['Owner']();native._signal_prepare_time=None;native._signal_prepare_previous=None;native._signal_prepare_accel=helper['PrepareAccel']()
    frames=[];outputs=[]
    for index in range(120):
      now=20.+index*.05
      if variant==16 and index==35:now-=.5
      if variant==17 and index>=35:now+=.3
      context={'now':now,'dt':.05,'prepare_active':index>5,'control_active':variant!=1,'hold':index>80 and variant%2==0,
        'reset':variant==2 and index>35,'gas':variant==3 and index>35,'brake':variant==4 and index>35,'acc':variant!=5,
        'lead_one':variant==6 and index>35,'lead_two':variant==7 and index>35,'ego_speed':10.,'ego_accel':.1}
      output={'should_stop':variant==8 and index>35,'speed':11.,'accel':[-2.,-.25,0.,.3][variant%4],'jerk':[-1.,0.,1.][variant%3],
              'a_desired':[-2.,0.,.5][variant%3],'coasting':10.}
      native.dt=context['dt'];native.mpc=NS(mode='acc' if context['acc'] else 'blended')
      native.output_should_stop=output['should_stop'];native.output_v_target_now=output['speed'];native.output_a_target=output['accel']
      native.output_j_target_now=output['jerk'];native.a_desired=output['a_desired'];native.coasting_target=output['coasting']
      sm={'carState':NS(gasPressed=context['gas'],brakePressed=context['brake'],vEgo=context['ego_speed'],aEgo=context['ego_accel']),
          'radarState':NS(leadOne=NS(status=context['lead_one']),leadTwo=NS(status=context['lead_two']))}
      carrot=NS(prepare_decision=NS(active=context['prepare_active']),prepare_control_active=context['control_active'],start_decision=NS(hold=context['hold']))
      native.apply_signal_stop_prepare(sm,carrot,context['reset'],now=now);native.apply_signal_start_guard(carrot)
      outputs.append({'output':{'should_stop':native.output_should_stop,'speed':native.output_v_target_now,'accel':native.output_a_target,
        'jerk':native.output_j_target_now,'a_desired':native.a_desired,'coasting':native.coasting_target},
        'state':{'time':native._signal_prepare_time,'previous':native._signal_prepare_previous,'cap':native._signal_prepare_accel.cap}})
      frames.append({'input':context,'output':output})
    cases.append(frames);expected.append(outputs)
  (args.output/'input.json').write_text(json.dumps(cases)+'\n');(args.output/'expected.json').write_text(json.dumps(expected)+'\n')
  if args.binary:
    raw=subprocess.check_output([str(args.binary.resolve())],input=json.dumps(cases).encode());(args.output/'actual.json').write_bytes(raw)
    (args.output/'receipt.json').write_text(json.dumps(compare(json.loads(raw),expected))+'\n')
  print(json.dumps({'cases':len(cases),'frames':sum(map(len,cases)),'native_compared':bool(args.binary)}))
if __name__=='__main__':main()
