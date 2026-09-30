#!/usr/bin/env python3
import importlib.util
import json
from pathlib import Path
import struct
import sys
import time


def main() -> None:
  path = Path(__file__).resolve().parents[2] / 'openpilot/system/manager/update_status.py'
  spec = importlib.util.spec_from_file_location('original_checkout_status', path)
  assert spec is not None and spec.loader is not None
  source = importlib.util.module_from_spec(spec)
  spec.loader.exec_module(source)
  repo = Path(json.loads(sys.stdin.readline())['repo'])
  status = None
  print(json.dumps({'ready': True}), flush=True)
  for line in sys.stdin:
    request = json.loads(line)
    start = time.monotonic()
    commit = returned = None
    match request['op']:
      case 'read':
        commit = source.read_checkout_commit(repo)
      case 'capture':
        status = source.UpdateStatus(str(repo))
      case 'update':
        assert status is not None
        now = struct.unpack('<d', struct.pack('<Q', request['now_bits']))[0]
        returned = status.update(now)
      case other:
        raise ValueError(other)
    elapsed = time.monotonic() - start
    print(json.dumps({'commit': commit, 'returned': returned, 'elapsed': elapsed,
                      'running_commit': status.running_commit if status is not None else None,
                      'reboot_required': status.reboot_required if status is not None else False}), flush=True)


if __name__ == '__main__':
  main()
