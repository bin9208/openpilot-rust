import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess

from pandad_spi_cases import full_scenarios


def normalize(value):
  if isinstance(value, dict): return {key: normalize(item) for key, item in value.items()}
  if isinstance(value, list): return [normalize(item) for item in value]
  if isinstance(value, str): return re.sub(r'argp: 0x[0-9a-f]+', 'argp: <address>', value)
  return value


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--source', type=Path, required=True)
  parser.add_argument('--native', type=Path, required=True)
  parser.add_argument('--runner', nargs=argparse.REMAINDER, default=[])
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  cases = full_scenarios()
  payload = ''.join(json.dumps(case) + '\n' for case in cases)
  (args.output / 'input.jsonl').write_text(payload)
  results = []
  for name, command in [('source', [str(args.source)]), ('native', [*args.runner, str(args.native)])]:
    run = subprocess.run(command, input=payload, text=True, capture_output=True, timeout=120)
    (args.output / f'{name}.jsonl').write_text(run.stdout)
    (args.output / f'{name}.stderr').write_text(run.stderr)
    run.check_returncode()
    results.append([normalize(json.loads(line)) for line in run.stdout.splitlines()])
  source, native = results
  assert len(source) == len(native) == len(cases), (len(source), len(native), len(cases))
  for index, (expected, actual) in enumerate(zip(source, native, strict=True)):
    if expected != actual:
      (args.output / 'mismatch.json').write_text(json.dumps({'index': index, 'name': cases[index]['name'], 'source': expected, 'native': actual}, indent=2) + '\n')
      raise AssertionError(f'SPI mismatch at {index}: {cases[index]["name"]}')
  report = {'status': 'PASS', 'scenarios': len(cases), 'operations': sum(len(case['operations']) for case in cases),
            'syscalls': sum(len(row['calls']) for row in source),
            'scope': 'source-defined TX bytes, all RX results, retry/recovery/locks, logs, timing and SPI error events; unspecified original TX tail bytes represented as null',
            'source_sha256': hashlib.sha256(args.source.read_bytes()).hexdigest(), 'native_sha256': hashlib.sha256(args.native.read_bytes()).hexdigest()}
  (args.output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
  print(json.dumps(report))


if __name__ == '__main__':
  main()
