from __future__ import annotations

import argparse
import ast
import hashlib
import json
from pathlib import Path
import random
import subprocess
import sys

from original_params_binding import load

ROOT = Path(__file__).resolve().parents[2]


def coordinate(latitude: float, longitude: float) -> dict[str, float]:
  return {'latitude': latitude, 'longitude': longitude}


def cases() -> list[dict]:
  output = []
  randomizer = random.Random(196)
  for index in range(120):
    first = coordinate(randomizer.uniform(-80, 80), randomizer.uniform(-179, 179))
    second = coordinate(first['latitude'] + randomizer.uniform(-.1, .1), first['longitude'] + randomizer.uniform(-.1, .1))
    position = coordinate(first['latitude'] + randomizer.uniform(-.2, .2), first['longitude'] + randomizer.uniform(-.2, .2))
    if index % 12 == 0:
      second = dict(first)
    output.extend(({'op': 'distance', 'a': first, 'b': second},
                   {'op': 'minimum', 'a': first, 'b': second, 'position': position},
                   {'op': 'along', 'geometry': [first, second, position, first], 'position': second},
                   {'op': 'along', 'geometry': [first, second], 'position': position}))
  output.extend(({'op': 'along', 'geometry': [], 'position': coordinate(0, 0)},
                 {'op': 'along', 'geometry': [coordinate(0, 0)], 'position': coordinate(0, -1)},
                 {'op': 'distance', 'a': coordinate(90, 0), 'b': coordinate(-90, 0)},
                 {'op': 'distance', 'a': coordinate(0, 179.999), 'b': coordinate(0, -179.999)}))
  for length, maximum in ((0, 0), (1, 0), (6, 1), (6, 3), (8, 3), (6, 6), (4095, 4096), (4096, 4096), (5003, 4096)):
    output.append({'op': 'limit', 'points': list(range(length)), 'maximum': maximum})
  for text in ('left', 'right', 'straight', 'slight left', 'slight right', 'slight straight',
               'left right', 'right straight', 'uturn', '', 'Left'):
    output.append({'op': 'direction', 'value': text})
  banners = [
    {'distanceAlongGeometry': 800, 'primary': {'text': 'Far', 'type': 'turn', 'modifier': 'right'}},
    {'distanceAlongGeometry': 100, 'primary': {'text': 'Near', 'type': 'exit', 'modifier': 'slight right'},
     'secondary': {'text': 'Exit 3'}, 'sub': {'components': [{'type': 'text', 'text': 'Exit'},
       {'type': 'lane', 'active': False, 'directions': ['left', 'straight']},
       {'type': 'lane', 'active': True, 'directions': ['slight right'], 'active_direction': 'right'}]}},
  ]
  for distance in (-100., 0., 99.999, 100., 100.001, 799.999, 800., 800.001, 900.):
    output.append({'op': 'banner', 'banners': banners, 'distance': distance})
    output.append({'op': 'banner', 'banners': banners[::-1], 'distance': distance})
  for banner in ([], [{'distanceAlongGeometry': 100, 'primary': {}, 'secondary': None, 'sub': None}],
                 [{'distanceAlongGeometry': 100, 'primary': {'text': None, 'type': None, 'modifier': None}}],
                 [{'distanceAlongGeometry': 100}],
                 [{'distanceAlongGeometry': 100, 'primary': {}, 'sub': {'components': [{'active': True}]}}]):
    output.append({'op': 'banner', 'banners': banner, 'distance': 1.})
  return output


def source_limit():
  tree = ast.parse((ROOT / 'openpilot/selfdrive/navd/navd.py').read_text())
  function = next(node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == 'limit_route_points')
  constant = next(node for node in tree.body if isinstance(node, ast.Assign) and
                  any(isinstance(target, ast.Name) and target.id == 'NAV_ROUTE_MAX_POINTS' for target in node.targets))
  namespace = {}
  exec(compile(ast.Module(body=[constant, function], type_ignores=[]), 'source-navd-limit', 'exec'), namespace)
  return namespace['limit_route_points']


def run_source(row: dict, helpers, limit) -> dict:
  def point(value):
    return helpers.Coordinate(value['latitude'], value['longitude'])
  try:
    match row['op']:
      case 'distance':
        value = point(row['a']).distance_to(point(row['b']))
      case 'minimum':
        value = helpers.minimum_distance(point(row['a']), point(row['b']), point(row['position']))
      case 'along':
        value = helpers.distance_along_geometry([point(value) for value in row['geometry']], point(row['position']))
      case 'limit':
        value = limit(row['points'], row['maximum'])
      case 'direction':
        value = helpers.string_to_direction(row['value'])
      case 'banner':
        value = helpers.parse_banner_instructions(row['banners'], row['distance'])
      case _:
        raise AssertionError(row['op'])
    return {'ok': True, 'value': value}
  except (KeyError, IndexError, TypeError, ValueError, ZeroDivisionError) as error:
    return {'ok': False, 'error': type(error).__name__}


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--binding', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  load(args.binding.resolve(), 'ipc://' + str((args.output / 'source-log').resolve()), args.output / 'logs')
  from openpilot.selfdrive.navd import helpers
  import numpy
  inputs = cases()
  limit = source_limit()
  source = [run_source(row, helpers, limit) for row in inputs]
  payload = ''.join(json.dumps(row) + '\n' for row in inputs)
  (args.output / 'inputs.jsonl').write_text(payload)
  (args.output / 'source.json').write_text(json.dumps(source, indent=2) + '\n')
  command = [str(args.binary.resolve())]
  process = subprocess.run(command, input=payload, capture_output=True, text=True, check=False, timeout=20)
  (args.output / 'native.jsonl').write_text(process.stdout)
  (args.output / 'native.stderr').write_text(process.stderr)
  native = [json.loads(line) for line in process.stdout.splitlines()]
  failures = []
  for index, (expected, actual) in enumerate(zip(source, native, strict=False)):
    passed = expected['ok'] == actual['ok'] and (not expected['ok'] or expected['value'] == actual['value'])
    if not passed:
      failures.append({'index': index, 'input': inputs[index], 'source': expected, 'native': actual})
  paths = [args.binary, args.binding, Path(__file__), ROOT / 'openpilot/selfdrive/navd/helpers.py', ROOT / 'openpilot/selfdrive/navd/navd.py']
  paths.extend((ROOT / 'rust/crates/navd').rglob('*.rs'))
  passed = process.returncode == 0 and len(source) == len(native) and not failures
  report = {'status': 'PASS' if passed else 'FAIL', 'cases': len(source), 'native_cases': len(native),
            'returncode': process.returncode, 'failures': failures, 'command': command,
            'python': sys.version, 'numpy': numpy.__version__,
            'files': {str(path.resolve()): hashlib.sha256(path.read_bytes()).hexdigest() for path in paths}}
  (args.output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
  print(json.dumps({key: report[key] for key in ('status', 'cases', 'native_cases', 'returncode')}))
  if not passed:
    raise SystemExit(1)


if __name__ == '__main__':
  main()
