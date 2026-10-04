import argparse
import json
from pathlib import Path
import random
import subprocess
import sys

sys.path.insert(0,str(Path(__file__).resolve().parent/'ui_application_qa'))
from plot_samples_source import render


def main():
  parser=argparse.ArgumentParser()
  parser.add_argument('--binary',type=Path,required=True)
  parser.add_argument('--output',type=Path,required=True)
  args=parser.parse_args()
  args.output.mkdir(parents=True,exist_ok=True)
  rng=random.Random(14813)
  steps=[]
  now=0.0
  for index in range(5000):
    reset=index in [0,1300,1800,4500]
    if reset:
      now=rng.choice([-10.0,0.0,100.0])
    else:
      now+=rng.choice([0.0,0.049999999999,0.05,0.050000000001,0.1,0.25,0.5,0.001,-0.02])
    values=[rng.uniform(-30,30) for _ in range(3)]
    if index%13==0:
      values=[0.0]*3
    steps.append({'now':now,'values':values,'reset':reset,'history':index%97==0 or index==4999})
  (args.output/'input.json').write_text(json.dumps(steps))
  source=render(steps)
  native_result=subprocess.run([str(args.binary)],input=json.dumps(steps),text=True,capture_output=True,check=True)
  native=json.loads(native_result.stdout)
  (args.output/'source.json').write_text(json.dumps(source))
  (args.output/'native.json').write_text(native_result.stdout)
  (args.output/'native.stderr').write_text(native_result.stderr or '(no stderr)\n')
  differences=[{'step':index,'source':a,'native':b} for index,(a,b) in enumerate(zip(source,native,strict=True)) if a!=b]
  (args.output/'result.json').write_text(json.dumps({'cases':len(steps),'history_cases':sum(step['history'] for step in steps),
                                                  'differences':differences},indent=2))
  assert not differences,differences[:1]
  print(f'PASS {len(steps)} original/native plot samples, timing and bounds; full histories at {sum(step["history"] for step in steps)} boundaries')


if __name__=='__main__':
  main()
