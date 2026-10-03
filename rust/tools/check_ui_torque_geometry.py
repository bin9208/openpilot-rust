import argparse
import json
from pathlib import Path
import random
import subprocess
import sys

sys.path.insert(0,str(Path(__file__).resolve().parent/'ui_application_qa'))
from torque_geometry_source import render

parser=argparse.ArgumentParser()
parser.add_argument('--binary',type=Path,required=True)
parser.add_argument('--output',type=Path,required=True)
args=parser.parse_args()
args.output.mkdir(parents=True,exist_ok=True)
rng=random.Random(14810)
steps=[]
for i in range(1200):
  a={'cx':rng.uniform(-100,700),'cy':rng.uniform(1000,1600),'radius':rng.uniform(1199,1228),
     'thickness':rng.choice([0,1,14,20,56]),'start':rng.uniform(-97,-90),'end':rng.uniform(-90,-83)}
  if i%3==0:
    a['end']=a['start']
  if i%5==0:
    a['end'],a['start']=a['start'],a['end']
  steps.append({'arc':a})
  if i%12==0:
    steps.append({'arc':{**a,'cx':a['cx']+0.001,'start':a['start']+0.00001}})
anchor={'cx':268.5,'cy':1438.5,'radius':1207.5,'thickness':14.5,'start':-96.05,'end':-83.95}
steps.append({'reset':True,'arc':anchor})
for i in range(300):
  steps.append({'arc':{**anchor,'cx':1000+i}})
steps.append({'arc':{**anchor,'cy':anchor['cy']+0.01}})
steps.append({'arc':anchor})
(args.output/'input.json').write_text(json.dumps(steps))
source=render(steps)
result=subprocess.run([str(args.binary)],input=json.dumps(steps),text=True,capture_output=True,check=True)
(args.output/'native.stderr').write_text(result.stderr or '(no stderr)\n')
native=json.loads(result.stdout)
(args.output/'source.json').write_text(json.dumps(source))
(args.output/'native.json').write_text(result.stdout)
assert len(source)==len(native)
differences=[]
for i,(a,b) in enumerate(zip(source,native,strict=True)):
  if a!=b:
    differences.append({'step':i,'source':a,'native':b})
(args.output/'result.json').write_text(json.dumps({'cases':len(steps),'exact_f32_points':not differences,'differences':differences},indent=2))
assert not differences, differences[:1]
print(f'PASS {len(steps)} original/native torque arcs, exact float32 points with quantized hits, ties and LRU eviction')
