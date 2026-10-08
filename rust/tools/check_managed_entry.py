#!/usr/bin/env python3
# /// script
# requires-python = ">=3.12,<3.13"
# dependencies = ["pyzmq==27.2.0", "pycapnp==2.1.0", "sentry-sdk==2.55.0", "zstandard==0.25.0", "numpy==2.5.3", "setproctitle==1.3.7"]
# ///
"""Original/native managed child policy through actual collectors, Params, IPC and local SDK."""
from __future__ import annotations
import argparse
import hashlib
from importlib.metadata import version
import json
import os
from pathlib import Path
import select
import signal
import subprocess
import sys
import tempfile
import time

import msgq
from openpilot.cereal import log
from logmessaged_native import Peer
from check_crash_sdk_transport import Receiver

ROOT=Path(__file__).resolve().parents[2]
CASES=[
 ('normal',{'body':'return'}),
 ('interrupt',{'body':'interrupt'}),
 ('sigint',{'body':'signal'}),
 ('error',{'body':'error','real_sdk':True}),
 ('chain',{'body':'chain','real_sdk':True}),
 ('reporting-disabled',{'body':'error','real_sdk':True,'reporting':False}),
 ('prepare-error',{'body':'return','prepare':'error','real_sdk':True}),
 ('prepare-interrupt',{'body':'return','prepare':'interrupt'}),
 ('reset-error',{'body':'return','reset':'error'}),
 ('reset-interrupt',{'body':'return','reset':'interrupt'}),
 ('name-error',{'body':'return','process':'invalid\0name'}),
 ('tag-error',{'body':'return','sdk_failure':'daemon_tag'}),
 ('capture-error',{'body':'error','sdk_failure':'capture_exception'}),
 ('flush-error',{'body':'error','sdk_failure':'flush'}),
 ('logger-error',{'body':'error','close_logger':True}),
 ('logger-interrupt',{'body':'interrupt','close_logger':True}),
 ('params-open-error',{'body':'error','params_fault':'open'}),
 ('params-put-error',{'body':'error','params_fault':'put'}),
 ('already-sent',{'body':'error','sent':'1','prior':'previous','real_sdk':True}),
 ('false-sent',{'body':'error','sent':'true','prior':'previous'}),
 ('global-context-precedence',{'body':'error','global_daemon':'global-owner','daemon':'local-child','real_sdk':True}),
 ('unicode-daemon',{'body':'interrupt','daemon':'데몬😀'}),
]


def receive(process,output):
 assert select.select([process.stdout],[],[],15)[0],('child stdout timeout',output)
 line=process.stdout.readline();assert line,('child exited',process.poll(),output)
 with (output/'responses.jsonl').open('a') as stream:stream.write(line)
 return json.loads(line)


def event(peer,name,output):
 packet=peer.subscribers[name].receive();assert packet is not None,(name,output)
 with (output/(name+'.bin')).open('ab') as stream:stream.write(packet)
 with log.Event.from_bytes(packet) as value:
  assert value.valid and value.which()==name and value.logMonoTime>0
  return json.loads(getattr(value,name))


def normalized(value):
 if isinstance(value,dict):
  if value.get('op')=='capture_exception':value={**value,'fields':{'captured_exception':True}}
  return {key:(json.loads(item) if key=='value_json' else normalized(item)) for key,item in value.items() if key not in ['exception','traceback','message','kind','cmdline','comm','secondary','exception_type']}
 if isinstance(value,list):return [normalized(item) for item in value]
 return value


def run(binary,collector,binding,output,scenario,kind,original,runner):
 output.mkdir(parents=True);peer=Peer(collector,output/('collector-'+output.parent.name+'-'+kind),original);receiver=Receiver();process=None;root=None;body_subscriber=None
 try:
  peer.start()
  peer.synchronize()
  body_subscriber=msgq.sub_sock('managedEntryBody',segment_size=1024*1024,timeout=5000)
  with tempfile.TemporaryDirectory(prefix='managed-entry-') as temporary:
   root=Path(temporary)
   for name in ['base/openpilot/common','params/'+peer.prefix]:(root/name).mkdir(parents=True)
   (root/'base/openpilot/common/version.h').write_text('#define VERSION "fixture-entry"\n')
   origin='git@github.com:commaai/openpilot.git' if scenario.get('reporting',True) else 'git@github.com:fixture/fork.git'
   (root/'base/build.json').write_text(json.dumps({'channel':'nightly','openpilot':{'git_origin':origin,'git_commit':'abcdefghijk'}}))
   (root/'params'/peer.prefix/'DongleId').write_text('fixture-device')
   (root/'prepared-input').write_text('prepared native body')
   (root/'params-blocked').write_text('not a directory')
   if 'sent' in scenario:(root/'params'/peer.prefix/'CarrotExceptionSent').write_text(scenario['sent'])
   if 'prior' in scenario:(root/'params'/peer.prefix/'CarrotException').write_text(scenario['prior'])
   request={'root':str(root),'prefix':peer.prefix,'endpoint':peer.endpoint,'dsn':receiver.dsn,'process':'openpilot.fixture_managed_entry','daemon':'managed-fixture',**scenario}
   path=output/'request.json';path.write_text(json.dumps(request)+'\n')
   command=[sys.executable,str(ROOT/'rust/tools/managed_entry_reference.py'),str(path),str(binding)] if kind=='source' else [*runner,str(binary),str(path)]
   with (output/'stderr.txt').open('w') as stderr:
    process=subprocess.Popen(command,env={**os.environ,'OPENPILOT_PREFIX':peer.prefix,'RUST_BACKTRACE':'1','PYTHONDONTWRITEBYTECODE':'1'},stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=stderr,text=True)
   ready=receive(process,output);assert ready['ready'] and ready['reporting_enabled']==scenario.get('reporting',True)
   first=event(peer,'logMessage',output);assert first['msg']=='managed-entry-ready'
   process.stdin.write('continue\n');process.stdin.flush()
   response=receive(process,output);signal_elapsed=None
   if response.get('context_ready'):
    assert body_subscriber.receive(non_blocking=True) is None
    process.stdin.write('continue\n');process.stdin.flush();response=receive(process,output)
   if response.get('waiting_for_sigint'):
    before=time.monotonic();process.send_signal(signal.SIGINT);response=receive(process,output);signal_elapsed=time.monotonic()-before;assert signal_elapsed<5
   body=any(step['step']=='body' for step in response['steps'])
   records={'logMessage':[],'errorLogMessage':[]}
   # Each policy branch has a known message count; inspect both real collector topics.
   expected=[]
   if body:expected.append(('managed body entered',20))
   if response['outcome']['kind']=='interrupted':expected.append(('child '+request['process']+' got SIGINT',30))
   elif response['outcome']['kind'] in ['raised','reporting_failed'] and not scenario.get('close_logger'):expected.append(('crash',40))
   if scenario.get('sdk_failure') in ['capture_exception','flush']:expected.append(('sentry exception',40))
   for message,level in expected:
    value=event(peer,'logMessage',output);assert value['msg']==message and value['levelnum']==level
    records['logMessage'].append(value)
    if level>=40:
     error=event(peer,'errorLogMessage',output);assert error==value;records['errorLogMessage'].append(error)
   if body:
    payload=body_subscriber.receive();assert payload==b'managed body IPC';(output/'body-message.bin').write_bytes(payload)
   else:assert body_subscriber.receive(non_blocking=True) is None
   captures=[]
   actual_capture=any(call['op']=='capture_exception' for call in response['sdk_calls']) and not scenario.get('sdk_failure')
   if request.get('real_sdk') and ready['reporting_enabled'] and actual_capture:captures.append(receiver.events.get(timeout=10))
   assert receiver.events.empty()
   process.stdin.write('continue\n');process.stdin.flush();exit_code=process.wait(timeout=5)
   assert exit_code==(0 if response['outcome']['kind'] in ['returned','interrupted'] else 1)
   assert all(peer.subscribers[name].receive(non_blocking=True) is None for name in records)
   parameter=root/'params'/peer.prefix/'CarrotException';params=parameter.read_text() if parameter.is_file() else None
   (root/'params').chmod(0o700)
   code,_=peer.stop();assert code in (0,-signal.SIGINT) if original else code==0
   disk=[json.loads(line) for path in peer.root.glob('swaglog.*') for line in path.read_text().splitlines()]
   assert len(disk)==len(records['logMessage'])
   for stored,wire in zip(disk,records['logMessage'],strict=True):
    payload=stored.copy();payload.pop('id');payload['msg']=payload.pop('msg$s');assert payload==wire
   result={'result':'PASS','argv':command,'response':response,'ready':ready,'exit_code':exit_code,'signal_response_seconds':signal_elapsed,'records':records,'disk':disk,'params':params,'captures':captures}
   (output/'result.json').write_text(json.dumps(result,indent=2)+'\n');return result
 finally:
  if process is not None and process.poll() is None:process.kill();process.wait(timeout=5)
  if root is not None and (root/'params').exists():(root/'params').chmod(0o700)
  body_subscriber=None;peer.close();receiver.close()


def compare(source,native,scenario):
 assert source['exit_code']==native['exit_code'] and source['params']==native['params']
 assert source['response']['outcome']['kind']==native['response']['outcome']['kind']
 if native['response']['outcome']['kind']!='interrupt_log_failed':assert source['response']['outcome'].get('stage')==native['response']['outcome'].get('stage')
 assert normalized(source['response']['steps'])==normalized(native['response']['steps'])
 assert normalized(source['response']['sdk_calls'])==normalized(native['response']['sdk_calls'])
 for name in ['logMessage','errorLogMessage']:
  left,right=source['records'][name],native['records'][name];assert len(left)==len(right)
  for expected,actual in zip(left,right,strict=True):
   context={key:value for key,value in actual['ctx'].items() if key not in ['runtime_language','source_commit','source_tree']}
   assert expected['msg']==actual['msg'] and expected['levelnum']==actual['levelnum'] and expected['ctx']==context
   assert ('exc_info' in expected)==('exc_info' in actual)
   assert actual['ctx']['runtime_language']=='rust'
   if actual['msg']=='crash':
    outcome=native['response']['outcome'];exception=outcome.get('exception',outcome.get('original'))
    assert actual['exc_info'].startswith(exception['kind']+': '+exception['message']+'\n')
    assert exception['backtrace'] not in ['disabled backtrace','unsupported backtrace','']
 assert len(source['captures'])==len(native['captures'])
 for expected,actual in zip(source['captures'],native['captures'],strict=True):
  left,right=expected['event'],actual['event']
  for key in ['tags','user','release','environment']:assert left.get(key)==right.get(key),(key,left,right)
  assert left['platform']=='python' and right['platform']=='native'
  assert left['exception']['values'] and right['exception']['values']
  assert right['extra']['rust_backtrace'] not in ['disabled backtrace','']
 if not scenario.get('prepare') and '\0' not in scenario.get('process',''):
  assert source['response']['comm']==native['response']['comm']==scenario.get('process','openpilot.fixture_managed_entry')[:15]
  assert source['response']['cmdline'].strip()==scenario.get('process','openpilot.fixture_managed_entry')
  assert 'managed_child' in native['response']['cmdline'] and not native['response']['cmdline'].startswith('openpilot.fixture_managed_entry')
 return {'result':'PASS','exit_code':native['exit_code'],'params':native['params'],'log_messages':len(native['records']['logMessage']),'error_messages':len(native['records']['errorLogMessage']),'sdk_events':len(native['captures'])}


def main():
 parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('binary',type=Path);parser.add_argument('collector',type=Path);parser.add_argument('binding',type=Path);parser.add_argument('output',type=Path);parser.add_argument('--case');parser.add_argument('--runner',nargs=argparse.REMAINDER,default=[]);args=parser.parse_args()
 args.output=args.output.resolve();args.output.mkdir(parents=True,exist_ok=True);results=[]
 for original in [True,False]:
  for name,scenario in CASES:
   if args.case and args.case!=name:continue
   target=args.output/('original-collector' if original else 'native-collector')/name
   source=run(args.binary.resolve(),args.collector.resolve(),args.binding.resolve(),target/'source',scenario,'source',original,args.runner)
   native=run(args.binary.resolve(),args.collector.resolve(),args.binding.resolve(),target/'native',scenario,'native',original,args.runner)
   result={'collector':'original' if original else 'native','case':name,**compare(source,native,scenario)};results.append(result);print(json.dumps(result),flush=True)
 sources=['openpilot/system/manager/process.py','openpilot/system/sentry.py','openpilot/common/logging_extra.py','openpilot/common/swaglog.py','openpilot/common/params_pyx.pyx']
 manifest={'result':'PASS','comparisons':len(results),'results':results,'binary_sha256':hashlib.sha256(args.binary.read_bytes()).hexdigest(),'collector_sha256':hashlib.sha256(args.collector.read_bytes()).hexdigest(),'params_binding_sha256':hashlib.sha256(args.binding.read_bytes()).hexdigest(),'source_sha256':{name:hashlib.sha256((ROOT/name).read_bytes()).hexdigest() for name in sources},'python':sys.version,'pycapnp':__import__('capnp').__version__,'sentry_sdk':version('sentry-sdk'),'setproctitle':version('setproctitle'),'msgq_binding':{'path':sys.modules['msgq.ipc_pyx'].__file__,'sha256':hashlib.sha256(Path(sys.modules['msgq.ipc_pyx'].__file__).read_bytes()).hexdigest()},'runner':args.runner}
 (args.output/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n');print(json.dumps(manifest,indent=2))

if __name__=='__main__':main()
