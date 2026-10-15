"""Pinned upstream diagnostic store oracle; only owned temporary files, no uploads."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile
import types

PIN='e8e152dbcfd9574a1791ed12026d1880e87689b3'
def generate(output):
  blob=subprocess.check_output(['git','show',f'{PIN}:openpilot/system/manager/process_crash.py'])
  module=types.ModuleType('original_process_crash');exec(compile(blob,'original_process_crash.py','exec'),module.__dict__)
  cases=[]
  events=[]
  for code in list(range(-70,3)):
    events.append({'id':f'1-2-{len(events)}','name':'worker_1','pid':2,'exitcode':code,'git_branch':'test','git_commit':'test-commit','dongle_id':'test-device'})
  base=dict(events[0])
  for key,values in [('id',['../escape','1'*300,'1-2-3','01-02-03','1-2-','1-2-٣','9'*20+'-1-2','9'*21+'-1-2',None,42]),('name',['','a/b','a'*100,'a'*101,'차량','name-1','worker','False',123,True,None]),('pid',[None,'2',2.0,True,False,-1,0]),('exitcode',[None,'-6',-6.0,True,False])]:
    for value in values:events.append({**base,key:value})
  with tempfile.TemporaryDirectory(prefix='crash-source-') as scratch:
    for i,event in enumerate(events):
      root=Path(scratch)/str(i);calls=[];module.time.time=lambda:1700000000.125
      text=('한😀x' * 40000) if i%5==0 else 'original traceback\n'
      def capture(event):calls.append(['capture',dict(event)]);return text
      def payload(event):
        calls.append(['payload',dict(event)])
        if i%3==0:raise OSError('owned Params unavailable')
        return {'tmux_why':'process_crash','car_name':'owned'}
      collector=module.CrashCollector(root=root,capture=capture,payload=payload)
      try:collector.record(dict(event));error=None
      except Exception as e:error=type(e).__name__
      reports=[]
      for path in sorted(root.glob('*.json')):
        raw=path.with_suffix('.txt').read_bytes();reports.append({'file':path.name,'report':json.loads(path.read_text()),'text_sha256':hashlib.sha256(raw).hexdigest(),'text_bytes':len(raw)})
      cases.append({'event':event,'large':i%5==0,'payload_error':i%3==0,'error':error,'calls':calls,'reports':reports})
  output.write_text(json.dumps({'pin':PIN,'source_sha256':hashlib.sha256(blob).hexdigest(),'cases':cases},ensure_ascii=False,indent=2)+'\n')
  print(f'{len(cases)} actual pinned source diagnostic store cases')
if __name__=='__main__':
  p=argparse.ArgumentParser();p.add_argument('--output',type=Path,required=True);generate(p.parse_args().output)
