import argparse
import hashlib
import json
from pathlib import Path
import subprocess

from pandad_device_cases import cases


def main() -> None:
  parser = argparse.ArgumentParser(description='Compare native Panda reads with unchanged source and packed health fields.')
  parser.add_argument('--source', type=Path, required=True)
  parser.add_argument('--native', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--runner', nargs=argparse.REMAINDER, default=[])
  args = parser.parse_args()
  output: Path = args.output
  output.mkdir(parents=True, exist_ok=False)
  corpus = cases()
  payload = ''.join(json.dumps(case) + '\n' for case in corpus)
  (output / 'input.jsonl').write_text(payload)
  results = []
  for name, command in [('source', [str(args.source)]), ('native', [*args.runner, str(args.native)])]:
    run = subprocess.run(command, input=payload, text=True, capture_output=True, check=False, timeout=60)
    (output / f'{name}.jsonl').write_text(run.stdout)
    (output / f'{name}.stderr').write_text(run.stderr)
    run.check_returncode()
    rows = [json.loads(line) for line in run.stdout.splitlines()]
    if len(rows) != len(corpus):
      raise AssertionError(f'{name} returned incomplete trace')
    for row in rows:
      for result in row['results']:
        if isinstance(result, dict) and 'health' in result:
          del result['health']['interrupt_load']
    results.append(rows)
  for index, (source, native) in enumerate(zip(*results, strict=True)):
    if source != native:
      (output / 'mismatch.json').write_text(json.dumps({'index': index, 'source': source, 'native': native}, indent=2) + '\n')
      raise AssertionError(f'Panda device comparison differs at {index}')
    if source['remaining'] != 0:
      raise AssertionError(f'Unconsumed input at {index}')
  report = {'status': 'PASS', 'scenarios': len(corpus), 'operations': sum(len(case['operations']) for case in corpus),
            'scope': 'packed health fields/bits, short reads, failures, control order/parameters, firmware and serial bytes',
            'source_sha256': hashlib.sha256(args.source.read_bytes()).hexdigest(),
            'native_sha256': hashlib.sha256(args.native.read_bytes()).hexdigest()}
  (output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
  print(json.dumps(report))


if __name__ == '__main__':
  main()
