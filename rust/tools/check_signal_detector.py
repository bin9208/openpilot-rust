"""Actual source frame constructors and full current-image detector comparisons."""
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
  module=ModuleType('signal_tracker');sys.modules[module.__name__]=module
  exec(compile(source('tools/signal_analysis/signal_tracker.py'),'e8e152db:signal_tracker','exec'),module.__dict__)
  tests=ModuleType('signal_tracker_source_tests');exec(compile(source('tools/signal_analysis/test_signal_tracker.py'),'e8e152db:test_signal_tracker','exec'),tests.__dict__)
  night=tests.TestNightProposals.frame;day=tests.TestDayProposals.frame
  images=[night(),night(reflected=True),night(color=(0,0,0)),night(background=100),night(color=(0,255,0)),night(color=(0,255,255)),
          day(),day('green'),day(housing=False),day('green',housing=False)]
  cases=[];expected=[]
  for index,rgb in enumerate(images):
    path=args.output/f'{index}.rgb';path.write_bytes(rgb.tobytes());conservative=index%2==0
    cases.append({'path':str(path.resolve()),'width':rgb.shape[1],'height':rgb.shape[0],'conservative_green':conservative})
    gray=cv2.cvtColor(rgb,cv2.COLOR_RGB2GRAY);dark=bool(module.dark_scene(rgb,gray=gray));kw={'gray':gray,'conservative_green':conservative}
    expected.append({'dark':dark,'housing':module.housing_proposals(rgb,**kw),'forward_housing':module.housing_proposals(rgb,forward_only=True,**kw),
      'day':module.day_proposals(rgb,**kw),'night':module.night_proposals(rgb,is_dark=dark,**kw),
      'detect_default':module.detect(rgb,**kw),'detect_day':module.detect(rgb,daytime_cores=True,**kw)})
  (args.output/'input.json').write_text(json.dumps(cases)+'\n');(args.output/'expected.json').write_text(json.dumps(expected)+'\n')
  if args.binary:
    raw=subprocess.check_output([str(args.binary.resolve())],input=json.dumps(cases).encode());(args.output/'actual.json').write_bytes(raw)
    (args.output/'receipt.json').write_text(json.dumps(compare(json.loads(raw),expected))+'\n')
  print(json.dumps({'frames':len(cases),'native_compared':bool(args.binary),'detected':sum(len(f['detect_day']) for f in expected)}))
if __name__=='__main__':main()
