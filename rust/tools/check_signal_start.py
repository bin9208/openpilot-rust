"""Active source-pinned stopped signal guard corpus and native comparison."""
import argparse
import dataclasses
import json
import math
from pathlib import Path
import subprocess
from signal_source import classes, compare


def corpus():
  cases = []
  box = [620., 120., 700., 160.]
  def track(raw, id=1, box=box, seen=True):
    return {'raw': raw, 'box': box, 'id': id, 'seen_red': seen}
  for variant in range(33):
    frames = []
    for index in range(90):
      now = 20. + index*.1
      raw = 'red' if index < 15 or index >= 75 else 'green'
      tracks = [track(raw)]
      if variant == 1 and index >= 15: tracks = [track(raw, id=2, box=[850., 120., 930., 160.])]
      if variant == 2 and index >= 15: tracks += [track('red', id=2, box=[850., 120., 930., 160.])]
      if variant == 3 and index >= 15: tracks = []
      if variant == 4: tracks += [track(raw, id=2, box=[780., 120., 860., 160.])]
      if variant == 5: tracks = [track(raw, box=[300.,120.,380.,160.]), track(raw,id=2,box=[850.,120.,930.,160.])]
      if variant == 6 and index >= 15: tracks += [track('red',id=2,box=[850.,240.,880.,250.])]
      if variant == 7: tracks = [track(raw,seen=False)]
      observation={'timestamp':now, 'frame_id':index, 'session':'a' if variant != 8 or index < 17 else 'restart', 'tracks':tracks}
      if variant == 9 and index >= 15: observation['timestamp']-=.66
      if variant == 10 and index >= 15: observation['timestamp']+=.01
      if variant == 11 and index >= 15: observation['frame_id']=14
      if variant == 12 and index >= 15: observation['timestamp']=21.4
      if variant == 13 and index >= 15: observation=None
      if index >= 18 and index < 30:
        if variant == 24: observation = {}
        if variant == 25: observation = []
        if variant == 26: observation['tracks'] = None
        if variant == 27: observation['tracks'][0].pop('raw')
        if variant == 28: observation['tracks'][0]['box'] = [0]
        if variant == 29: observation['session'] = ''
        if variant == 30: observation['frame_id'] = True
        if variant == 31: observation['tracks'][0]['id'] = True
        if variant == 32: observation['timestamp'] = True
      frames.append({'now':now if variant != 14 or index != 18 else now-2., 'observation':observation,
                     'enabled':variant != 15 or index < 18,'valid':variant != 16 or index < 18,
                     'drive':variant != 17 or index < 18,'gas':variant == 18 and index == 18,
                     'speed':1. if variant == 19 or (variant == 20 and index >= 18) else .2,
                     'departure':variant != 21})
    cases.append(frames)
  return cases


def main():
  parser=argparse.ArgumentParser()
  parser.add_argument('--binary',type=Path)
  parser.add_argument('--output',type=Path,required=True)
  args=parser.parse_args();args.output.mkdir(parents=True,exist_ok=False)
  guard=classes('openpilot/selfdrive/carrot/signal_start_guard.py',['SignalStartGuard','StartDecision','observation_color'])
  cases=corpus();expected=[]
  for frames in cases:
    native=guard['SignalStartGuard']();values=[]
    for frame in frames:
      context=dict(frame);now=context.pop('now');obs=context.pop('observation')
      decision=dataclasses.asdict(native.update(now,obs,**context))
      color=guard['observation_color'](obs,now,**{k:context[k] for k in ['enabled','valid','drive']})
      def sanitize(v):
        if isinstance(v,float) and not math.isfinite(v):return None
        if isinstance(v,dict):return {k:sanitize(x) for k,x in v.items()}
        if isinstance(v,list):return [sanitize(x) for x in v]
        return v
      values.append({'decision':decision,'color':color,'state':sanitize(vars(native))})
    expected.append(values)
  (args.output/'input.json').write_text(json.dumps(cases)+'\n')
  (args.output/'expected.json').write_text(json.dumps(expected)+'\n')
  if args.binary:
    actual=subprocess.check_output([str(args.binary.resolve())],input=json.dumps(cases).encode())
    (args.output/'actual.json').write_bytes(actual)
    (args.output/'receipt.json').write_text(json.dumps(compare(json.loads(actual),expected))+'\n')
  print(json.dumps({'source_commit':'e8e152dbcfd9574a1791ed12026d1880e87689b3','cases':len(cases),'frames':sum(map(len,cases)),'native_compared':bool(args.binary)}))

if __name__=='__main__': main()
