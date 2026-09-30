"""Observe real source fcntl and native ioctl/close calls on a selected temporary file."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

ROOT=Path(__file__).resolve().parents[2]
ACTIONS=[{'kind':'read','register':0x51},{'kind':'write','register':0x51,'value':0x85},
         {'kind':'read','register':0x51},{'kind':'write','register':0x10,'value':0x17},{'kind':'read','register':0x10}]


def run(args,side,name,faults,missing=False):
  output=args.output/name/side
  output.mkdir(parents=True)
  device=output/'device'
  if not missing:
    device.write_bytes(b'only a regular fixture file')
  config={'device':str(device),'actions':ACTIONS}
  (output/'input.json').write_text(json.dumps(config))
  environment=dict(os.environ,AMP_DEVICE_PATH=str(device),AMP_TRACE_PATH=str(output/'syscalls.jsonl'),AMP_STATE_PATH=str(output/'registers.bin'),
                   AMP_SEED='47',AMP_FAULTS=','.join(f'{index}:{error}' for index,error in faults.items()),LD_PRELOAD=str(args.fixture))
  command=[sys.executable,str(ROOT/'rust/tools/amplifier_linux_source.py'),str(output/'input.json')] if side=='source' else [*args.runner,str(args.binary),str(output/'input.json')]
  run=subprocess.run(command,env=environment,capture_output=True,text=True)
  (output/'process-stdout.log').write_text(run.stdout or '<empty>\n')
  (output/'process-stderr.log').write_text(run.stderr or '<empty>\n')
  assert run.returncode==0,(side,run.returncode,run.stderr)
  outcome=json.loads(run.stdout)
  trace=output/'syscalls.jsonl'
  events=[json.loads(line) for line in trace.read_text().splitlines()] if trace.exists() else []
  for event in events:
    if event['kind']=='close':
      assert event['closed'] is True
    elif event['kind']=='smbus':
      assert event['struct_size']==16 and event['union_size']==34 and event['aligned'] is True
  result={'outcome':outcome,'events':events,'registers':list((output/'registers.bin').read_bytes())}
  (output/'result.json').write_text(json.dumps(result,indent=2))
  (output/'stderr.log').write_text(run.stderr or '<empty>\n')
  (output/'invocation.json').write_text(json.dumps({'argv':command,'environment':{key:environment[key] for key in ['AMP_DEVICE_PATH','AMP_TRACE_PATH','AMP_STATE_PATH','AMP_SEED','AMP_FAULTS','LD_PRELOAD']},
    'exit':run.returncode,'binary_sha256':hashlib.sha256(args.binary.read_bytes()).hexdigest() if side=='native' else None},indent=2))
  return result


def main():
  parser=argparse.ArgumentParser()
  parser.add_argument('binary',type=Path)
  parser.add_argument('fixture',type=Path)
  parser.add_argument('output',type=Path)
  parser.add_argument('--runner',action='append',default=[])
  parser.add_argument('--case')
  args=parser.parse_args()
  args.binary,args.fixture,args.output=args.binary.resolve(),args.fixture.resolve(),args.output.resolve()
  cases=[('ordinary',{},False),('missing_device',{},True)]
  for index in range(11):
    cases.append((f'io_error_{index}',{index:5},False))
    cases.append((f'interrupted_{index}',{index:4},False))
  cases.extend([('close_overrides_read',{1:5,2:28},False),('close_overrides_write',{3:5,4:28},False),
                ('repeat_interrupted',{0:4,1:4,2:4,4:4,5:4},False)])
  results=[]
  for name,faults,missing in cases:
    if args.case and args.case!=name:
      continue
    expected=run(args,'source',name,faults,missing)
    actual=run(args,'native',name,faults,missing)
    assert expected==actual,(name,expected,actual)
    results.append({'scenario':name,'result':'PASS','syscalls':len(actual['events'])})
    print(name,'PASS',flush=True)
  (args.output/'result.json').write_text(json.dumps({'result':'PASS','cases':results},indent=2))


if __name__=='__main__':
  main()
