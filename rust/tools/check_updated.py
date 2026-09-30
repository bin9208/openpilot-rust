"""Focused original/native updater comparisons using real owned Git checkouts."""

import argparse
import json
from pathlib import Path
import subprocess
import sys
import tempfile

from updated_fixtures import normalize, seed, setup

ROOT = Path(__file__).resolve().parents[2]


def cases():
  yield (
    'check-fetch-metered',
    {},
    [
      {},
      {'request': 'check'},
      {'request': 'fetch'},
      {'request': 'none', 'params': {'NetworkMetered': '1'}},
      {'request': 'none', 'now': 1791072000.001},
    ],
  )
  yield (
    'missing-branch-failure',
    {},
    [
      {},
      {'request': 'fetch', 'params': {'UpdaterTargetBranch': 'missing'}},
      {'params': {'UpdaterTargetBranch': 'dev', 'NetworkMetered': '1', 'UpdaterLastFetchTime': '2026-10-01T00:00:00+00:00'}},
    ],
  )
  yield 'tizi-branch-map-agnos-same-version', {'device': 'tizi', 'agnos': True}, [{}, {'request': 'fetch', 'params': {'UpdaterTargetBranch': 'release3'}}]
  yield 'agnos-model-manifest-boundary', {'device': 'tici', 'agnos': True, 'os_version': 'older-os'}, [{}, {'request': 'fetch'}]
  yield (
    'connectivity-thresholds',
    {},
    [
      {},
      {'report_failure': 1, 'params': {'UptimeOnroad': str(2300 * 3600), 'RouteCount': '8001'}},
      {'report_failure': 1, 'params': {'UptimeOnroad': str(2301 * 3600), 'RouteCount': '8001'}},
      {'report_failure': 1, 'params': {'UptimeOnroad': str(2699 * 3600), 'RouteCount': '8001'}},
      {'report_failure': 1, 'params': {'UptimeOnroad': str(2701 * 3600), 'RouteCount': '8401'}},
      {'report_failure': 1, 'params': {'RouteCount': '9000.5'}},
      {'report_failure': 16, 'has_internet': True},
    ],
  )


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--launcher', type=Path, required=True)
  parser.add_argument('--binding', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  results = []
  with tempfile.TemporaryDirectory(prefix='updated-source-') as temporary:
    directory = Path(temporary)
    remote, previous = seed(directory)
    for name, options, steps in cases():
      pair = []
      for implementation in ['source', 'native']:
        root = directory / f'{name}-{implementation}'
        config, env = setup(root, remote, previous, args.launcher.resolve())
        config.update(options, steps=steps)
        env['PYTHONPATH'] = str(ROOT)
        command = (
          [sys.executable, str(ROOT / 'rust/tools/updated_source.py'), str(args.binding.resolve())]
          if implementation == 'source'
          else [str(args.binary.resolve())]
        )
        process = subprocess.run(command, input=json.dumps(config), env=env, capture_output=True, text=True, timeout=60)
        output = args.output / f'{name}-{implementation}'
        output.with_suffix('.stdout').write_text(process.stdout or '<empty>\n')
        output.with_suffix('.stderr').write_text(process.stderr or '<empty>\n')
        assert process.returncode == 0, (name, implementation, process.stderr)
        value = normalize(json.loads(process.stdout.splitlines()[-1]), root)
        output.with_suffix('.json').write_text(json.dumps(value, indent=2))
        pair.append(value)
      assert pair[0] == pair[1], (name, 'source/native mismatch; inspect captured JSON')
      if name == 'check-fetch-metered':
        assert pair[1]['snapshots'][2]['consistent'] and pair[1]['snapshots'][4]['consistent']
        assert pair[1]['snapshots'][2]['params']['UpdateAvailable'] == b'1'.hex()
        assert pair[1]['snapshots'][3]['params']['UpdaterFetchAvailable'] == b'1'.hex()
      elif name in ['tizi-branch-map-agnos-same-version', 'agnos-model-manifest-boundary']:
        assert pair[1]['snapshots'][-1]['consistent']
        if name == 'agnos-model-manifest-boundary':
          assert pair[1]['agnos'][-1]['manifest'].endswith('/agnos-tici.json')
          assert pair[1]['agnos'][-1]['slot'] == 1
        else:
          assert not pair[1]['agnos']
      elif name == 'missing-branch-failure':
        assert pair[1]['snapshots'][-1]['wait'] == 300 and not pair[1]['snapshots'][-1]['consistent']
      elif name == 'connectivity-thresholds':
        assert 'Offroad_ConnectivityNeededPrompt' not in pair[1]['snapshots'][1]['params']
        assert 'Offroad_ConnectivityNeededPrompt' in pair[1]['snapshots'][2]['params']
        assert 'Offroad_ConnectivityNeeded' in pair[1]['snapshots'][4]['params']
        assert 'Offroad_ConnectivityNeeded' not in pair[1]['snapshots'][5]['params']
        assert 'Offroad_UpdateFailed' in pair[1]['snapshots'][6]['params']
      results.append({'scenario': name, 'snapshots': len(pair[0]['snapshots']), 'commands': len(pair[0]['commands']), 'passed': True})
      print(f'PASS {name}: {results[-1]["snapshots"]} snapshots, {results[-1]["commands"]} ordered native commands')
  (args.output / 'result.json').write_text(json.dumps({'passed': True, 'scenarios': results, 'device_access': False}, indent=2) + '\n')


if __name__ == '__main__':
  main()
