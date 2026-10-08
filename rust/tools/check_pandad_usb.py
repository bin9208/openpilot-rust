import argparse
import hashlib
import json
from pathlib import Path
import subprocess

from pandad_usb_cases import cases, device


def invoke(command, payload, output, label):
  result = subprocess.run(list(map(str, command)), input=payload, text=True, capture_output=True, timeout=90)
  (output / f'{label}.stdout').write_text(result.stdout)
  (output / f'{label}.stderr').write_text(result.stderr)
  assert result.returncode == 0, (label, result.returncode, result.stderr[-2000:])
  return [json.loads(line) for line in result.stdout.splitlines()]


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--source', type=Path, required=True)
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--fixture', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--qemu', type=Path)
  parser.add_argument('--sysroot', type=Path)
  parser.add_argument('--native-preload', type=Path)
  args = parser.parse_args()
  if (args.qemu is None) != (args.sysroot is None):
    parser.error('provide both --qemu and --sysroot')
  args.output.mkdir(parents=True, exist_ok=False)
  rows = cases()
  payload = ''.join(json.dumps(row) + '\n' for row in rows)
  (args.output / 'inputs.jsonl').write_text(payload)
  native = ([args.qemu.resolve(), '-L', args.sysroot.resolve()] if args.qemu else []) + [args.binary.resolve(), args.fixture.resolve()]
  if args.native_preload:
    native = ['env', f'LD_PRELOAD={args.native_preload.resolve()}', *native]
  source = invoke([args.source.resolve()], payload, args.output, 'source')
  actual = invoke(native, payload, args.output, 'native')
  assert len(source) == len(actual) == len(rows)
  for index, (expected, got) in enumerate(zip(source, actual, strict=True)):
    if rows[index].get('mode') == 'concurrent':
      assert expected['results'] == [{'metrics': {'active': 0, 'maximum': 1, 'total': rows[index]['threads'] * rows[index]['transfers']},
                                     'errors': 0, 'connected': True, 'healthy': True}], expected
    if expected != got:
      (args.output / 'mismatch.json').write_text(json.dumps({'label': rows[index]['label'], 'input': rows[index],
                                                          'expected': expected, 'actual': got}, indent=2) + '\n')
      raise AssertionError((index, rows[index]['label'], args.output / 'mismatch.json'))
  initialization_failures = []
  for code in (-1, -3, -99, 1):
    fixture_rows = [{'mode': 'list', 'init': code, 'devices': [device()], 'repeats': 2},
                    {'mode': 'list', 'devices': [device()], 'repeats': 2}]
    fixture_payload = ''.join(json.dumps(row) + '\n' for row in fixture_rows)
    (args.output / f'list-init-{code}.jsonl').write_text(fixture_payload)
    expected = invoke([args.source.resolve()], fixture_payload, args.output, f'list-init-{code}-source')
    got = invoke(native, fixture_payload, args.output, f'list-init-{code}-native')
    assert expected == got
    assert expected[1]['calls'] == [] and expected[1]['results'] == [[], []], expected
    initialization_failures.append(code)
  unlocked = {'mode': 'concurrent_unlocked', 'threads': 8, 'transfers': 16, 'devices': [device()]}
  (args.output / 'concurrent-unlocked.json').write_text(json.dumps(unlocked) + '\n')
  observed = invoke([args.source.resolve()], json.dumps(unlocked) + '\n', args.output, 'concurrent-unlocked')[0]
  assert observed['results'][0]['metrics']['maximum'] > 1, observed
  assert observed['results'][0]['metrics']['total'] == 128, observed
  report = {'result': 'pass', 'scenarios': len(rows), 'operations': sum(len(row['operations']) for row in rows),
            'connection_failures': sum(row['failed'] for row in source), 'exact_calls_logs_buffers_results_health_serial_cleanup': True,
            'enumeration_initialization_failures_not_retried': initialization_failures,
            'mutex_serialized_real_threads': [2, 4, 8], 'unlocked_fixture_max_overlap': observed['results'][0]['metrics']['maximum'],
            'source_sha256': hashlib.sha256(args.source.read_bytes()).hexdigest(), 'native_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest(),
            'fixture_sha256': hashlib.sha256(args.fixture.read_bytes()).hexdigest(), 'native_command': list(map(str, native)),
            'scope': 'unchanged original USB code versus Rust using a scripted libusb ABI; no physical USB or vehicle'}
  (args.output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
  print(json.dumps(report))


if __name__ == '__main__':
  main()
