from __future__ import annotations

import argparse
from pathlib import Path
from card_qa.ci import require_space


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--path', type=Path, required=True)
  parser.add_argument('--growth-mib', type=int, required=True)
  arguments = parser.parse_args()
  if arguments.growth_mib < 0:
    parser.error('growth must be nonnegative')
  require_space(arguments.path, arguments.growth_mib * 1024**2)


if __name__ == '__main__':
  main()
