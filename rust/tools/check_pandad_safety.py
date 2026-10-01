import argparse
import hashlib
import itertools
import json
from pathlib import Path
import random
import subprocess


def invoke(command, rows, output, label):
  payload = ''.join(json.dumps(row) + '\n' for row in rows)
  (output / f'{label}.input.jsonl').write_text(payload)
  process = subprocess.run(command, input=payload, text=True, capture_output=True, timeout=90)
  (output / f'{label}.stdout').write_text(process.stdout)
  (output / f'{label}.stderr').write_text(process.stderr)
  assert process.returncode == 0, (label, process.returncode, process.stderr[-2000:])
  result = [json.loads(line) for line in process.stdout.splitlines()]
  assert len(result) == len(rows), (label, len(result), len(rows))
  return result


def configurations():
  models = [0, 1, 3, 17, 28, 35, 65535]
  result = [{'op': 'car_params', 'alternative': alternative,
             'configs': [[models[(index * 3 + count) % len(models)], (65535 - index * 100) & 65535] for index in range(count)]}
            for count in range(6) for alternative in (-32768, -1, 0, 1, 32767)]
  result += [{'op': 'car_params', 'alternative': -1, 'configs': [[model, 65535]]} for model in [*range(65), 65535]]
  return result


def operation(onroad=True, **params):
  return {'onroad': onroad, 'params': {key: list(value) if isinstance(value, bytes) else value for key, value in params.items()}}


def cases(encoded):
  rows = []

  def add(label, pandas, operations):
    rows.append({'op': 'safety', 'label': label, 'pandas': pandas, 'operations': operations})

  for index, wire in enumerate(encoded[:30]):
    for pandas in (0, 1, 2, 4):
      add(f'lifecycle-{index}-{pandas}', pandas, [
        operation(False), operation(CarParams=wire, FirmwareQueryDone=b'0', ControlsReady=b'0', ObdMultiplexingEnabled=b'0'),
        operation(), operation(ObdMultiplexingEnabled=b'1'), operation(), operation(FirmwareQueryDone=b'1'), operation(),
        operation(ControlsReady=b'1'), operation(ObdMultiplexingEnabled=b'0', FirmwareQueryDone=b'0', ControlsReady=b'0', CarParams=encoded[-1]),
        operation(False), operation(), operation(ObdMultiplexingEnabled=b'1', FirmwareQueryDone=b'1', ControlsReady=b'1', CarParams=[255] * 8),
        operation(CarParams=wire), operation(False), operation(False, ObdMultiplexingEnabled=b'1'),
      ])
  for index, wire in enumerate(encoded[30:]):
    add(f'model-ordinal-{index}', 2, [operation(CarParams=wire, FirmwareQueryDone=b'1', ControlsReady=b'1'), operation()])
  for index, values in enumerate(itertools.product((None, b'', b'0', b'1', b'true', b'01', b'1\0'), repeat=3)):
    add(f'bool-bytes-{index}', 2, [operation(CarParams=encoded[14], **dict(zip(
      ('FirmwareQueryDone', 'ControlsReady', 'ObdMultiplexingEnabled'), values, strict=True))), operation()])
  wire = encoded[14]
  add('complete-after-empty', 2, [operation(FirmwareQueryDone=b'1', ControlsReady=b'1'), operation(CarParams=wire)])
  for length in range(len(wire)):
    add(f'truncated-{length}', 2, [operation(CarParams=wire[:length], FirmwareQueryDone=b'1', ControlsReady=b'1'),
                                 operation(CarParams=wire)])
  for value in ([0], [255], [0] * 8, [255] * 8, list(b'broken CarParams')):
    add(f'malformed-{value[:3]}-{len(value)}', 1, [operation(CarParams=value, FirmwareQueryDone=b'1', ControlsReady=b'1')])
  rng = random.Random(175)
  for sequence in range(50):
    operations = []
    for _ in range(25):
      params = {key: rng.choice((None, b'0', b'1', b'true')) for key in
                ('FirmwareQueryDone', 'ControlsReady', 'ObdMultiplexingEnabled', 'ObdMultiplexingChanged') if rng.randrange(3) == 0}
      if rng.randrange(5) == 0:
        params['CarParams'] = rng.choice(encoded)
      operations.append(operation(rng.randrange(5) != 0, **params))
    add(f'seeded-{sequence}', rng.randrange(5), operations)
  return rows


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--source', type=Path, required=True)
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--qemu', type=Path)
  parser.add_argument('--sysroot', type=Path)
  args = parser.parse_args()
  if (args.qemu is None) != (args.sysroot is None):
    parser.error('--qemu and --sysroot must be supplied together')
  args.output.mkdir(parents=True, exist_ok=False)
  source_command = [args.source.resolve()]
  native_command = ([args.qemu.resolve(), '-L', args.sysroot.resolve()] if args.qemu is not None else []) + [args.binary.resolve()]
  encoded = invoke(source_command, configurations(), args.output, 'car-params')
  rows = cases(encoded)
  expected = invoke(source_command, [row | {'params_root': str(args.output.resolve() / 'source-params')} for row in rows], args.output, 'source')
  actual = invoke(native_command, [row | {'params_root': str(args.output.resolve() / 'native-params')} for row in rows], args.output, 'native')
  guard_labels = {f'truncated-{length}' for length in range(len(encoded[14]) - 8, len(encoded[14]))}
  recovery = expected[next(index for index, row in enumerate(rows) if row['label'] == 'complete-after-empty')]['results'][1]
  guards = []
  for index, (wanted, got) in enumerate(zip(expected, actual, strict=True)):
    if rows[index]['label'] in guard_labels:
      original, original_next = wanted['results']
      rejected, recovered = got['results']
      assert not original['failed'] and original['state']['safety_configured'], (index, original)
      assert original_next['commands'] == [] and original_next['logs'] == [] and not original_next['failed'], original_next
      length = len(rows[index]['operations'][0]['params']['CarParams'])
      assert rejected == {
        'failed': True, 'changed': [],
        'commands': [{'panda': panda, 'request': 0xdc, 'value': 3, 'index': 1} for panda in (0, 1)],
        'logs': [{'level': 30, 'message': 'Finished FW query, Waiting for params to set safety model'},
                 {'level': 30, 'message': f'got {length} bytes CarParams'}],
        'state': {'initialized': True, 'log_once': True, 'prev_obd_multiplexing': False, 'safety_configured': False},
      }, (index, rejected)
      assert recovered == recovery, (index, recovered, recovery)
      guards.append({'label': rows[index]['label'], 'source': wanted, 'native': got})
      continue
    if wanted != got:
      (args.output / 'mismatch.json').write_text(json.dumps({'index': index, 'input': rows[index], 'expected': wanted, 'actual': got}, indent=2))
      raise AssertionError((index, rows[index]['label'], args.output / 'mismatch.json'))
  assert len(guards) == 8
  (args.output / 'intentional-truncation-differences.json').write_text(json.dumps(guards, indent=2) + '\n')
  report = {'result': 'pass', 'scenarios': len(rows), 'steps': sum(len(row['operations']) for row in rows),
            'exact_state_commands_logs_params_and_wire_errors_scenarios': len(rows) - len(guards),
            'issue_176_explicit_rejection_and_exact_recovery_scenarios': len(guards),
            'source_binary_sha256': hashlib.sha256(args.source.read_bytes()).hexdigest(),
            'native_binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest(),
            'native_command': list(map(str, native_command)),
            'scope': 'unchanged C++ PandaSafety and Params compared with native policy and physical Params; transport recorder, no device'}
  (args.output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
  print(json.dumps(report))


if __name__ == '__main__':
  main()
