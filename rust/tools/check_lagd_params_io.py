#!/usr/bin/env python3
"""Source read_file directory-as-empty behavior at native lagd Params boundaries."""

import argparse
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import tempfile
import time
import uuid
from lagd_frames import car_params


def scenario(binary, key):
  prefix = 'lagdio_' + uuid.uuid4().hex
  shm = Path('/dev/shm/msgq_' + prefix)
  shm.mkdir()
  with tempfile.TemporaryDirectory(prefix='lagd-io-') as temporary:
    root = Path(temporary)
    params = root / prefix
    params.mkdir()
    (params / key).mkdir()
    if key == 'LiveDelay':
      (params / 'CarParams').write_bytes(car_params())
    env = dict(os.environ, PARAMS_ROOT=str(root), OPENPILOT_PREFIX=prefix)
    process = subprocess.Popen([binary, '--frames', '1'], stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env)
    try:
      if key == 'CarParams':
        time.sleep(0.2)
        if process.poll() is None:
          process.send_signal(signal.SIGTERM)
      stdout, stderr = process.communicate(timeout=3)
      return {
        'key': key,
        'returncode': process.returncode,
        'directory_retained': (params / key).is_dir(),
        'stdout': stdout.decode(),
        'stderr': stderr.decode(),
        'pass': process.returncode == 0 and (params / key).is_dir(),
      }
    finally:
      if process.poll() is None:
        process.kill()
      process.wait(timeout=3)
      shutil.rmtree(shm)
      Path('/tmp/logmessage' + prefix).unlink(missing_ok=True)


def main():
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument('binary', type=Path)
  parser.add_argument('output', type=Path)
  args = parser.parse_args()
  records = [scenario(args.binary, key) for key in ['CarParams', 'LiveDelay']]
  args.output.write_text(json.dumps(records, indent=2) + '\n')
  assert all(row['pass'] for row in records), records
  print('PASS: unreadable CarParams waits for signal; unreadable LiveDelay remains absent to source policy')


if __name__ == '__main__':
  main()
