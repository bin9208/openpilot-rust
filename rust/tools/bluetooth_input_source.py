import json
import os
from pathlib import Path
import subprocess
import sys

from bluetooth_engine_source import daemon


def main() -> None:
  path = sys.argv[1]
  before = len(list(Path('/proc/self/fd').iterdir()))
  try:
    fd = daemon.open_input(path)
  except (OSError, subprocess.SubprocessError) as error:
    print(json.dumps({'error': str(error), 'fd_delta': len(list(Path('/proc/self/fd').iterdir())) - before}))
    return
  print(json.dumps({'opened': True, 'fd': fd}), flush=True)
  try:
    for line in sys.stdin:
      assert json.loads(line) == 'read'
      try:
        data = os.read(fd, daemon.EVENT.size * 128)
        if not data or len(data) % daemon.EVENT.size:
          raise OSError('HID device disconnected or incomplete event')
        result = {'events': [{'kind': kind, 'code': code, 'value': value, 'at': sec + usec / 1e6}
                             for sec, usec, kind, code, value in daemon.EVENT.iter_unpack(data)]}
      except BlockingIOError:
        result = {'pending': True}
      except OSError as error:
        result = {'error': str(error)}
      print(json.dumps(result), flush=True)
  finally:
    os.close(fd)
  print(json.dumps({'closed': True, 'fd_delta': len(list(Path('/proc/self/fd').iterdir())) - before}))


if __name__ == '__main__':
  main()
