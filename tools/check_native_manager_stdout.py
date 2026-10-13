"""Compare the native stdout boundary with the original source function on owned pipes."""
from __future__ import annotations
import argparse
import ast
import fcntl
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import time


def run(root: Path, native: Path, helper: Path, output: Path) -> None:
  output.mkdir(parents=True, exist_ok=False)
  source = root / 'openpilot/system/manager/helpers.py'
  function = next(node for node in ast.parse(source.read_text()).body if isinstance(node, ast.FunctionDef) and node.name == 'unblock_stdout')
  wrapper = output / 'source_boundary.py'
  wrapper.write_text('import errno,fcntl,os,sys,signal,runpy\n' + ast.unparse(function) + '\nunblock_stdout()\nrunpy.run_path(sys.argv[1], run_name="__main__")\n')
  body = output / 'owned_body.py'
  body.write_text('''import fcntl,json,os,signal,sys,time
from pathlib import Path
root=Path(sys.argv[-1]); received=[]
signal.signal(signal.SIGINT,lambda value,frame: received.append(value))
signal.signal(signal.SIGTERM,lambda value,frame: received.append(value))
record=dict(pid=os.getpid(),sid=os.getsid(0),pgid=os.getpgrp(),foreground=os.tcgetpgrp(0),stdio=[os.isatty(i) for i in range(3)],blocking=[not(fcntl.fcntl(i,fcntl.F_GETFL)&os.O_NONBLOCK) for i in range(3)])
for _ in range(256): os.write(1,b'x'*4096)
(root/'ready.json').write_text(json.dumps(record))
while not received: time.sleep(.01)
time.sleep(3)
(root/'exit.json').write_text(json.dumps(dict(signal=received[0],pid=os.getpid(),cleanup_complete=True)))
raise SystemExit(7)
''')
  rows = []
  for implementation in ('source', 'native'):
    for recipient in ('full', 'closed'):
      for name, signum in (('INT', signal.SIGINT), ('TERM', signal.SIGTERM)):
        case = output / f'{implementation}-{recipient}-{name}'
        case.mkdir()
        reader, writer = os.pipe()
        if recipient == 'full':
          fcntl.fcntl(writer, fcntl.F_SETFL, os.O_NONBLOCK)
          while True:
            try: os.write(writer, b'F'*4096)
            except BlockingIOError: break
          fcntl.fcntl(writer, fcntl.F_SETFL, 0)
        else:
          os.close(reader); reader = None
        command = ['/usr/bin/python3', str(wrapper), str(body), str(case)] if implementation == 'source' else [str(native), str(helper), str(root), '/usr/bin/python3', str(body), str(case)]
        with (case / 'stderr.log').open('wb') as errors:
          child = subprocess.Popen(command, stdout=writer, stderr=errors)
          os.close(writer)
          try:
            deadline = time.monotonic() + 15
            while not (case / 'ready.json').exists():
              assert child.poll() is None, (command, (case/'stderr.log').read_text())
              assert time.monotonic() < deadline, command
              time.sleep(.01)
            ready = json.loads((case/'ready.json').read_text())
            assert ready['pid'] == ready['sid'] == ready['pgid'] == ready['foreground'], ready
            assert ready['stdio'] == ready['blocking'] == [True]*3, ready
            started = time.monotonic(); child.send_signal(signum)
            status = child.wait(timeout=15); elapsed = time.monotonic()-started
            finish = json.loads((case/'exit.json').read_text())
            assert finish['signal'] == signum and finish['cleanup_complete'], finish
            assert status == 7 and elapsed >= 3, (status, elapsed)
            assert not Path(f"/proc/{ready['pid']}").exists(), ready
            rows.append(dict(implementation=implementation,recipient=recipient,signal=name,command=command,exit_code=status,cleanup_seconds=elapsed,child=ready,output_bytes=1048576,reaped=True))
          finally:
            if child.poll() is None: child.terminate(); child.wait(timeout=15)
            if reader is not None: os.close(reader)
  (output/'result.json').write_text(json.dumps(dict(source_sha256=hashlib.sha256(source.read_bytes()).hexdigest(),native_sha256=hashlib.sha256(native.read_bytes()).hexdigest(),helper_sha256=hashlib.sha256(helper.read_bytes()).hexdigest(),scenarios=rows),indent=2)+'\n')
  print(json.dumps(dict(result='PASS',scenarios=len(rows),output=str(output/'result.json'))))

if __name__ == '__main__':
  parser=argparse.ArgumentParser(); parser.add_argument('--root',type=Path,required=True); parser.add_argument('--native',type=Path,required=True); parser.add_argument('--helper',type=Path,required=True); parser.add_argument('--output',type=Path,required=True)
  args=parser.parse_args(); run(args.root.resolve(),args.native.resolve(),args.helper.resolve(),args.output.resolve())
