from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import time

from card_runtime_source import load_binding
from radarcan_runtime_capture import Arguments, ROOT, capture, recorded_environment
from radarcan_runtime_types import Case


def main() -> None:
  parser = argparse.ArgumentParser()
  for option in ('binary', 'binding', 'numerics', 'dbc', 'cases', 'evidence'):
    parser.add_argument('--' + option, type=Path, required=True)
  parser.add_argument('--name', action='append')
  parser.add_argument('--lifecycle', action='store_true')
  args = parser.parse_args()
  free = shutil.disk_usage(ROOT).free
  growth = 256 * 2**20
  assert free >= 25 * 2**30 + growth, (free, growth)
  cases: list[Case] = json.loads(args.cases.read_text())
  if args.name and not args.lifecycle:
    cases = [case for case in cases if case['name'] in args.name]
    assert len(cases) == len(args.name)
  args.evidence.mkdir(parents=True)
  load_binding(args.binding)
  source_paths = [*ROOT.glob('openpilot/selfdrive/carrot/radar/*.py'),
    ROOT / 'opendbc_repo/opendbc/car/interfaces.py',
    *ROOT.glob('opendbc_repo/opendbc/car/*/radar_interface.py'),
    *ROOT.glob('rust/tools/radarcan_runtime_*.py'), Path(__file__)]
  sources = {str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in source_paths}
  (args.evidence / 'command.json').write_text(json.dumps({'argv': os.sys.argv, 'environment': recorded_environment(dict(os.environ)),
    'cases': [case['name'] for case in cases], 'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest(),
    'free_before': free, 'growth_bound': growth, 'sources': sources}, indent=2))
  invocation = Arguments(args.binary.resolve(), args.binding.resolve(), args.numerics.resolve(),
    args.dbc.resolve(), args.evidence.resolve())
  results = []
  start = time.monotonic()
  if args.lifecycle:
    import resource
    from radarcan_runtime_lifecycle import capture as capture_lifecycle, scenarios
    resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
    scenario_list = scenarios(next(case for case in cases if case['candidate'] == 'VOLKSWAGEN_ID4_MK1'), invocation)
    if args.name:
      scenario_list = [scenario for scenario in scenario_list if scenario.name in args.name]
      assert len(scenario_list) == len(args.name)
    for scenario in scenario_list:
      results.append(capture_lifecycle(invocation, scenario))
      (args.evidence / 'results.json').write_text(json.dumps(results, indent=2))
      print(scenario.name, results[-1]['source_exit'], results[-1]['native_exit'], 'pass', flush=True)
    (args.evidence / 'exit.json').write_text(json.dumps({'status': 'pass', 'cases': len(results),
      'seconds': time.monotonic() - start}) + '\n')
    return
  for case in cases:
    results.append(capture(invocation, case))
    (args.evidence / 'results.json').write_text(json.dumps(results, indent=2))
    print(case['name'], results[-1]['publications'], 'exact', flush=True)
  (args.evidence / 'exit.json').write_text(json.dumps({'status': 'pass', 'cases': len(results),
    'input_steps': sum(row['input_steps'] for row in results),
    'publications': sum(row['publications'] for row in results), 'seconds': time.monotonic() - start}) + '\n')


if __name__ == '__main__':
  main()
