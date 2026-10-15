"""Actual final-source causal pixels, complete tracker state and output oracle."""
import argparse,hashlib,json,subprocess,sys
from pathlib import Path
from types import ModuleType
import cv2
import numpy as np
from signal_source import source,compare

def main():
  parser=argparse.ArgumentParser();parser.add_argument('--binary',type=Path);parser.add_argument('--output',type=Path,required=True);parser.add_argument('--input',type=Path,help='Replay identical frozen RGB inputs for a same-architecture source oracle')
  args=parser.parse_args();args.output.mkdir(parents=True,exist_ok=False)
  cv2.setNumThreads(1);cv2.ocl.setUseOpenCL(False)
  module=ModuleType('signal_tracker');sys.modules[module.__name__]=module
  exec(compile(source('tools/signal_analysis/signal_tracker.py'),'e8e152db:signal_tracker','exec'),module.__dict__)
  tests=ModuleType('signal_tracker_source_tests');exec(compile(source('tools/signal_analysis/test_signal_tracker.py'),'e8e152db:test_signal_tracker','exec'),tests.__dict__)
  cases=[];expected=[]
  frozen_input=json.loads(args.input.read_text()) if args.input else None
  if frozen_input is not None:assert len(frozen_input)==12 and all(len(c)==24 for c in frozen_input)
  for scenario in range(12):
    tracker=module.SignalTracker(daytime_cores=True,robust_tracking=True,verified_lamps=True);frames=[];results=[];timestamp=10.
    for index in range(24):
      green=index>=12
      rgb=tests.TestDayProposals.frame('green' if green else 'red') if scenario%2 else tests.TestNightProposals.frame(color=(0,255,0) if green else (255,0,0))
      if scenario>=4:
        # Current scene correspondence includes translation and textured surrounds.
        rng=np.random.default_rng(23);noise=rng.integers(0,30,rgb.shape,np.uint8);rgb=np.maximum(rgb,noise)
        rgb=np.roll(rgb,(index//4)%3,axis=1)
      if scenario in [8,9] and index in [9,10,16]:rgb=np.zeros_like(rgb)
      timestamp+=.3 if scenario in [6,7,10,11] and index in [8,16] else .05
      if scenario==11 and index==20:timestamp-=1.
      allow_green=not(scenario in [2,3,10] and index>=15)
      if frozen_input is None:
        path=args.output/f'{scenario}-{index}.rgb';path.write_bytes(rgb.tobytes())
        frame={'path':str(path.resolve()),'width':rgb.shape[1],'height':rgb.shape[0],'timestamp':timestamp,'allow_green':allow_green}
      else:
        frame=frozen_input[scenario][index]
        rgb=np.fromfile(frame['path'],np.uint8).reshape(frame['height'],frame['width'],3)
        timestamp,allow_green=frame['timestamp'],frame['allow_green']
      frames.append(frame)
      tracks=tracker.process(rgb,timestamp,allow_green=allow_green)
      state={k:v for k,v in vars(tracker).items() if k not in ['tracks','previous_gray']};state['previous_gray']={'width':rgb.shape[1],'height':rgb.shape[0],'sha256':hashlib.sha256(tracker.previous_gray.tobytes()).hexdigest()};state['tracks']=[vars(t).copy() for t in tracker.tracks]
      # Freeze mutable nested dictionaries before the next current image.
      results.append(json.loads(json.dumps({'tracks':tracks,'state':state})))
    cases.append(frames);expected.append(results)
  (args.output/'input.json').write_text(json.dumps(cases));(args.output/'expected.json').write_text(json.dumps(expected))
  if args.binary:
    raw=subprocess.check_output([str(args.binary.resolve())],input=json.dumps(cases).encode());(args.output/'actual.json').write_bytes(raw)
    (args.output/'receipt.json').write_text(json.dumps(compare(json.loads(raw),expected))+'\n')
  print(json.dumps({'scenarios':len(cases),'frames':sum(map(len,cases)),'native_compared':bool(args.binary)}))
if __name__=='__main__':main()
