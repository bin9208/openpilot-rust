# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path

from carrot_server_heartbeat_driver import ROOT
from carrot_server_heartbeat_protocol import run as protocol
from carrot_server_heartbeat_lifecycle import run as lifecycle


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--case', action='append', default=[])
  parser.add_argument('--group', choices=('protocol', 'lifecycle'), default='protocol')
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  runner = protocol if args.group == 'protocol' else lifecycle
  results = runner(args.binary, args.output, tuple(args.case))
  identity = dict(binary=dict(path=str(args.binary), sha256=hashlib.sha256(args.binary.read_bytes()).hexdigest()) if args.binary else None, source={str(path.relative_to(ROOT)): hashlib.sha256(path.read_bytes()).hexdigest() for path in (ROOT / 'openpilot/selfdrive/carrot/server/services/heartbeat.py', ROOT / 'openpilot/selfdrive/carrot/server/features/system.py', ROOT / 'openpilot/selfdrive/carrot/server/app.py')})
  (args.output / 'identity.json').write_text(json.dumps(identity, indent=2)+'\n')
  raise SystemExit(0 if not args.binary or all(row['passed'] for row in results) else 1)


if __name__ == '__main__':
  main()
