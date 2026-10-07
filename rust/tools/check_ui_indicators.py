# /// script
# dependencies = ["numpy", "pillow"]
# ///
# How to run: UI_MSGQ_PYTHON=<owned extension> <ui-venv>/python rust/tools/check_ui_indicators.py --binary <product_render> --output <evidence> --display :125

import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

import numpy as np
from PIL import Image


def step(frame, status='engaged', traffic=0, brake=None, steer=None, value=None, **torque):
  return {'frame':frame,'status':status,'traffic':traffic,'brake':brake or [],'steer':steer or [],'value':value,
          'angle':False,'lat_active':False,'speed':0.0,'curvature':0.0,'desired':0.0,'roll':0.0,'torque':0.0,**torque}


def main() -> None:
  parser=argparse.ArgumentParser()
  parser.add_argument('--binary',type=Path,required=True)
  parser.add_argument('--output',type=Path,required=True)
  parser.add_argument('--display',required=True)
  parser.add_argument('--filter',default='')
  parser.add_argument('--torque',action='store_true')
  parser.add_argument('--rect',type=float,nargs=4,default=[32.3,4.7,486.4,227.3])
  args=parser.parse_args()
  args.output.mkdir(parents=True,exist_ok=True)
  root=Path(__file__).resolve().parents[2]
  cases=[
    ('confidence-model','confidence',False,[step(0,'disengaged'),step(20,brake=[0.05,0.1],steer=[0.02,0.03]),
       step(60,brake=[0.6],steer=[0.2]),step(90,brake=[0.9],steer=[0.9]),step(120,'override',brake=[0.2],steer=[0.1]),step(140,'disengaged')]),
    ('confidence-empty','confidence',False,[step(0),step(30,brake=[0.0],steer=[]),step(60,brake=[],steer=[0.0]),step(100,'override')]),
    ('confidence-demo','confidence',True,[step(0,'disengaged',value=0.95),step(50,'override',value=0.3),step(100,'disengaged',value=0.05)]),
    ('traffic-red-green','traffic',False,[step(0,traffic=1),step(30,traffic=2),step(85,traffic=0),step(110,traffic=1)]),
    ('traffic-zero-clock','traffic',False,[step(0,traffic=2),step(80,traffic=1),step(120,traffic=2)]),
    ('traffic-reentry','traffic',False,[step(0,traffic=0),step(10,traffic=2),step(65,traffic=-7),step(80,traffic=2),step(140,traffic=1)]),
  ]
  if args.torque:
    cases=[
      ('torque-output','torque',False,[step(0,'disengaged'),step(10,torque=-0.95),step(45,torque=1.0),
         step(80,'override',torque=0.3),step(110,'disengaged',torque=-0.6),step(140,torque=0.0)]),
      ('torque-angle-default','torque',False,[step(0,angle=True,lat_active=True,speed=0,roll=0.12,desired=0.03),
         step(20,angle=True,lat_active=True,speed=5,roll=0.12,curvature=0.03,desired=0.03),
         step(40,angle=True,lat_active=True,speed=10,roll=0.12,curvature=0.01,desired=0.03),
         step(60,angle=True,lat_active=True,speed=15,roll=0.12,curvature=0.01,desired=-0.03),
         step(90,angle=True,lat_active=True,speed=25,roll=-0.04,curvature=-0.01,desired=-0.005),
         step(120,angle=True,lat_active=False,speed=20,roll=-0.04,desired=0.01)]),
      ('torque-angle-car','torque',False,[step(0,angle=True,lat_active=True,speed=15,desired=0.005),
         step(30,angle=True,lat_active=True,speed=20,desired=-0.009,roll=0.12),
         step(60,'override',angle=True,lat_active=True,speed=10,desired=0.03,roll=-0.1),
         step(90,'disengaged',angle=True,lat_active=True,speed=25,desired=0.05),
         step(120,angle=True,lat_active=True,speed=15,desired=-0.005)]),
      ('torque-demo','torque',True,[step(0,'disengaged',value=0.49),step(25,'override',value=0.5),
         step(50,'disengaged',value=0.75),step(75,'override',value=1.0),step(100,value=-1.0),step(130,value=0.0)]),
    ]
  results=[]
  for name,kind,demo,steps in cases:
    if args.filter and args.filter not in name:
      continue
    scene={'kind':kind,'config':{'big':False,'large_viewport':False,'pc':True,'scale':1.0},'language':'en',
           'rect':dict(zip(['x','y','width','height'],args.rect,strict=True)),'frames':160,'prime':0,'params':{},
           'capture_frames':list(range(160)),'indicator':{'demo':demo,'steps':steps}}
    if name=='torque-angle-car':
      scene['car']={'alpha_longitudinal_available':False,'openpilot_longitudinal_control':True,'max_lateral_accel':1.6}
    path=args.output/f'{name}.json'
    path.write_text(json.dumps(scene))
    outputs=[]
    with tempfile.TemporaryDirectory(prefix='msgq_rust-probe-indicator148-',dir='/dev/shm') as namespace:
      env=dict(os.environ,DISPLAY=args.display,PYTHONPATH=os.environ['UI_MSGQ_PYTHON']+os.pathsep+str(root),
               OFFSCREEN='1',OPENPILOT_PREFIX=Path(namespace).name.removeprefix('msgq_'))
      for lane in ['source','native']:
        output=args.output/f'{lane}-{name}.png'
        command=([sys.executable,str(root/'rust/tools/ui_application_qa/product_source.py'),str(path),str(output)]
                 if lane=='source' else [str(args.binary),str(root),str(path),str(output)])
        with output.with_suffix('.log').open('w') as log:
          subprocess.run(command,env=env,stdout=log,stderr=subprocess.STDOUT,check=True)
        outputs.append(output)
    states=[json.loads(output.with_suffix('.json').read_text()) for output in outputs]
    maximum=0.0
    for index,(source,native) in enumerate(zip(*states,strict=True)):
      a,b=source['indicator'],native['indicator']
      error=abs(a['value']-b['value'])
      maximum=max(maximum,error)
      assert error<=1e-12,(name,index,a,b)
      assert a.get('visible')==b.get('visible'),(name,index,a,b)
      assert abs(a.get('opacity',0)-b.get('opacity',0))<=1e-12,(name,index,a,b)
    differences=[]
    for index in scene['capture_frames']:
      images=[np.asarray(Image.open(output.with_name(f'{output.stem}-frame-{index:04}.png'))) for output in outputs]
      if not np.array_equal(*images):
        differences.append({'frame':index,'pixels':int(np.any(images[0]!=images[1],axis=-1).sum())})
    row={'scene':name,'frames':len(states[0]),'state_max_error':maximum,'different_frames':differences}
    results.append(row)
    (args.output/'results.json').write_text(json.dumps(results,indent=2))
    print(json.dumps(row),flush=True)
    assert not differences,row
  print(f'PASS {len(results)} original/native indicator cases; exact frames and 1e-12 filter/visibility agreement')


if __name__=='__main__':
  main()
