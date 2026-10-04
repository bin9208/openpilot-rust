from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess

from card_runtime_source import load_binding
from radarcan_constructor_source import cases
from radarcan_exact import assert_exact
from radarcan_source import normalized, trace

ROOT = Path(__file__).resolve().parents[2]


def compare(actual, expected):
  for native, source in zip(actual, expected, strict=True):
    assert_exact(native['name'], source['name'])
    assert_exact(native['stdout'], source['stdout'])
    assert_exact(native['stderr'], source['stderr'])
    left, right = native['result'], source['result']
    assert left['outcome'] == right['outcome']
    if right['outcome'] == 'ok':
      assert_exact(left, right)
      continue
    assert_exact(left['parameter_reads'], right['parameter_reads'])
    error = right['error']
    match error['kind']:
      case 'FileNotFoundError':
        assert left['error']['debug'].startswith('DbcAsset'), (native['name'], left, right)
        filename = error['message'].rsplit("'", 2)[1]
        assert filename in left['error']['debug']
      case 'KeyError':
        assert left['error']['debug'].startswith(('MissingRadarDbc', 'Vehicle(UnknownCandidate')), (native['name'], left, right)
        candidate = source['name'].removeprefix('constructor-').rsplit('-unavailable', 1)[0]
        assert candidate in left['error']['debug'], (native['name'], left, right)
      case 'ValueError':
        assert left['error']['display'] == error['message'], (native['name'], left, right)
      case _:
        raise AssertionError((native['name'], left, right))


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binding', type=Path, required=True)
  parser.add_argument('--dbc', type=Path, required=True)
  parser.add_argument('--evidence', type=Path, required=True)
  parser.add_argument('--binary', type=Path)
  parser.add_argument('--reuse-source', type=Path)
  parser.add_argument('--runner', type=Path)
  parser.add_argument('--sysroot', type=Path)
  args = parser.parse_args()
  if (args.runner is None) != (args.sysroot is None):
    parser.error('--runner and --sysroot must be specified together')
  free = shutil.disk_usage(ROOT).free
  growth = 512 * 2**20
  assert free >= 25 * 2**30 + growth, (free, growth)
  args.evidence.mkdir(parents=True)
  load_binding(args.binding)
  if args.reuse_source is None:
    requests = cases(args.dbc)
    expected = [trace(case) for case in requests]
    (args.evidence / 'input.json').write_text(json.dumps(normalized(requests)) + '\n')
    (args.evidence / 'source.json').write_text(json.dumps(expected) + '\n')
  else:
    requests = json.loads((args.reuse_source / 'input.json').read_text())
    expected = json.loads((args.reuse_source / 'source.json').read_text())
  identity = {'source_cases': len(requests), 'free_before': free, 'growth_bound': growth,
    'source_reused': str(args.reuse_source) if args.reuse_source else None,
    'params_binding_sha256': hashlib.sha256(args.binding.read_bytes()).hexdigest()}
  source_paths = [*ROOT.glob('opendbc_repo/opendbc/car/*/radar_interface.py'),
    *ROOT.glob('opendbc_repo/opendbc/car/*/values.py'), ROOT / 'opendbc_repo/opendbc/car/interfaces.py',
    ROOT / 'rust/tools/radarcan_constructor_source.py', ROOT / 'rust/tools/radarcan_source.py', Path(__file__)]
  source_paths.extend(ROOT / path for path in ['opendbc_repo/opendbc/can/parser.py', 'opendbc_repo/opendbc/can/dbc.py',
    'opendbc_repo/opendbc/car/car_helpers.py', 'rust/tools/radarcan_decoder_source.py', 'rust/tools/radarcan_settings_source.py',
    'openpilot/common/params.h', 'openpilot/common/params_pyx.pyx'])
  (args.evidence / 'sources.json').write_text(json.dumps({str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest()
    for p in source_paths}, indent=2))
  if args.binary is None:
    identity.update(status='source_only_native_pending', successful=sum(row['result']['outcome'] == 'ok' for row in expected))
    (args.evidence / 'result.json').write_text(json.dumps(identity, indent=2))
    print(identity)
    return
  command = [str(args.binary.resolve()), str((args.evidence / 'native.json').resolve()), 'constructor-does-not-load-numerics',
    str(args.dbc.resolve())]
  if args.runner:
    command = [str(args.runner.resolve()), '-L', str(args.sysroot.resolve()), *command]
  (args.evidence / 'command.json').write_text(json.dumps({'command': command, **identity}, indent=2))
  result = subprocess.run(command, input=json.dumps(normalized(requests)), text=True, capture_output=True, check=False)
  (args.evidence / 'process.log').write_text(result.stdout + result.stderr + f'\nEXIT {result.returncode}\n')
  result.check_returncode()
  actual = json.loads((args.evidence / 'native.json').read_text())
  compare(actual, expected)
  identity.update(status='pass', executed_elf_sha256=hashlib.sha256(args.binary.read_bytes()).hexdigest())
  (args.evidence / 'result.json').write_text(json.dumps(identity, indent=2))
  print(identity)


if __name__ == '__main__':
  main()
