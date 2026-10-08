import argparse
import json
from pathlib import Path
import subprocess
import sys
import numpy as np
sys.path.insert(0,str(Path(__file__).resolve().parent/'ui_application_qa'))
from driver_geometry_source import render

parser = argparse.ArgumentParser()
parser.add_argument('--binary',type=Path,required=True)
parser.add_argument('--output',type=Path,required=True)
args = parser.parse_args()
args.output.mkdir(parents=True,exist_ok=True)
steps = []
for offset in [0.0,12.3,-14.9]:
  for orientation in [[0,0,0],[0.01,0.2,-0.3],[-0.9,-0.6,0.4],[0.4,-0.5,-0.6]]:
    for frame in range(50):
      steps.append({'reset':frame==0,'orientation':[float(np.float32(v)) for v in orientation], 'active':frame<25,
                    'rhd':frame>10,'rect':{'x':offset,'y':offset*0.5,'width':2160-abs(offset),'height':1080-abs(offset)}})
(args.output/'input.json').write_text(json.dumps(steps))
source = render(steps)
result = subprocess.run([str(args.binary)],input=json.dumps(steps),capture_output=True,text=True,check=True)
(args.output/'native.stderr').write_text(result.stderr or '(no stderr)\n')
native = json.loads(result.stdout)
(args.output/'source.json').write_text(json.dumps(source))
(args.output/'native.json').write_text(result.stdout)
maximum = 0.0
for index,(a,b) in enumerate(zip(source,native,strict=True)):
  for key in ['fade','pose','difference','sins','coss','face','transformed','center']:
    error = float(np.max(np.abs(np.asarray(a[key])-np.asarray(b[key]))))
    maximum = max(maximum,error)
    assert error<=1e-11,(index,key,error)
  assert a['lines']==b['lines'],(index,'lines',a['lines'],b['lines'])
  for key in ['horizontal','vertical']:
    assert (a[key] is None)==(b[key] is None),(index,key)
    if a[key] is not None:
      assert abs(a[key]['thickness']-b[key]['thickness'])<=1e-11,(index,key)
      assert a[key]['points']==b[key]['points'],(index,key,a[key]['points'],b[key]['points'])
(args.output/'result.json').write_text(json.dumps({'cases':len(steps),'max_f64_error':maximum,'exact_f32_face_and_arc_points':True}))
print(f'PASS: {len(steps)} source/native face poses, fades and arcs; double error <=1e-11; exact float32 drawing points')
