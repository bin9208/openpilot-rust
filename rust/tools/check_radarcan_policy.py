from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import subprocess

from radarcan_cases import cases
from radarcan_source import normalized, trace
from radarcan_exact import assert_exact

ROOT = Path(__file__).resolve().parents[2]


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--numerics', type=Path)
  parser.add_argument('--binding', type=Path)
  parser.add_argument('--dbc', type=Path)
  parser.add_argument('--runner', type=Path)
  parser.add_argument('--sysroot', type=Path)
  parser.add_argument('--operation', choices=['batches', 'lead_filter', 'track', 'weights', 'parser_sets', 'base', 'decoder', 'cluster', 'hyundai', 'runtime'])
  parser.add_argument('--case', action='append')
  parser.add_argument('--evidence', type=Path, required=True)
  args = parser.parse_args()
  if (args.runner is None) != (args.sysroot is None):
    parser.error('--runner and --sysroot must be specified together')
  args.evidence.mkdir(parents=True)
  import numpy as np
  assert np.__version__ == '2.5.3'
  include_parser = args.operation is None or args.operation == 'parser_sets'
  if include_parser or args.operation in ('base', 'decoder', 'cluster', 'hyundai', 'runtime'):
    if args.binding is None:
      raise ValueError('original Params binding is required for source CAN imports')
    from card_runtime_source import load_binding
    load_binding(args.binding)
  if args.operation == 'runtime':
    from radarcan_loop_cases import cases as runtime_cases
    assert args.dbc is not None, 'prepared original DBC assets required'
    requests = runtime_cases(args.dbc)
  elif args.operation == 'hyundai':
    from radarcan_hyundai_source import cases as hyundai_cases
    assert args.dbc is not None, 'prepared original DBC assets required'
    requests = hyundai_cases(args.dbc)
  elif args.operation == 'cluster':
    from radarcan_cluster_source import cases as cluster_cases
    requests = cluster_cases()
  elif args.operation == 'decoder':
    from radarcan_decoder_cases import cases as decoder_cases
    assert args.dbc is not None, 'prepared original DBC assets required'
    requests = decoder_cases(args.dbc)
  else:
    requests = [case for case in cases(include_parser=include_parser) if args.operation is None or case['op'] == args.operation]
  if args.case:
    requests = [case for case in requests if case['name'] in args.case]
  assert requests, 'no matching radar policy cases'
  expected = [trace(case) for case in requests]
  (args.evidence / 'input.json').write_text(json.dumps(normalized(requests)) + '\n')
  (args.evidence / 'source.json').write_text(json.dumps(expected) + '\n')
  files = ['openpilot/selfdrive/carrot/radar/can_batch.py', 'opendbc_repo/opendbc/car/radar_tracks.py',
           'opendbc_repo/opendbc/car/radar_lead_filter.py', 'openpilot/common/filter_simple.py',
           'rust/tools/radarcan_cases.py', 'rust/tools/radarcan_source.py', 'rust/tools/check_radarcan_policy.py',
           'rust/tools/radarcan_parser_source.py', 'opendbc_repo/opendbc/can/parser.py',
           'opendbc_repo/opendbc/can/dbc.py', 'opendbc_repo/opendbc/can/packer.py',
           'rust/tools/radarcan_base_source.py', 'opendbc_repo/opendbc/car/interfaces.py']
  files += ['rust/tools/radarcan_exact.py']
  if args.operation in ('decoder', 'hyundai'):
    files += ['rust/tools/radarcan_decoder_source.py', 'rust/tools/radarcan_decoder_cases.py',
              'rust/tools/radarcan_settings_source.py', 'rust/tools/radarcan_hyundai_source.py']
    files += [str(path.relative_to(ROOT)) for path in sorted((ROOT / 'opendbc_repo/opendbc/car').glob('*/radar_interface.py'))]
  if args.operation == 'cluster':
    files += ['rust/tools/radarcan_cluster_source.py', 'opendbc_repo/opendbc/car/ford/radar_interface.py']
  if args.operation == 'runtime':
    files += ['rust/tools/radarcan_loop_source.py', 'rust/tools/radarcan_loop_cases.py',
      'rust/tools/radarcan_decoder_source.py', 'rust/tools/radarcan_decoder_cases.py', 'rust/tools/radarcan_settings_source.py',
      'openpilot/selfdrive/carrot/radar/radarcan.py', 'openpilot/selfdrive/carrot/radar/lateral.py',
      'openpilot/selfdrive/pandad/pandad_api_impl.py', 'openpilot/cereal/messaging/__init__.py',
      'opendbc_repo/opendbc/car/volkswagen/radar_interface.py']
  (args.evidence / 'sources.json').write_text(json.dumps({name: hashlib.sha256((ROOT / name).read_bytes()).hexdigest() for name in files}, indent=2) + '\n')
  (args.evidence / 'dependencies.json').write_text(json.dumps({'numpy': np.__version__,
    'params_binding': str(args.binding.resolve()) if args.binding else None,
    'params_binding_sha256': hashlib.sha256(args.binding.read_bytes()).hexdigest() if args.binding else None}, indent=2) + '\n')
  command = [str(args.binary.resolve()), str((args.evidence / 'native.json').resolve())]
  if args.numerics:
    command.append(str(args.numerics.resolve()))
  if args.dbc:
    command.append(str(args.dbc.resolve()))
  if args.runner:
    command = [str(args.runner.resolve()), '-L', str(args.sysroot.resolve()), *command]
  (args.evidence / 'command.json').write_text(json.dumps(command) + '\n')
  try:
    child = subprocess.run(command, input=json.dumps(normalized(requests)), text=True, capture_output=True, check=False)
  except FileNotFoundError as error:
    (args.evidence / 'red.json').write_text(json.dumps({'status': 'expected_missing_native_component', 'source_cases': len(requests),
      'error': str(error), 'native_executed': False}, indent=2) + '\n')
    raise
  (args.evidence / 'process.log').write_text(child.stdout + child.stderr + f'\nEXIT {child.returncode}\n')
  if child.returncode:
    (args.evidence / 'red.json').write_text(json.dumps({'status': 'native_functional_failure',
      'source_cases': len(requests), 'native_executed': True, 'exit': child.returncode,
      'executed_elf_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest()}, indent=2) + '\n')
  child.check_returncode()
  actual = json.loads((args.evidence / 'native.json').read_text())
  if args.operation == 'runtime':
    from openpilot.cereal import messaging
    for answer in actual:
      answer['result']['publications'] = [normalized(messaging.log_from_bytes(bytes(payload)).to_dict())
        for payload in answer['result']['publications']]
    (args.evidence / 'native-decoded.json').write_text(json.dumps(actual) + '\n')
  assert_exact(actual, expected)
  (args.evidence / 'result.json').write_text(json.dumps({'status': 'pass', 'cases': len(requests),
    'executed_elf_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest()}, indent=2) + '\n')


if __name__ == '__main__':
  main()
