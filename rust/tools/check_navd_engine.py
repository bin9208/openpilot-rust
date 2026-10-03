from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys

from navd_cases import cases
from navd_source import Fixture, ROOT
from original_params_binding import load


def diagnostic_name(row):
  text = row['arguments'][0]
  for prefix, name in (('UI restarting', 'ui_restart'), ('Got new destination', 'new_destination'),
                       ('Calculating route', 'calculating'), ('Got empty route', 'empty_route'),
                       ('failed to get route', 'request_failed'), ('navd.failed_to_compute', 'compute_failed'),
                       ('navd route limited', 'route_limited'), ('Destination reached', 'destination_reached'),
                       ('API request failed', 'api_failed')):
    if text.startswith(prefix):
      return name
  raise AssertionError(row)


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--binding', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument("--runner", nargs=argparse.REMAINDER, default=[])
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  load(args.binding.resolve(), 'ipc://' + str((args.output / 'source-log').resolve()), args.output / 'logs')
  from openpilot.cereal import log
  failures = []
  steps = 0
  corpus = cases()
  for case in corpus:
    output = args.output / case['name']
    output.mkdir()
    fixture = Fixture(case['parameters'])
    source = [fixture.execute(row) for row in case['steps']]
    payload = json.dumps(case) + '\n'
    (output / 'input.json').write_text(payload)
    (output / 'source.json').write_text(json.dumps(source, indent=2) + '\n')
    process = subprocess.run([*args.runner, str(args.binary.resolve())], input=payload, text=True, capture_output=True, check=False, timeout=30)
    (output / 'native.json').write_text(process.stdout)
    (output / 'native.stderr').write_text(process.stderr)
    if process.returncode:
      failures.append({'case': case['name'], 'returncode': process.returncode})
      continue
    native = json.loads(process.stdout)
    for index, (expected, actual) in enumerate(zip(source, native, strict=True)):
      expected.pop('stdout')
      expected['diagnostics'] = [diagnostic_name(row) for row in expected['diagnostics']]
      expected['error'] = bool(expected['error'])
      for effect in actual['effects']:
        if 'event' in effect:
          with log.Event.from_bytes(bytes(effect.pop('event'))) as message:
            service = message.which()
            effect.update(service=service, valid=message.valid, data=getattr(message, service).to_dict())
      if expected != actual:
        failures.append({'case': case['name'], 'step': index, 'operation': case['steps'][index]['op'], 'source': expected, 'native': actual})
    steps += len(source)
  sources = [Path(__file__), ROOT / 'rust/tools/navd_cases.py', ROOT / 'rust/tools/navd_source.py',
             ROOT / 'openpilot/selfdrive/navd/navd.py', ROOT / 'openpilot/selfdrive/navd/helpers.py', args.binary, args.binding]
  sources.extend((ROOT / 'rust/crates/navd').rglob('*.rs'))
  report = {'status': 'FAIL' if failures else 'PASS', 'cases': len(corpus), 'steps': steps,
            'failures': failures, 'python': sys.version,
            'files': {str(path.resolve()): hashlib.sha256(path.read_bytes()).hexdigest() for path in sources}}
  (args.output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
  print(json.dumps({key: report[key] for key in ('status', 'cases', 'steps')}))
  raise SystemExit(bool(failures))


if __name__ == '__main__':
  main()
