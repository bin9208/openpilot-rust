"""Run unmodified original plannerd.main against real Params and original C++ msgq."""

import argparse
from pathlib import Path
import sys
import types

from card_runtime_source import load_binding
from generate_selfdrive_alerts import load_source
from plannerd_owner_loader import register_solvers


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binding', type=Path, required=True)
  parser.add_argument('--source-native', type=Path, required=True)
  args = parser.parse_args()
  load_binding(args.binding)
  events = types.ModuleType('openpilot.selfdrive.selfdrived.events')
  events.Events = load_source('tici')['Events']
  sys.modules[events.__name__] = events
  register_solvers(args.source_native.resolve())
  from openpilot.selfdrive.controls.plannerd import main as original_main

  original_main()


if __name__ == '__main__':
  main()
