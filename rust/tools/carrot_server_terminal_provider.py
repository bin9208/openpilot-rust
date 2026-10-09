# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Test-only original Python interpreter/module adapter; actual handlers stay original."""

from __future__ import annotations

import json
import os
from pathlib import Path
import sys

from carrot_server_terminal_command_source import configure


class ProtocolMismatch(ValueError):
  pass


def main() -> None:
  args = sys.argv[1:]
  if args[:2] != ['-m', 'selfdrive.carrot.server.terminal_commands.cli']:
    raise ProtocolMismatch('owned terminal provider only accepts the original fixed CLI module')
  config = Path(os.environ['OWNED_TERMINAL_CONFIG'])
  root = Path(json.loads(config.read_text())['owned_root'])
  with (root / 'cli-provider.jsonl').open('a') as stream:
    stream.write(json.dumps({'argv': args, 'cwd': os.getcwd(), 'sid': os.getsid(0), 'pid': os.getpid()}) + '\n')
  cli, _ = configure(config)
  raise SystemExit(cli.main(args[2:]))


if __name__ == '__main__':
  main()
