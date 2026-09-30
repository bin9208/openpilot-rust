#!/usr/bin/env python3
# /// script
# requires-python = ">=3.12,<3.13"
# dependencies = ["pyzmq==27.2.0", "pycapnp==2.1.0", "sentry-sdk==2.55.0", "zstandard==0.25.0", "numpy==2.5.3"]
# ///
"""Actual daemon shutdown during held apport; fixture processes and loopback only."""
from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
from types import SimpleNamespace
import uuid

from check_crash_sdk_transport import Receiver
from check_tombstoned_runtime import fixture
from check_tombstoned_reference import BODY, ROOT


def source_main(binding, root):
  from tombstoned_reference import Worker
  worker = Worker(binding)
  worker.handle({"op": "configure", "base": str(root / "base"), "params": str(root / "params"), "pc": False, "device": "tici"})
  worker.root = root / "logs"
  worker.tomb.APPORT_DIR = str(root / "apport") + "/"
  def sleep(seconds):
    assert seconds == 5
    (root / "ready").touch()
    time.sleep(seconds)
  worker.tomb.time = SimpleNamespace(sleep=sleep)
  worker.tomb.main()


def processes(token):
  matches=[]
  needle=("TOMBSTONE_SHUTDOWN_FIXTURE=" + token).encode()
  for directory in Path('/proc').iterdir():
    if not directory.name.isdecimal():continue
    try:
      if needle not in (directory/'environ').read_bytes().split(b'\0'):continue
      stat=(directory/'stat').read_text().split(') ',1)[1].split()
      matches.append({'pid':int(directory.name),'state':stat[0],'ppid':int(stat[1]),'group':int(stat[2]),'command':(directory/'cmdline').read_bytes().decode(errors='backslashreplace').replace('\0',' ')})
    except (FileNotFoundError,ProcessLookupError,PermissionError):continue
  return matches


def wait_for(predicate, process, seconds=12):
  deadline=time.monotonic()+seconds
  while time.monotonic()<deadline:
    if predicate():return
    assert process.poll() is None, ('early exit',process.returncode)
    time.sleep(.01)
  raise AssertionError('fixture readiness timed out')


def run(binary,binding,output,kind,sig,runner):
  receiver=Receiver(); process=None; owned=[]
  output.mkdir(parents=True)
  token=uuid.uuid4().hex
  try:
    with tempfile.TemporaryDirectory(prefix='tombstone-shutdown-') as temporary:
      root=Path(temporary);fixture(root,'fixture')
      (root/'bin/apport-retrace').write_text('#!/bin/bash\nhead -c 4096 "$2" > "$FIXTURE_INPUT"\necho $$ > "$FIXTURE_PID"\nsleep 60 &\necho $! > "$FIXTURE_DESCENDANT"\nwait\n')
      env={**os.environ,'OPENPILOT_PREFIX':'fixture','PARAMS_ROOT':str(root/'params'),'PATH':str(root/'bin')+':/usr/bin:/bin','FIXTURE_INPUT':str(root/'retrace.input'),'FIXTURE_TRACE':str(root/'trace'),'FIXTURE_PID':str(root/'helper.pid'),'FIXTURE_DESCENDANT':str(root/'descendant.pid'),'TOMBSTONE_SHUTDOWN_FIXTURE':token}
      if kind=='source':command=[sys.executable,str(Path(__file__).resolve()),'--source-main',str(binding),str(root)]
      else:command=[*runner,str(binary),'--base-dir',str(root/'base'),'--apport-dir',str(root/'apport'),'--log-root',str(root/'logs'),'--local-sentry-dsn',receiver.dsn,'--local-reporting-device','tici']
      stderr=output/'stderr.txt'
      with stderr.open('w') as errors,(output/'stdout.txt').open('w') as stdout:
        process=subprocess.Popen(command,env=env,cwd=ROOT,stdout=stdout,stderr=errors,start_new_session=True)
      wait_for(lambda:(root/'ready').exists() if kind=='source' else 'tombstoned: ready' in stderr.read_text(),process)
      (root/'apport/new.crash').write_text(BODY);(root/'apport/new.crash').chmod(0o640)
      wait_for(lambda:(root/'descendant.pid').exists(),process)
      before=processes(token);owned=[row['pid'] for row in before]
      assert len(before)>=3,before
      assert receiver.events.empty()
      begin=time.monotonic();process.send_signal(sig)
      try:exit_code=process.wait(timeout=5);timed_out=False
      except subprocess.TimeoutExpired:exit_code=None;timed_out=True
      elapsed=time.monotonic()-begin;after=processes(token)
      active=[row for row in after if row['pid']!=process.pid and row['state']!='Z']
      result={'kind':kind,'signal':signal.Signals(sig).name,'argv':command,'before':before,'after':after,'exited_within_5_seconds':not timed_out,'seconds':elapsed,'exit_code':exit_code,'active_descendants_after_stop':active,'http_events':receiver.events.qsize(),'crash_retained':(root/'apport/new.crash').exists(),'crash_copied':(root/'logs/crash').exists()}
      (output/'result.json').write_text(json.dumps(result,indent=2)+'\n');return result
  finally:
    if process is not None:
      for group in {row['group'] for row in processes(token)}|{process.pid}:
        try:os.killpg(group,signal.SIGKILL)
        except ProcessLookupError:pass
      if process.poll() is None:process.kill()
      process.wait(timeout=5)
    receiver.close()


def main():
  if len(sys.argv)>1 and sys.argv[1]=='--source-main':source_main(Path(sys.argv[2]),Path(sys.argv[3]));return
  parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('binary',type=Path);parser.add_argument('binding',type=Path);parser.add_argument('output',type=Path);parser.add_argument('--observe',action='store_true');parser.add_argument('--runner',nargs=argparse.REMAINDER,default=[]);args=parser.parse_args();args.output.mkdir(parents=True,exist_ok=True)
  results=[run(args.binary.resolve(),args.binding.resolve(),args.output/(kind+'-'+signal.Signals(sig).name),kind,sig,args.runner) for kind,sig in [('source',signal.SIGINT),('native',signal.SIGINT),('native',signal.SIGTERM)]]
  passed=all(row['exited_within_5_seconds'] and row['exit_code']==0 and not row['active_descendants_after_stop'] and row['crash_retained'] and not row['crash_copied'] and row['http_events']==0 for row in results if row['kind']=='native')
  assert results[0]['exited_within_5_seconds'] and results[0]['exit_code']==-signal.SIGINT, results[0]
  report={'result':'PASS' if passed else 'FAIL','binary_sha256':hashlib.sha256(args.binary.read_bytes()).hexdigest(),'source_sha256':hashlib.sha256((ROOT/'openpilot/system/tombstoned.py').read_bytes()).hexdigest(),'cases':results};(args.output/'manifest.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2))
  if not args.observe:assert passed,'native stop budget or owned child cleanup failed'

if __name__=='__main__':main()
