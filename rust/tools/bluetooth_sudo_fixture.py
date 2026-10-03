#!/usr/bin/python3
import json
import os
from pathlib import Path
import sys
import signal
import time

with Path(os.environ['INPUT_FIXTURE_SUDO_LOG']).open('a') as stream:
  stream.write(json.dumps(sys.argv[1:]) + '\n')
mode = os.environ.get('INPUT_FIXTURE_SUDO_MODE', 'ok')
if mode == 'timeout':
  time.sleep(10)
if mode == 'signal':
  os.kill(os.getpid(), signal.SIGTERM)
if mode == 'loud':
  sys.stdout.write('o' * 1_048_576)
  sys.stderr.write('e' * 1_048_576)
if mode == 'fail-' + sys.argv[2]:
  sys.exit(7)
