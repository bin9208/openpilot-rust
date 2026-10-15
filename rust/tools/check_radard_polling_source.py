"""Compare native subscriptions with the exact pinned Bosch polling function."""

import argparse
import __future__
import ast
from enum import IntFlag
import itertools
import json
from pathlib import Path
import subprocess
from types import SimpleNamespace

from radar_final_source import source


def definitions(path, names, globals_):
  nodes = [n for n in ast.parse(source(path)).body if isinstance(n, (ast.ClassDef, ast.FunctionDef)) and n.name in names]
  assert {n.name for n in nodes} == set(names)
  exec(compile(ast.Module(body=nodes, type_ignores=[]), 'e8e152db:' + path, 'exec', flags=__future__.annotations.compiler_flag), globals_)


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  values = {'IntFlag': IntFlag}
  definitions('opendbc_repo/opendbc/car/hyundai/values.py', ['HyundaiExtFlags'], values)
  definitions('openpilot/selfdrive/carrot/radar/__init__.py', ['effective_radar_track_mode'], values)
  definitions('openpilot/selfdrive/carrot/radar/radard_dpath.py', ['bosch_radar_tracks_active'], values)
  cases, expected = [], []
  for brand, ext, unavailable, mode in itertools.product(
    ['hyundai', 'toyota', 'volkswagen', 'mazda'], [0, 1 << 17, 1 << 18, (1 << 17) | (1 << 18)], [False, True], [-2, -1, 0, 1, 2, 3]
  ):
    cases.append({'brand': brand, 'ext': ext, 'unavailable': unavailable, 'mode': mode})
    cp = SimpleNamespace(brand=brand, extFlags=ext, radarUnavailable=unavailable)
    effective = values['effective_radar_track_mode'](brand, unavailable, mode)
    active = values['bosch_radar_tracks_active'](cp, effective)
    expected.append(['modelV2', 'liveTracks'] if active else ['modelV2'])
  result = subprocess.run([str(args.binary.resolve())], input=json.dumps(cases), text=True, capture_output=True, check=True)
  assert json.loads(result.stdout) == expected
  (args.output / 'input.json').write_text(json.dumps(cases) + '\n')
  (args.output / 'actual.json').write_text(result.stdout)
  (args.output / 'receipt.json').write_text(
    json.dumps({'source_commit': 'e8e152dbcfd9574a1791ed12026d1880e87689b3', 'cases': len(cases), 'status': 'PASS'}) + '\n'
  )
  print(json.dumps({'cases': len(cases), 'status': 'PASS'}))


if __name__ == '__main__':
  main()
