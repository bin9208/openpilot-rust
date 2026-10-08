import argparse
import json
from pathlib import Path
import resource
import signal
import subprocess
import sys
import tempfile

from original_params_binding import load


def source(binding: Path, root: Path):
  resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
  load(binding, f'ipc://{root}/logs.sock', root / 'logs')
  from openpilot.common.params import Params
  from openpilot.selfdrive.carrot.server.services import params
  params.Params = lambda: Params(str(root))
  for line in sys.stdin:
    request = json.loads(line)
    print(json.dumps(params.get_param_value(request['name'], request['default'])), flush=True)


def compare(binary: Path, binding: Path, output: Path):
  resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
  output.mkdir(parents=True, exist_ok=True)
  load(binding, f'ipc://{output.resolve()}/logs.sock', output / 'logs')
  from openpilot.common.params import Params
  receipts = []
  for index, (name, raw) in enumerate((
    ('LongitudinalPersonalityMax', b'invalid'),
    ('LongitudinalPersonalityMax', b'2147483648'),
    ('LongitudinalPersonalityMax', b'-2147483649'),
    ('UptimeOnroad', b'invalid'), ('UptimeOnroad', b'1e999'), ('UptimeOnroad', b'1e-999'),
  )):
    case = output / str(index)
    case.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='carrot-fatal-') as temporary:
      root = Path(temporary)
      results = {}
      for side in ('original', 'native'):
        owned = root / side
        params = Params(str(owned))
        Path(params.get_param_path('IsMetric')).write_bytes(b'1')
        Path(params.get_param_path(name)).write_bytes(raw)
        requests = [{'action': 'get_param', 'root': str(owned), 'name': 'IsMetric', 'default': False},
                    {'action': 'get_param', 'root': str(owned), 'name': name, 'default': 9},
                    {'action': 'get_param', 'root': str(owned), 'name': 'IsMetric', 'default': False}]
        data = ''.join(json.dumps(request) + '\n' for request in requests)
        (case / f'{side}.inputs.jsonl').write_text(data)
        (case / 'raw.hex').write_text(raw.hex() + '\n')
        command = [sys.executable, '-P', str(Path(__file__).resolve()), '--binding', str(binding), '--source-root', str(owned)] if side == 'original' else [str(binary)]
        result = subprocess.run(command, input=data, capture_output=True, text=True, timeout=10)
        (case / f'{side}.stdout').write_text(result.stdout)
        (case / f'{side}.stderr').write_text(result.stderr)
        results[side] = {'command': command, 'exit_code': result.returncode, 'stdout': result.stdout}
      passed = all(result['exit_code'] == -signal.SIGABRT and result['stdout'] == 'true\n' for result in results.values())
      receipt = {'name': name, 'raw_hex': raw.hex(), 'core_limit': 0, 'results': results, 'passed': passed}
      (case / 'result.json').write_text(json.dumps(receipt, indent=2) + '\n')
      receipts.append(receipt)
  summary = {'cases': len(receipts), 'passed': all(receipt['passed'] for receipt in receipts),
             'observable': 'prefix only; SIGABRT; no JSON output for fatal input or trailing input',
             'stderr': 'captured implementation-specific diagnostics, not compared as prose'}
  (output / 'result.json').write_text(json.dumps(summary) + '\n')
  assert summary['passed'], summary
  print(json.dumps(summary))


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path)
  parser.add_argument('--binding', type=Path, required=True)
  parser.add_argument('--output', type=Path)
  parser.add_argument('--source-root', type=Path)
  args = parser.parse_args()
  if args.source_root is not None:
    source(args.binding, args.source_root)
  elif args.binary is not None and args.output is not None:
    compare(args.binary, args.binding, args.output)
  else:
    parser.error('--binary and --output are required for comparison')


if __name__ == '__main__':
  main()
