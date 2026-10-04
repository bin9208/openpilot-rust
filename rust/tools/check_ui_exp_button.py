import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

import numpy as np
from PIL import Image


def backend(frame,experimental=False,engageable=True,enabled=False):
  return {'frame':frame,'experimental':experimental,'engageable':engageable,'enabled':enabled}


def event(frame,pressed=False,released=False,x=101,y=111,slot=0):
  return {'frame':frame,'events':[{'pos':{'x':x,'y':y},'slot':slot,'pressed':pressed,'released':released,'down':not released,'time':frame/20}]}


def main():
  parser=argparse.ArgumentParser()
  parser.add_argument('--binary',type=Path,required=True)
  parser.add_argument('--output',type=Path,required=True)
  parser.add_argument('--display',required=True)
  parser.add_argument('--filter',default='')
  args=parser.parse_args()
  args.output.mkdir(parents=True,exist_ok=True)
  root=Path(__file__).resolve().parents[2]
  cp={'alpha_longitudinal_available':False,'openpilot_longitudinal_control':True,'max_lateral_accel':3.0}
  clicks=[event(5,pressed=True),event(6,released=True),event(40,pressed=True),event(41,released=True),event(90,pressed=True),event(91,released=True)]
  cases=[
    ('hold',{**cp},{'ExperimentalModeConfirmed':'1'},[backend(0),backend(30,True),backend(60),backend(110,True)],clicks),
    ('unconfirmed',{**cp},{'ExperimentalModeConfirmed':'0'},[backend(0)],clicks),
    ('no-longitudinal',{**cp,'openpilot_longitudinal_control':False},{'ExperimentalModeConfirmed':'1'},[backend(0)],clicks),
    ('no-car',None,{'ExperimentalModeConfirmed':'1'},[backend(0)],clicks),
    ('disabled-engage',{**cp},{'ExperimentalModeConfirmed':'1'},[backend(0,engageable=False),backend(60,engageable=False,enabled=True)],clicks),
    ('alpha-off',{**cp,'alpha_longitudinal_available':True},{'ExperimentalModeConfirmed':'1','AlphaLongitudinalEnabled':'0'},[backend(0)],clicks),
    ('alpha-on',{**cp,'alpha_longitudinal_available':True},{'ExperimentalModeConfirmed':'1','AlphaLongitudinalEnabled':'1'},[backend(0,True)],clicks),
    ('touch-cancel',{**cp},{'ExperimentalModeConfirmed':'1'},[backend(0)],
      [event(5,pressed=True),event(6,released=True,x=400),event(20,pressed=True,slot=1),event(21,released=True,slot=1),
       event(40,pressed=True),event(41,x=400),event(42,x=101),event(43,released=True)]),
  ]
  results=[]
  for name,car,params,steps,touches in cases:
    if args.filter and args.filter not in name:
      continue
    scene={'kind':'exp-button','config':{'big':True,'large_viewport':True,'pc':True,'scale':1.0},'language':'en',
           'rect':{'x':20.3,'y':30.7,'width':500.4,'height':240.2},'frames':140,'prime':0,'params':params,'car':car,
           'background':[31,37,43,255],'capture_frames':list(range(140)),'steps':touches,
           'exp':{'button_size':162,'icon_size':108,'steps':steps}}
    path=args.output/f'{name}.json'
    path.write_text(json.dumps(scene))
    outputs=[]
    with tempfile.TemporaryDirectory(prefix='msgq_rust-probe-exp148-',dir='/dev/shm') as namespace:
      env=dict(os.environ,DISPLAY=args.display,PYTHONPATH=os.environ['UI_MSGQ_PYTHON']+os.pathsep+str(root),
               OFFSCREEN='1',OPENPILOT_PREFIX=Path(namespace).name.removeprefix('msgq_'))
      for lane in ['source','native']:
        output=args.output/f'{lane}-{name}.png'
        command=([sys.executable,str(root/'rust/tools/ui_application_qa/product_source.py'),str(path),str(output)] if lane=='source'
                 else [str(args.binary),str(root),str(path),str(output)])
        with output.with_suffix('.log').open('w') as log:
          subprocess.run(command,env=env,stdout=log,stderr=subprocess.STDOUT,check=True)
        outputs.append(output)
    traces=[json.loads(output.with_suffix('.json').read_text()) for output in outputs]
    states=[{'frame':frame,'source':a,'native':b} for frame,(a,b) in enumerate(zip(*traces,strict=True)) if a!=b]
    pixels=[]
    for frame in range(140):
      images=[np.asarray(Image.open(output.with_name(f'{output.stem}-frame-{frame:04}.png'))) for output in outputs]
      if not np.array_equal(*images):
        pixels.append({'frame':frame,'pixels':int(np.any(images[0]!=images[1],axis=-1).sum())})
    result={'scene':name,'frames':len(traces[0]),'state_differences':states,'pixel_differences':pixels}
    results.append(result)
    (args.output/'results.json').write_text(json.dumps(results,indent=2))
    print(json.dumps(result),flush=True)
    assert not states and not pixels,result
  print(f'PASS {len(results)} original/native experimental button cases; exact frames, Params, press and hold states')


if __name__=='__main__':
  main()
