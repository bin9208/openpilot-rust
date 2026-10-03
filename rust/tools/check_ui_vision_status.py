import argparse
from copy import deepcopy
from dataclasses import asdict
import json
from pathlib import Path
import subprocess

from openpilot.selfdrive.ui.vision_status import parse_vision_display_packet, vision_display_state

parser=argparse.ArgumentParser()
parser.add_argument('--binary',type=Path,required=True)
parser.add_argument('--output',type=Path,required=True)
args=parser.parse_args()
args.output.mkdir(parents=True,exist_ok=True)
now=10_000_000_000
base={'type':'xiaogeVision','version':1,'lane':{'leftLine':0,'rightLine':1,'valid':True,'receivedMonoTimeNanos':now,'latencyMs':485.0},
      'blindspot':{'left':False,'right':False,'valid':True,'receivedMonoTimeNanos':now,'side':'left'}}
steps=[]


def append(payload,label,time=now):
  if not isinstance(payload,bytes):
    payload=json.dumps(payload,ensure_ascii=True).encode()
  steps.append({'payload':list(payload),'now':time,'label':label})


for delta in [-1,0,1,1_499_999_999,1_500_000_000,1_500_000_001,3_999_999_999,4_000_000_000,4_000_000_001]:
  for side in ['left','right','',None,True,['left'],{'side':'left'},'\ud800']:
    for left,right in [(False,False),(True,False),(False,True),(True,True)]:
      value=deepcopy(base)
      value['blindspot'].update(side=side,left=left,right=right)
      append(value,f'age-{delta}-{side!r}-{left}-{right}',now+delta)
for group,name,values in [
  ('root','version',[None,0,1,1.0,True,False,'1',float('nan'),float('inf')]),
  ('root','type',[None,True,1,'xiaogeVision','legacy']),
  ('lane','leftLine',[-2,-1,0,1,2,True,False,0.0,1.0,'0',None]),
  ('lane','rightLine',[-1,0,1,True,False,1.0,None]),
  ('lane','valid',[True,False,0,1,None,'true']),
  ('blindspot','valid',[True,False,0,1,None,'false']),
  ('blindspot','left',[True,False,0,1,None]),
  ('blindspot','right',[True,False,0,1,None]),
  ('lane','receivedMonoTimeNanos',[-1,0,now,now+1,now-4_000_000_001,True,False,0.0,1.0,None,2**127,10**400]),
  ('blindspot','receivedMonoTimeNanos',[-1,0,now,now+1,now-1_500_000_001,True,False,1.0,None,2**127,10**400]),
  ('lane','latencyMs',[None,-1,True,False,'485',float('nan'),float('inf'),float('-inf'),-0.0,0,1,485,1450,10**20,10**200,10**400,-10**400,1e308])]:
  for field in values:
    value=deepcopy(base)
    destination=value if group=='root' else value[group]
    destination[name]=field
    append(value,f'{group}.{name}={field!r}')
  value=deepcopy(base)
  del (value if group=='root' else value[group])[name]
  append(value,f'missing-{group}.{name}')
for group in ['lane','blindspot']:
  for replacement in [None,[],True,1,'object']:
    value=deepcopy(base)
    value[group]=replacement
    append(value,f'non-object-{group}-{replacement!r}')
value=deepcopy(base)
value['ignored']={'nonfinite':float('nan'),'surrogate':'\ud800','integer':10**400}
text=json.dumps(value,ensure_ascii=True)
for encoding in ['utf-8','utf-8-sig','utf-16','utf-16-le','utf-16-be','utf-32','utf-32-le','utf-32-be']:
  append(text.encode(encoding),f'encoding-{encoding}')
for encoding in ['utf-8','utf-16-le','utf-16-be','utf-32-le','utf-32-be']:
  raw=json.dumps(value,ensure_ascii=False).encode(encoding,errors='surrogatepass')
  append(raw,f'raw-surrogate-{encoding}')
  raw=raw[:-1]
  append(raw,f'truncated-{encoding}')
for payload in [b'',b'null',b'[]',b'true',b'1',b'{',b'{}',b'\xff',b'"\x00"',b'{} trailing']:
  append(payload,f'invalid-{payload!r}')
append(json.dumps(base).replace('"leftLine": 0','"leftLine": -0').encode(),'negative-zero-integer')
append(json.dumps(base).replace('"version": 1','"version": 0, "version": 1').encode(),'duplicate-version')
append(json.dumps(base).replace('"version": 1','"version": 1, "version": 0').encode(),'duplicate-invalid-version')
append(json.dumps(base).replace('"latencyMs": 485.0','"latencyMs": 485, "latencyMs": NaN').encode(),'duplicate-latency')
source=[]
for step in steps:
  try:
    packet=parse_vision_display_packet(bytes(step['payload']))
    state=vision_display_state(packet,step['now'])
    result=asdict(state)
    latency=result['latency_ms']
    result['latency_ms']=None if latency is None else {
      'kind':'integer' if isinstance(latency,int) else 'float','value':str(latency) if isinstance(latency,int) else latency,
    }
    source.append({'accepted':True,'state':result,'seconds':None if latency is None else latency/1000})
  except (ValueError,TypeError,UnicodeDecodeError,OverflowError) as error:
    source.append({'accepted':False,'overflow':isinstance(error,OverflowError)})
(args.output/'input.json').write_text(json.dumps(steps))
(args.output/'source.json').write_text(json.dumps(source))
result=subprocess.run([str(args.binary)],input=json.dumps(steps),text=True,capture_output=True,check=True)
(args.output/'native.json').write_text(result.stdout)
(args.output/'native.stderr').write_text(result.stderr or '(no stderr)\n')
native=json.loads(result.stdout)
differences=[{'step':index,'label':steps[index]['label'],'source':a,'native':b} for index,(a,b) in enumerate(zip(source,native,strict=True)) if a!=b]
(args.output/'result.json').write_text(json.dumps({'cases':len(steps),'differences':differences},indent=2))
assert not differences,differences[:10]
print(f'PASS {len(steps)} original/native vision packet acceptance, exact freshness/side/latency states and seconds')
