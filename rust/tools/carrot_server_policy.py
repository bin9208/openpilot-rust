import argparse
from copy import deepcopy
import json
from pathlib import Path
import subprocess

from openpilot.selfdrive.carrot.server.services import params, settings


def source(case):
  if case['action'] == 'coerce':
    kind, value = params._coerce_inferred_value(case['value'], case['setting'])
    return {'kind': kind, 'coerced': value}
  data = deepcopy(case['data'])
  groups, names, group_list = settings.group_index(data)
  categories = settings.build_menu_categories(data, names)
  data, groups, names, group_list = settings.with_vehicle_gap_limits((data, groups, names, group_list), case['maximum'])
  groups, group_list, categories, _ = settings.filter_settings_catalog_for_brand(groups, group_list, categories, case['brand'])
  return {'data': data, 'groups': groups, 'by_name': names, 'groups_list': group_list, 'categories': categories}


def cases():
  data = {'apilot': 'fixture', 'params': [
    {'name': 'z', 'group': 'Z'},
    {'name': 'other'},
    {'name': 'CruiseGapLevels', 'group': 'A', 'min': 2, 'max': 4, 'default': 4,
     'options': {'ko': ['2', '3', '4'], 'en': ['two', 'three', 'four']}},
    {'name': 'detail', 'group': 'A', 'detail_parent': 'CruiseGapLevels', 'hidden_brands': ['HYUNDAI']},
    {'name': 'hidden', 'group': 'A', 'hidden_brands': [' hyundai ']},
  ], 'menu': [{'id': 'top', 'ko': '최상위', 'groups': [
    {'id': 'nested', 'en': 'Nested', 'groups': [{'id': 'outer', 'ko': '바깥', 'groups': [
      {'id': 'inner', 'ko': '안쪽', 'params': ['CruiseGapLevels', 'detail', 'hidden', 'missing']}]}]},
    {'id': 'direct', 'params': ['z', 'other']},
  ]}]}
  for maximum in (3, 4, 5):
    for brand in ('', ' Hyundai ', 'toyota'):
      yield {'action': 'catalog', 'maximum': maximum, 'brand': brand, 'data': data}
  yield {'action': 'catalog', 'maximum': 4, 'brand': '', 'data': {'params': []}}
  shipped = json.loads((Path(__file__).resolve().parents[2] / 'openpilot/selfdrive/carrot_settings.json').read_text())
  for maximum, brand in ((3, ''), (4, 'hyundai'), (4, 'toyota')):
    yield {'action': 'catalog', 'maximum': maximum, 'brand': brand, 'data': shipped}
  for invalid in (None, [], {'params': None}, {'params': [None]}, {'params': [{'group': []}]}):
    yield {'action': 'catalog', 'maximum': 4, 'brand': '', 'data': invalid}
  for definition, values in (
    ({'min': 0, 'max': 60, 'default': 0}, (14.999, '14.999', 0.5, 1.5, -1.5, 2.5, -0.5, float('nan'), float('inf'), float('-inf'), None, 'invalid')),
    ({'min': 0, 'max': 1, 'default': 0}, ('yes', 'off', True, 2, [])),
    ({'min': 0, 'max': 2.5, 'default': 1.0}, ('1.25', 2, False)),
    ({}, ({'ordered': [1, 2]}, '한글', None)),
  ):
    for value in values:
      yield {'action': 'coerce', 'setting': definition, 'value': value}


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  inputs = list(cases())
  request = ''.join(json.dumps(case) + '\n' for case in inputs)
  (args.output / 'inputs.jsonl').write_text(request)
  expected = []
  for case in inputs:
    try:
      payload = source(case)
    except Exception as error:
      payload = {'error': str(error)}
    expected.append(json.dumps(payload))
  (args.output / 'original.jsonl').write_text('\n'.join(expected) + '\n')
  result = subprocess.run([str(args.binary)], input=request, text=True, capture_output=True)
  (args.output / 'native.jsonl').write_text(result.stdout)
  (args.output / 'native.stderr').write_text(result.stderr)
  result.check_returncode()
  actual = result.stdout.splitlines()
  if actual != expected:
    mismatch = next((index for index, pair in enumerate(zip(expected, actual)) if pair[0] != pair[1]), min(len(expected), len(actual)))
    raise AssertionError(f'policy output mismatch at {mismatch}; original/native artifacts retained')
  receipt = {'cases': len(inputs), 'comparison': 'exact ordered Python JSON bytes', 'passed': True}
  (args.output / 'result.json').write_text(json.dumps(receipt) + '\n')
  print(json.dumps(receipt))


if __name__ == '__main__':
  main()
