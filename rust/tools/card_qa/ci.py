from __future__ import annotations

import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[3]
TOOLS = ROOT / 'rust/tools'


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


def run(command: list[str], evidence: Path, name: str) -> None:
  evidence.mkdir(parents=True, exist_ok=True)
  invocation = {'argv': command, 'cwd': str(ROOT)}
  (evidence / f'{name}.command.json').write_text(json.dumps(invocation, indent=2) + '\n')
  result = subprocess.run(command, cwd=ROOT, text=True, capture_output=True, check=False)
  (evidence / f'{name}.stdout').write_text(result.stdout)
  (evidence / f'{name}.stderr').write_text(result.stderr)
  (evidence / f'{name}.exit.json').write_text(json.dumps({'returncode': result.returncode}) + '\n')
  if result.returncode:
    print(result.stdout + result.stderr, file=sys.stderr)
  result.check_returncode()
  print(f'PASS {name}', flush=True)


def source_command(script: Path, arguments: list[str], dbc: Path) -> list[str]:
  bootstrap = '; '.join(['import runpy, sys', 'from can_source import load', 'load()', 'import opendbc.can.dbc as dbc',
    'dbc.DBC_PATH = sys.argv[1]', 'dbc.DBC.cache_clear()', 'sys.argv = sys.argv[2:]', 'runpy.run_path(sys.argv[0], run_name="__main__")'])
  return [sys.executable, '-P', '-c', bootstrap, str(dbc), str(script), *arguments]


def hashes(paths: list[Path]) -> dict[str, str]:
  return {str(path.relative_to(ROOT)): hashlib.sha256(path.read_bytes()).hexdigest() for path in sorted(set(paths))}
