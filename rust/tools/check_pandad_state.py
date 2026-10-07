import argparse
import hashlib
import json
from pathlib import Path
import subprocess

from pandad_state_cases import cases


def main() -> None:
  parser = argparse.ArgumentParser(description='Compare native Panda state commands and complete cereal bytes with original bodies.')
  parser.add_argument('--source', type=Path, required=True)
  parser.add_argument('--native', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--runner', nargs=argparse.REMAINDER, default=[])
  args = parser.parse_args()
  output: Path = args.output
  output.mkdir(parents=True, exist_ok=False)
  corpus = cases()
  for index, case in enumerate(corpus):
    payload = json.dumps(case) + '\n'
    (output / f'{index}-input.json').write_text(payload)
    results = []
    for name, command in [('source', [str(args.source)]), ('native', [*args.runner, str(args.native)])]:
      run = subprocess.run(command, input=payload, text=True, capture_output=True, check=False, timeout=60)
      (output / f'{index}-{name}.json').write_text(run.stdout)
      (output / f'{index}-{name}.stderr').write_text(run.stderr)
      run.check_returncode()
      rows = json.loads(run.stdout)
      if len(rows) != len(case['steps']):
        raise AssertionError(f'{name} returned incomplete state trace')
      results.append(rows)
    for step, (source, native) in enumerate(zip(*results, strict=True)):
      if source != native:
        (output / 'mismatch.json').write_text(json.dumps({'case': index, 'step': step, 'source': source, 'native': native}, indent=2) + '\n')
        raise AssertionError(f'Panda state differs at case {index}, step {step}')
  report = {'status': 'PASS', 'scenarios': len(corpus), 'steps': sum(len(case['steps']) for case in corpus),
            'scope': 'exact complete cereal bytes, all ordered reads/writes/logs, ignition, health-failure recovery and reconnect/heartbeat effects',
            'source_sha256': hashlib.sha256(args.source.read_bytes()).hexdigest(),
            'native_sha256': hashlib.sha256(args.native.read_bytes()).hexdigest()}
  (output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
  print(json.dumps(report))


if __name__ == '__main__':
  main()
