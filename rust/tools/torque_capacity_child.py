#!/usr/bin/env python3
import ctypes
import json
from pathlib import Path
import resource
import sys
import time


def stage(name: str) -> None:
  print(json.dumps({'stage': name, 'monotonic': time.monotonic()}), flush=True)


def main() -> None:
  resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
  libc = ctypes.CDLL(None, use_errno=True)
  libc.prctl.argtypes = [ctypes.c_int, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_ulong]
  libc.prctl.restype = ctypes.c_int
  if libc.prctl(4, 0, 0, 0, 0) != 0:
    raise OSError(ctypes.get_errno(), 'PR_SET_DUMPABLE failed')
  stage('before_import')
  from openpilot.cereal import messaging

  stage('after_import')
  publisher = messaging.pub_sock('liveTorqueParameters')
  payload = Path(sys.argv[1]).read_bytes()
  stage('before_send')
  publisher.send(payload)


if __name__ == '__main__':
  main()
