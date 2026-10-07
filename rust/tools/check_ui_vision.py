import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

import numpy as np
from PIL import Image


def packet(left=0,right=1,side='left',latency=485.0):
  return list(json.dumps({'type':'xiaogeVision','version':1,'lane':{'leftLine':left,'rightLine':right,'valid':True,
       'receivedMonoTimeNanos':10_000_000_000,'latencyMs':latency},'blindspot':{'left':False,'right':False,'valid':True,
       'receivedMonoTimeNanos':10_000_000_000,'side':side}}).encode())


def step(frame,**changes):
  return {'frame':frame,'started':True,'share':True,'started_frame':0,'left':False,'right':False,'car_publish':True,
          'car_valid':True,'vision_publish':True,'vision_valid':True,'payload':packet(),'now':None,**changes}


def main():
  parser=argparse.ArgumentParser()
  parser.add_argument('--binary',type=Path,required=True)
  parser.add_argument('--output',type=Path,required=True)
  parser.add_argument('--display',required=True)
  parser.add_argument('--filter',default='')
  args=parser.parse_args()
  args.output.mkdir(parents=True,exist_ok=True)
  root=Path(__file__).resolve().parents[2]
  encoded=list(bytes(packet(-1,0,latency=None)).decode().encode('utf-16-be'))
  cases=[
    ('states',[step(0,started=False,vision_publish=False),step(5,vision_publish=False),step(10),step(20,left=True,right=True),
       step(30,share=False,left=True),step(40,payload=packet(side='right')),step(50,car_valid=False,left=True,right=True),
       step(60,started_frame=9999),step(70),step(80,vision_valid=False),step(90,payload=list(b'{"invalid":true}')),
       step(100,payload=encoded),step(110,started=False),step(120,vision_publish=False)]),
    ('expiry',[step(0),step(1,vision_publish=False)]),
    ('latency',[step(0,payload=packet(latency=1450)),step(20,payload=packet(latency=True)),step(40,payload=packet(latency=float('nan'))),
       step(60,payload=packet(latency=10**200)),step(80,payload=packet(latency=None)),step(100,payload=packet(latency=0))]),
    ('vehicle-freshness',[step(0,left=True),step(10,left=True,car_publish=False),step(30,left=True,car_valid=False),
       step(50,right=True,started_frame=9999),step(70,right=True),step(90,share=False,left=True,right=True),
       step(110,share=False,car_publish=False,left=True,right=True)]),
  ]
  results=[]
  for language in ['en','ko']:
    for label,steps in cases:
      name=f'{label}-{language}'
      if args.filter and args.filter not in name:
        continue
      scene={'kind':'vision','config':{'big':False,'large_viewport':False,'pc':True,'scale':1.0},'language':language,
             'rect':{'x':12.3,'y':-3.7,'width':503.4,'height':239.3},'frames':130,'prime':0,'params':{},
             'capture_frames':list(range(130)),'vision':{'steps':steps}}
      path=args.output/f'{name}.json'
      path.write_text(json.dumps(scene))
      outputs=[]
      with tempfile.TemporaryDirectory(prefix='msgq_rust-probe-vision148-',dir='/dev/shm') as namespace:
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
      state_differences=[{'frame':frame,'source':a,'native':b} for frame,(a,b) in enumerate(zip(*traces,strict=True)) if a!=b]
      pixel_differences=[]
      for frame in range(130):
        images=[np.asarray(Image.open(output.with_name(f'{output.stem}-frame-{frame:04}.png'))) for output in outputs]
        if not np.array_equal(*images):
          pixel_differences.append({'frame':frame,'pixels':int(np.any(images[0]!=images[1],axis=-1).sum())})
      result={'scene':name,'frames':len(traces[0]),'state_differences':state_differences,'pixel_differences':pixel_differences}
      results.append(result)
      (args.output/'results.json').write_text(json.dumps(results,indent=2))
      print(json.dumps(result),flush=True)
      assert not state_differences and not pixel_differences,result
  print(f'PASS {len(results)} original/native EN/KO vision/BSD cases with exact frames and display/freshness states')


if __name__=='__main__':
  main()
