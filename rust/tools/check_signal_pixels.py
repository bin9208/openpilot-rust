"""Actual source current-lamp evidence and dark-scene pixel oracle."""
import argparse
import json
from pathlib import Path
import subprocess
import sys
from types import ModuleType
import cv2
import numpy as np
from signal_source import source,compare


def main():
  parser=argparse.ArgumentParser();parser.add_argument('--binary',type=Path);parser.add_argument('--output',type=Path,required=True)
  args=parser.parse_args();args.output.mkdir(parents=True,exist_ok=False)
  cv2.setNumThreads(1);cv2.ocl.setUseOpenCL(False)
  module=ModuleType('signal_pixels_source');sys.modules[module.__name__]=module
  exec(compile(source('tools/signal_analysis/signal_tracker.py'),'e8e152db:signal_tracker','exec'),module.__dict__)
  rng=np.random.default_rng(7124);cases=[];expected=[]
  for index in range(64):
    h,w=32,64;rgb=np.full((h,w,3),[20,20,20] if index%2 else [170,190,210],np.uint8)
    box=[8,8,56,24];rgb[8:24,8:56]=10
    radius=1+index%4
    if index%8 in [0,1,4]:cv2.circle(rgb,(14,16),radius,(255,0,0),-1)
    if index%8 in [2,3,4]:cv2.circle(rgb,(50,16),radius,(0,255,30),-1)
    if index%8==5:rgb=rng.integers(0,256,rgb.shape,np.uint8)
    if index%8==6:rgb[9:23,42:56]=[40,140,170]
    if index%8==7:rgb[9:23,42:56]=[0,255,255]
    conservative=index%3==0
    state,strength,evidence=module.lamp_evidence(rgb,box,conservative_green=conservative)
    cases.append({'rgb':{'width':w,'height':h,'data':rgb.ravel().tolist()},'box':box,'conservative_green':conservative})
    expected.append({'dark':bool(module.dark_scene(rgb)),'evidence':{'raw':state,'quality':strength,**evidence},'illuminated_green':bool(module.illuminated_green(rgb,box))})
  (args.output/'input.json').write_text(json.dumps(cases)+'\n');(args.output/'expected.json').write_text(json.dumps(expected)+'\n')
  if args.binary:
    raw=subprocess.check_output([str(args.binary.resolve())],input=json.dumps(cases).encode());(args.output/'actual.json').write_bytes(raw)
    (args.output/'receipt.json').write_text(json.dumps(compare(json.loads(raw),expected))+'\n')
  print(json.dumps({'cases':len(cases),'native_compared':bool(args.binary)}))
if __name__=='__main__':main()
