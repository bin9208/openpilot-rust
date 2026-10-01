#!/usr/bin/env python3
import json
import os
from pathlib import Path
import subprocess
import sys


def main():
  if os.getsid(0) != os.getpid():
    os.setsid()
  output = Path(os.environ['ATHENA_TRACER_PIDS'])
  helper = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(0.2)'])
  output.write_text(json.dumps({'tracer': os.getpid(), 'helper': helper.pid}))
  helper.wait(timeout=3)
  if os.environ.get('ATHENA_TRACER_FAIL') == '1':
    raise SystemExit(7)
  child = subprocess.Popen([sys.argv[-1]])
  temporary = output.with_suffix('.tmp')
  temporary.write_text(json.dumps({'tracer': os.getpid(), 'helper': helper.pid, 'native': child.pid}))
  temporary.replace(output)
  raise SystemExit(child.wait())


if __name__ == '__main__':
  main()
