import argparse
import hashlib
import json
from pathlib import Path
import resource
import subprocess

from pandad_protocol_cases import alerts, decoding, packing, seeds


def run(command, rows, output, label):
  payload = ''.join(json.dumps(row) + '\n' for row in rows)
  (output / f'{label}.input.jsonl').write_text(payload)
  process = subprocess.run(command, input=payload, text=True, capture_output=True, timeout=45)
  (output / f'{label}.stdout').write_text(process.stdout)
  (output / f'{label}.stderr').write_text(process.stderr)
  assert process.returncode == 0, (label, process.returncode, process.stderr)
  result = [json.loads(line) for line in process.stdout.splitlines()]
  assert len(result) == len(rows), (label, len(result), len(rows))
  for row, value in zip(rows, result, strict=True):
    if row['op'] == 'alerts':
      for state in value['results']:
        state['since'], state['pending_since'] = int(state['since']), int(state['pending_since'])
        state['times'] = [int(time) for time in state['times']]
  return result


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
  resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
  args.output.mkdir(parents=True, exist_ok=False)
  source_command = [args.source.resolve()]
  native_command = ([args.qemu.resolve(), '-L', args.sysroot.resolve()] if args.qemu is not None else []) + [args.binary.resolve()]
  encoded = run(source_command, seeds(), args.output, 'seeds')
  pack_rows = packing()
  rows = pack_rows + decoding(encoded) + alerts()
  expected = run(source_command, rows, args.output, 'source')
  valid_flag_cases = len(encoded) * 8 * 4 * 3
  for index in range(len(pack_rows), len(pack_rows) + valid_flag_cases):
    results = expected[index]['results']
    assert all(row['ok'] and row['resets'] == 0 for row in results), ('invalid flag fixture', index, rows[index], results)
    assert len(results[-1]['frames']) == 1, ('flag fixture lost frame', index, results)
  actual = run(native_command, rows, args.output, 'native')
  for index, (want, got) in enumerate(zip(expected, actual, strict=True)):
    assert want == got, (index, rows[index], want, got)
  invalid = []
  for length in (9, 10, 11, 13, 15, 17, 19, 21, 23, 25, 31, 33, 47, 49, 63, 65, 255):
    row = {'op': 'pack', 'offset': 0, 'frames': [{'address': 1, 'src': 0, 'data': [0] * length}]}
    exits = {}
    for side, command in (('source', source_command), ('native', native_command)):
      process = subprocess.run(command, input=json.dumps(row) + '\n', capture_output=True, text=True, timeout=3)
      (args.output / f'invalid-{length}-{side}.stderr').write_text(process.stderr)
      assert process.returncode != 0 and not process.stdout, (side, length, process.returncode, process.stdout)
      exits[side] = process.returncode
    invalid.append({'length': length, 'exits': exits})
    row['frames'][0]['src'] = 4
    want = run(source_command, [row], args.output, f'ignored-{length}-source')
    got = run(native_command, [row], args.output, f'ignored-{length}-native')
    assert want == got == [{'chunks': []}]
  report = {'result': 'pass', 'cases': len(rows), 'pack_cases': sum(row['op'] == 'pack' for row in rows),
            'decode_cases': sum(row['op'] == 'decode' for row in rows), 'alert_steps': sum(len(row.get('operations', [])) for row in rows),
            'valid_receive_flag_cases': valid_flag_cases, 'invalid_lengths': invalid, 'exact_packets_frames_resets_alert_state': True,
            'source_binary_sha256': hashlib.sha256(args.source.read_bytes()).hexdigest(),
            'native_binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest(),
            'native_command': list(map(str, native_command)),
            'scope': 'unchanged C++ CAN codec and SPI alert tracker; no physical transport or device validation'}
  (args.output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
  print(json.dumps(report))


if __name__ == '__main__':
  main()
