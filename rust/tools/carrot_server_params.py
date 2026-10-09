import argparse
import json
import resource
from pathlib import Path
import subprocess
import tempfile

from original_params_binding import load


def cases():
  dates = ('2026-10-08', '20261008', '2026-W41', '2026W414',
           '2026-10-08T12:34:56', '20261008x123456', '2026-W41-4T12:34:56',
           '2026-10-08T12:34:56Z', '2026-10-08T12:34:56+09:00',
           '2026-10-08T12:34:56.123456789-02:03:04.567890',
           '2026W41T12+00', '2026-10-08+12:34', '2026-10-08T12:34:56+00:00:00.5',
           '2026W41000+00', '2026-W41-12+00',
           '2026-10-08T12:34:56+24:00', '2026-02-30T12:00', 'invalid', '')
  for raw in dates:
    yield {'action': 'get_param', 'name': 'InstallDate', 'raw': raw.encode().hex(), 'default': 'fallback'}
  for name, raw, default in (
    ('IsMetric', b'1', False), ('IsMetric', b'true', True),
    ('LongitudinalPersonalityMax', b'14tail', 9),
    ('LongitudinalPersonalityMax', b'', 9),
    ('UptimeOnroad', b'0.1junk', 9),
    ('UptimeOnroad', b'', 9),
    ('CarName', b'\xff', 'fallback'), ('CarParamsPersistent', b'\xff\xfe', 'fallback'),
    ('LiveParameters', b'null', 'fallback'), ('LiveParameters', b'{invalid', 'fallback'),
    ('LiveParameters', b'{"ordered": [true, null, 2]}', 'fallback'),
  ):
    yield {'action': 'get_param', 'name': name, 'raw': raw.hex(), 'default': default}
  for name, values in (
    ('InstallDate', ('2026-10-08T12:34:56', 42, None, True, {'ordered': [1, 2]})),
    ('CarParamsPersistent', ('raw', 1, None)),
    ('LiveParameters', ('null', '[]', {'ordered': [1, 2]}, 1, True)),
    ('UptimeOnroad', (0.1, -0.0, 1e100)),
    ('LongitudinalPersonalityMax', (14.999, 2.5, 1e100)),
  ):
    for value in values:
      yield {'action': 'put_param', 'name': name, 'value': value}
  for value in (14.999, 0.5, 'invalid'):
    yield {'action': 'put_param', 'name': 'FutureSetting', 'value': value,
           'setting': {'min': 0, 'max': 60, 'default': 0}}
  for name, raw in (
    ('IsMetric', b''), ('IsMetric', b'1'),
    ('LongitudinalPersonalityMax', b'14tail'),
    ('LongitudinalPersonalityMax', b'1_000'),
    ('LongitudinalPersonalityMax', '１４'.encode()),
    ('LongitudinalPersonalityMax', b'\x1c14'),
    ('UptimeOnroad', b'0.1junk'), ('UptimeOnroad', b'0.1'),
    ('UptimeOnroad', b'NaN'), ('UptimeOnroad', b'1e1000'),
    ('LanguageSetting', b'\xff'), ('LanguageSetting', 'main_zh-CHS'.encode()),
    ('InstallDate', b'2026-10-08T12:34:56+09:00'),
  ):
    yield {'action': 'get_backup', 'name': name, 'raw': raw.hex()}
  yield {'action': 'put_param', 'name': 'UnknownWithoutDefinition', 'value': 1}


def compare(binary: Path, binding: Path, output: Path):
  output.mkdir(parents=True, exist_ok=True)
  with tempfile.TemporaryDirectory(prefix='carrot-params-owned-') as temporary:
    root = Path(temporary)
    load(binding, f'ipc://{root}/logs.sock', output / 'binding-logs')
    from openpilot.common.params import Params
    from openpilot.selfdrive.carrot.server.services import params
    inputs, expected, stores = [], [], []
    for index, case in enumerate(cases()):
      source = Params(str(root / f'source-{index}'))
      native = Params(str(root / f'native-{index}'))
      params.Params = lambda: source
      case = dict(case)
      case['root'] = str(root / f'native-{index}')
      with (output / 'inputs.jsonl').open('a') as record:
        record.write(json.dumps(case) + '\n')
      if 'raw' in case:
        for store in (source, native):
          Path(store.get_param_path(case['name'])).write_bytes(bytes.fromhex(case['raw']))
      try:
        if case['action'] == 'get_backup':
          result = params.get_all_param_values_for_backup()
        elif case['action'] == 'get_param':
          result = params.get_param_value(case['name'], case['default'])
        else:
          result = params.set_param_value(case['name'], case['value'], case.get('setting'))
      except Exception as error:
        result = {'error': str(error)}
      inputs.append(case)
      expected.append(json.dumps(result))
      (output / 'original.jsonl').write_text('\n'.join(expected) + '\n')
      path = Path(source.get_param_path(case['name']))
      stores.append({'original': path.read_bytes().hex() if path.exists() else None,
                     'native_path': native.get_param_path(case['name'])})
    request = ''.join(json.dumps(case) + '\n' for case in inputs)
    (output / 'inputs.jsonl').write_text(request)
    (output / 'original.jsonl').write_text('\n'.join(expected) + '\n')
    result = subprocess.run([str(binary)], input=request, text=True, capture_output=True)
    (output / 'native.jsonl').write_text(result.stdout)
    (output / 'native.stderr').write_text(result.stderr)
    result.check_returncode()
    for store in stores:
      path = Path(store.pop('native_path'))
      store['native'] = path.read_bytes().hex() if path.exists() else None
    (output / 'stores.json').write_text(json.dumps(stores, indent=2) + '\n')
    actual = result.stdout.splitlines()
    mismatches = [index for index, (old, new) in enumerate(zip(expected, actual))
                  if (json.loads(old) != json.loads(new) if inputs[index]['action'] == 'get_backup' else old != new)]
    raw_mismatches = [index for index, store in enumerate(stores) if store['original'] != store['native']]
    receipt = {'cases': len(inputs), 'mismatches': mismatches, 'raw_mismatches': raw_mismatches,
               'backup_comparison': 'actual JSON object mapping; raw unordered source traversal retained',
               'passed': len(actual) == len(expected) and not mismatches and not raw_mismatches}
    (output / 'result.json').write_text(json.dumps(receipt) + '\n')
    assert receipt['passed'], receipt
    print(json.dumps(receipt))


def main():
  resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', required=True, type=Path)
  parser.add_argument('--binding', required=True, type=Path)
  parser.add_argument('--output', required=True, type=Path)
  args = parser.parse_args()
  compare(args.binary, args.binding, args.output)


if __name__ == '__main__':
  main()
