from __future__ import annotations

import argparse
import json
from pathlib import Path
import shutil


def require_space(path: Path, growth: int) -> dict[str, int]:
  while not path.exists():
    path = path.parent
  free = shutil.disk_usage(path).free
  required = 25 * 1024**3 + growth
  result = {'free_bytes': free, 'required_bytes': required, 'growth_bytes': growth}
  print(json.dumps(result), flush=True)
  if free < required:
    raise OSError(f'require 25 GiB plus estimated growth; recover at least 35 GiB before retry: {result}')
  return result


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
