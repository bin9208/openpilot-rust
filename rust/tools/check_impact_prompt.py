"""Compare the exact pinned UI prompt class against native policy; no device/GPU."""
import argparse
import ast
import hashlib
import itertools
import json
import math
from pathlib import Path
import random
import subprocess
from types import SimpleNamespace as NS

PIN = 'e8e152dbcfd9574a1791ed12026d1880e87689b3'
PATH = 'openpilot/selfdrive/ui/impact_dashcam.py'


def source(steps, content):
  memory, params, draw = {}, {}, []
  sm = {'selfdriveState': NS(alertSize=NS(raw=0), alertStatus=NS(raw=0))}
  class Messages(dict):
    pass
  sm = Messages(sm)
  ui = NS(params_memory=NS(put=lambda key, value: memory.__setitem__(key,value)),params=NS(get_bool=lambda key: params.get(key,False)),started=True,sm=sm)
  app = NS(mouse_events=[],width=536,add_nav_stack_tick=lambda cb: None)
  clock = NS(now=1.0)
  namespace = dict(ui_state=ui,gui_app=app,read_object=lambda store,key: memory.get(key,{}),
                   FEEDBACK_KEY='feedback',NOTICE_KEY='notice',REBOOT_KEY='reboot',COUNTDOWN_SECONDS=10,
                   time=NS(monotonic=lambda: clock.now),math=math,tr=lambda s:s,
                   rl=NS(Rectangle=lambda x,y,w,h:NS(x=x,y=y,width=w,height=h),Color=lambda *args:args,WHITE='white',draw_rectangle_rounded=lambda *args:None),
                   gui_label=lambda rect,text,*args,**kwargs:draw.append(text),FontWeight=NS(BOLD=1,NORMAL=0),TextAlignment=NS(CENTER=1))
  tree=ast.parse(content)
  cls=next(node for node in tree.body if isinstance(node,ast.ClassDef))
  exec(compile(ast.fix_missing_locations(ast.Module(body=[cls],type_ignores=[])),PATH,'exec'),namespace)
  prompt=namespace['ImpactDashcamPrompt']()
  output=[]
  for step in steps:
    memory['notice']={} if step['token'] is None else {'token':step['token'],'created':0.0}
    params['reboot']=step['rebooting'];ui.started=step['started'];clock.now=step['now']
    sm.alive={'selfdriveState':step['alive']};sm.valid={'selfdriveState':step['valid']};sm.recv_time={'selfdriveState':step['received']}
    sm['selfdriveState'].alertSize.raw=3 if step['full'] else 0;sm['selfdriveState'].alertStatus.raw=2 if step['critical'] else 0
    app.mouse_events=[NS(left_down=True,left_pressed=False,left_released=False)] if step['touched'] else []
    draw.clear();prompt.handle_touch();prompt.render()
    remaining=None if not draw else max(0, math.ceil(10-(step['now']-prompt.shown_since)))
    output.append({'consumed':step['touched'] and not app.mouse_events,'remaining':remaining,'policy':{'token':step['token'],'shown_since':prompt.shown_since,'cancelled_token':prompt.cancelled_token}})
  return output


def main():
  parser=argparse.ArgumentParser();parser.add_argument('binary',type=Path);parser.add_argument('output',type=Path);parser.add_argument('--runner',nargs=argparse.REMAINDER,default=[]);args=parser.parse_args()
  content=subprocess.check_output(['git','show',f'{PIN}:{PATH}'])
  base=dict(token='1',started=True,rebooting=False,touched=False,now=1.,alive=True,valid=True,received=1.,full=False,critical=False)
  cases=[]
  for bs in itertools.product((False,True),repeat=7):
    cases.append([dict(base,**dict(zip(['started','rebooting','touched','alive','valid','full','critical'],bs)))])
  rng=random.Random(338)
  for case in range(30):
    steps=[]
    for i in range(80):
      now=i*.1+1
      steps.append(dict(base,token=None if i%31==0 else str(i//27),now=now,received=now-rng.choice([0,.499999,.5,.500001]),touched=rng.random()<.02,full=rng.random()<.04,critical=rng.random()<.04,rebooting=i>70,started=i%39!=0))
    cases.append(steps)
  cases.append([dict(base,now=1+i*.1,received=1+i*.1) for i in range(115)])
  expected=[source(case,content) for case in cases]
  run=subprocess.run([*args.runner,str(args.binary.resolve())],input='\n'.join(json.dumps(case) for case in cases)+'\n',text=True,capture_output=True,check=True,timeout=60)
  actual=[json.loads(line) for line in run.stdout.splitlines()]
  for index,(a,b) in enumerate(zip(expected,actual,strict=True)):
    assert a==b,(index,a,b)
  args.output.mkdir(parents=True,exist_ok=True)
  result=dict(source_commit=PIN,source_path=PATH,source_sha256=hashlib.sha256(content).hexdigest(),binary_sha256=hashlib.sha256(args.binary.read_bytes()).hexdigest(),cases=len(cases),steps=sum(map(len,cases)),result='pass',scope='actual source prompt class/policy transitions; GPU drawing and input dispatch require separate validation')
  (args.output/'result.json').write_text(json.dumps(result,indent=2)+'\n');print(result)
if __name__=='__main__':main()
