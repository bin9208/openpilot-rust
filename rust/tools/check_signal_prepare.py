"""Active final preparation state oracle with explicit geometric callback results."""
import argparse
import dataclasses
import itertools
import json
import math
from pathlib import Path
import subprocess
from signal_source import classes, compare


def corpus():
  cases=[]
  for variant in range(64):
    frames=[]
    for frame in range(120):
      now=10.+frame*.05;stamp=now-(.1 if variant%2 else 0.)
      night=24<=variant<48 or variant>=56
      boxes=[[620.,200.,660.,220. if night else 210.]]
      if night or variant%8==1:boxes.append([760.,200.,800.,220. if night else 210.])
      tracks=[]
      for i,b in enumerate(boxes):
        tracks.append({'id':i+1,'box':b,'age':0.,'observations':frame+3,'state':'red',
                       'evidence':{'raw':'red','source':'night_lamp_core' if night else 'detected',
                                   'quality':.9,'red_score':.8,'green_score':0.,'core_diameter':10.}})
      if variant%8==2:tracks.append({'id':3,'box':[1000.,200.,1040.,210.],'age':0.,'observations':frame+3,'state':'green',
                                     'evidence':{'raw':'green','source':'detected','quality':.9}})
      if variant%8==3 and frame>30:tracks=[]
      if variant%8==4 and frame>30:tracks[0]['id']=4
      if variant%8==5 and frame>30:
        for t in tracks:t['evidence']['raw']='green';t['state']='green'
      packet_frame=frame
      if variant>=48:
        cadence=[2,3,4,5][variant%4]
        packet_frame=frame//cadence
        stamp=10.+packet_frame*cadence*.05
        now+=.15 if variant%2 else .05
      observation={'version':1,'stream':'road','size':[1344,760],'timestamp':stamp,'frame_id':packet_frame,
                   'session':'camera' if variant%8!=6 or frame<30 else 'restart','tracks':tracks}
      if variant%8==7 and frame>30:observation['timestamp']=11.
      frames.append({'now':now,'observation':observation,'enabled':True,'valid':variant!=20 or frame<30,
                     'drive':variant!=21 or frame<30,'speed':10.,'gas':variant==22 and frame==30,
                     'brake':variant==23 and frame==30,'lead':variant==18 and frame>30,
                     'turning':variant==19 and frame>30,'model_stopping':False,'entry_allowed':True,
                     'night':night,'range':[45.,85.],'red_alignment':[-.5,.5],'green_alignment':[2.,2.5]})
    cases.append(frames)
  return cases


def main():
  parser=argparse.ArgumentParser();parser.add_argument('--binary',type=Path);parser.add_argument('--output',type=Path,required=True)
  args=parser.parse_args();args.output.mkdir(parents=True,exist_ok=False)
  scope=classes('openpilot/selfdrive/carrot/signal_stop_prepare.py',['PrepareDecision','SignalStopPrepare','central_red_priority','PrepareAccel'])
  scope['combinations']=itertools.combinations
  cases=corpus();expected=[]
  for frames in cases:
    prepare=scope['SignalStopPrepare']();outputs=[]
    for f in frames:
      c=dict(f);night=c.pop('night');distance=c.pop('range');red=c.pop('red_alignment');green=c.pop('green_alignment')
      now=c.pop('now');obs=c.pop('observation')
      geometry=lambda box:tuple(distance)
      alignment=lambda box,distance:tuple(green if box[0]>900 else red)
      decision=dataclasses.asdict(prepare.update(now,obs,**c,geometry=geometry,alignment=alignment,night_geometry=geometry if night else None))
      def sanitize(v):
        if isinstance(v,float) and not math.isfinite(v):return None
        if isinstance(v,dict):return {str(k):sanitize(x) for k,x in v.items()}
        if isinstance(v,(list,tuple)):return [sanitize(x) for x in v]
        return v
      outputs.append({'decision':decision,'state':sanitize(vars(prepare))})
    expected.append(outputs)
  (args.output/'input.json').write_text(json.dumps(cases)+'\n');(args.output/'expected.json').write_text(json.dumps(expected)+'\n')
  if args.binary:
    actual=subprocess.check_output([str(args.binary.resolve())],input=json.dumps(cases).encode());(args.output/'actual.json').write_bytes(actual)
    (args.output/'receipt.json').write_text(json.dumps(compare(json.loads(actual),json.loads(json.dumps(expected))))+'\n')
  print(json.dumps({'cases':len(cases),'frames':sum(map(len,cases)),'active_frames':sum(d['decision']['active'] for case in expected for d in case),'native_compared':bool(args.binary)}))
if __name__=='__main__':main()
