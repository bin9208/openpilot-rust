"""This executable never invokes sudo, PPP, IP, systemctl, resolvectl or network APIs."""

import json
import os
from pathlib import Path
import signal
import sys
import time


def main():
  root = Path(sys.argv[1])
  args = sys.argv[2:]
  with (root / 'calls.jsonl').open('a') as output:
    output.write(json.dumps(args) + '\n')
  if args[0] == 'pppd':
    (root / 'ppp.pid').write_text(str(os.getpid()))
    if (root / 'fail-ppp').exists():
      return 1
    while root.exists():
      time.sleep(0.05)
    return 0
  if args[0] == 'killall':
    path = root / 'ppp.pid'
    if path.exists():
      try:
        os.kill(int(path.read_text()), signal.SIGKILL)
      except ProcessLookupError:
        pass
    return 0
  if args[:3] == ['ip', 'rule', 'del']:
    return 1
  if args[:3] == ['ip', 'route', 'add'] and (root / 'fail-route').exists():
    return 1
  if args[:2] == ['resolvectl', 'dns'] and (root / 'fail-dns').exists():
    return 1
  if args[0] == '-4':
    if not (root / 'no-ip').exists():
      print('inet 10.0.0.2 peer 10.0.0.1/32 scope global ppp0')
  return 0


if __name__ == '__main__':
  sys.exit(main())
