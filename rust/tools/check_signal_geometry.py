"""Final signal road geometry oracle, including invalid and boundary inputs."""
import argparse
import ast
import itertools
import json
import math
from pathlib import Path
import subprocess
import sys
from types import ModuleType,SimpleNamespace
import numpy as np
from signal_source import source, compare


def load():
  orientation=ModuleType('openpilot.common.transformations.orientation')
  nodes=[n for n in ast.parse(source('openpilot/common/transformations/transformations.py')).body if isinstance(n,ast.FunctionDef) and n.name=='euler2rot_single']
  scope={'np':np};exec(compile(ast.Module(body=nodes,type_ignores=[]),'e8e152db:rotation','exec'),scope)
  orientation.rot_from_euler=scope['euler2rot_single'];sys.modules[orientation.__name__]=orientation
  nodes=[n for n in ast.parse(source('openpilot/selfdrive/carrot/signal_stop_prepare.py')).body if isinstance(n,ast.FunctionDef) and n.name in ['road_corridor','housing_range','housing_alignment','night_lamp_support']]
  scope={'np':np,'math':math,'LANE_SIGNAL_MARGIN':3.5};exec(compile(ast.Module(body=nodes,type_ignores=[]),'e8e152db:signal_geometry','exec'),scope)
  return scope


def corpus():
  cases=[]
  def curve(side):return {'x':[0.,25.,50.,100.,150.,200.],'y':[side]*6,'z':[0.]*6}
  for x,w,y,rpy,kind in itertools.product([500.,620.,700.,900.],[24.,40.,80.],[210.,250.,300.],[[0.,0.,0.],[0.,.025,0.],[.02,.025,.05],[-.02,-.01,-.05]],[0,1,2,3]):
    g={'focal':910.,'rpy':rpy,'edges':[curve(-8.),curve(8.)],'edge_stds':[.1,.2],
       'lanes':[curve(-5.4),curve(-1.8),curve(1.8),curve(5.4)],'lane_probs':[1.]*4,'lane_stds':[.1]*4}
    if kind==1:g['lane_probs'][1]=.79
    if kind==2:g['edge_stds']=[.6,.7]
    if kind==3:g['lanes'][2]['x'][2]=25.
    cases.append({'geometry':g,'box':[x,y,x+w,y+w/4.],'distance':[30.,100.]})
  return cases


def main():
  parser=argparse.ArgumentParser();parser.add_argument('--binary',type=Path);parser.add_argument('--output',type=Path,required=True)
  args=parser.parse_args();args.output.mkdir(parents=True,exist_ok=False);scope=load();cases=corpus();expected=[]
  for c in cases:
    g=c['geometry'];args_=[g['focal'],g['rpy'],[SimpleNamespace(**v) for v in g['edges']],g['edge_stds'],[SimpleNamespace(**v) for v in g['lanes']],g['lane_probs'],g['lane_stds']]
    expected.append({'range':scope['housing_range'](c['box'],*args_),'night':scope['night_lamp_support'](c['box'],*args_),
                     'alignment':scope['housing_alignment'](c['box'],c['distance'],*args_)})
  (args.output/'input.json').write_text(json.dumps(cases)+'\n');(args.output/'expected.json').write_text(json.dumps(expected)+'\n')
  if args.binary:
    actual=subprocess.check_output([str(args.binary.resolve())],input=json.dumps(cases).encode());(args.output/'actual.json').write_bytes(actual)
    decoded=json.loads(actual)
    receipt={key:compare([v[key] for v in decoded],[v[key] for v in expected]) for key in ['range','night','alignment']}
    (args.output/'receipt.json').write_text(json.dumps(receipt)+'\n')
  print(json.dumps({'cases':len(cases),'native_compared':bool(args.binary)}))
if __name__=='__main__':main()
