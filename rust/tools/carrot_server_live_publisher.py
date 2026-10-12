# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# Owned synthetic IPC publisher launched by carrot_server_live.py; no device endpoints.
from __future__ import annotations
import json
import os
from pathlib import Path
import sys
from openpilot.cereal import messaging


def main() -> None:
  config = json.loads(sys.stdin.readline())
  sockets = {name: messaging.pub_sock(name) for name in config['services']}
  print(json.dumps({'pid': os.getpid()}), flush=True)
  for line in sys.stdin:
    if not line.strip():
      break
    command = json.loads(line)
    for row in command['frames']:
      sockets[row['service']].send(Path(row['path']).read_bytes())
    print(
      json.dumps({'sent': len(command['frames']), 'readers_updated': {name: sockets[name].all_readers_updated() for name in command.get('readers', [])}}),
      flush=True,
    )
  sockets.clear()


if __name__ == '__main__':
  main()
