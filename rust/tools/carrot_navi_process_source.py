from __future__ import annotations

import argparse
from pathlib import Path
import sys

from card_runtime_source import load_binding


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binding', type=Path, required=True)
  parser.add_argument('--params-root', type=Path, required=True)
  args, original_args = parser.parse_known_args()
  load_binding(args.binding)
  from openpilot.common import params
  original_params = params.Params
  params.Params = lambda: original_params(str(args.params_root))
  from openpilot.selfdrive.carrot.carrot_navi import main as original_main
  sys.argv = [sys.argv[0], *original_args]
  original_main()


if __name__ == '__main__':
  main()
