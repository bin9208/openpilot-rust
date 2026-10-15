"""Actual final tracker transitions; image detections are explicit test inputs."""
import argparse
import dataclasses
import json
from pathlib import Path
import subprocess
from signal_source import classes, compare


def corpus():
  cases=[]
  for variant in range(32):
    frames=[]
    for frame in range(120):
      timestamp=frame*([.05,.1,.2,.3][variant%4])+10
      raw='red' if frame<35 else 'green' if frame<75 else 'unknown'
      x=600.+(frame%5 if variant%3==0 else 0.)
      box=[x,120.,x+80.,160.]
      detection={'box':box,'raw':raw,'source':'detected','quality':.9}
      if variant&4:detection.update(continuity_id=1,match_score=.99,red_motion_verified=True)
      if variant&8:detection['source']='causal_template'
      detections=[] if variant&16 and frame%7==0 else [detection]
      if variant==31:detections+=[dict(detection,box=[x+3,120.,x+83.,160.])]
      frames.append({'timestamp':timestamp,'detections':detections})
    cases.append(frames)
  return cases


def main():
  parser=argparse.ArgumentParser();parser.add_argument('--binary',type=Path);parser.add_argument('--output',type=Path,required=True)
  args=parser.parse_args();args.output.mkdir(parents=True,exist_ok=False)
  scope=classes('tools/signal_analysis/signal_tracker.py',['overlap','Track','SignalTracker'])
  cases=corpus();expected=[]
  for frames in cases:
    tracker=scope['SignalTracker'](daytime_cores=True,robust_tracking=True,verified_lamps=True)
    outputs=[]
    for f in frames:
      output=tracker.update(f['timestamp'],f['detections'])
      state=dict(vars(tracker));state['tracks']=[dataclasses.asdict(t) for t in tracker.tracks]
      outputs.append({'output':output,'state':state})
    expected.append(outputs)
  (args.output/'input.json').write_text(json.dumps(cases)+'\n');(args.output/'expected.json').write_text(json.dumps(expected)+'\n')
  if args.binary:
    actual=subprocess.check_output([str(args.binary.resolve())],input=json.dumps(cases).encode());(args.output/'actual.json').write_bytes(actual)
    (args.output/'receipt.json').write_text(json.dumps(compare(json.loads(actual),expected))+'\n')
  print(json.dumps({'cases':len(cases),'frames':sum(map(len,cases)),'native_compared':bool(args.binary)}))
if __name__=='__main__':main()
