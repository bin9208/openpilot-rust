"""Native user policy: update preparation during driving, activation at restart (#346)."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile


def click(frame, x, y):
  return [{'frame': frame + i, 'events': [{'pos': {'x': x, 'y': y}, 'slot': 0,
    'pressed': i == 0, 'released': i == 1, 'down': i == 0, 'time': (frame + i) / 20}]} for i in range(2)]


def scene(big, params):
  return {'kind': 'software' if big else 'device',
    'config': {'big': big, 'large_viewport': big, 'pc': True, 'scale': 1.0},
    'language': 'en', 'rect': {'x': 0, 'y': 0, 'width': 2160 if big else 536, 'height': 1080 if big else 240},
    'frames': 70, 'prime': -2, 'params': {'UpdaterState': 'idle', **params},
    'time_valid': True, 'capture_effects': True, 'steps': []}


def main():
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--display', required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  root = Path(__file__).resolve().parents[2]
  cases = []
  for big in [True, False]:
    for request in ['Check', 'Download']:
      value = scene(big, {'UpdaterFetchAvailable': '1' if request == 'Download' else '0'})
      value['steps'] = ([{'frame': 5, 'started': True}] + click(10, 2040, 425) if big else
        [{'frame': 15, 'scroll': 380}, {'frame': 25, 'started': True}] + click(35, 200, 120))
      cases.append((f'{"large" if big else "small"}-onroad-{request.lower()}', value, request))
  value = scene(True, {'IsTestedBranch': '1', 'GitBranch': 'dev-md', 'UpdaterTargetBranch': 'dev-md',
    'UpdaterAvailableBranches': 'dev-md,candidate'})
  value['steps'] = [{'frame': 5, 'started': True}] + click(10, 2040, 595) + [{'frame': 15, 'selection': 'candidate', 'confirm': True}]
  cases.append(('tested-branch-selection', value, 'Check'))
  for started in [True, False]:
    value = scene(True, {'UpdateAvailable': '1'})
    value['steps'] = [{'frame': 5, 'started': started}] + click(10, 2040, 425)
    cases.append((f'large-ready-{"onroad" if started else "offroad"}', value, 'Check' if started else 'Reboot'))
  for started in [True, False]:
    value = scene(False, {'UpdateAvailable': '1'})
    value['steps'] = [{'frame': 15, 'scroll': 380}, {'frame': 25, 'started': started}] + click(35, 200, 120)
    cases.append((f'small-ready-{"onroad" if started else "offroad"}', value, 'Check' if started else 'Reboot'))
  value = scene(False, {'UpdateAvailable': '1'})
  value['steps'] = [{'frame': 15, 'scroll': 380}] + click(35, 200, 120)
  value['steps'][-1]['started'] = True
  cases.append(('ignition-before-release', value, None))
  results = []
  for name, value, request in cases:
    for language in ['en', 'ko']:
      value['language'] = language
      label = f'{name}-{language}'
      path = args.output / f'{label}.json'
      path.write_text(json.dumps(value))
      output = args.output / f'{label}.png'
      with tempfile.TemporaryDirectory(prefix='msgq_rust-probe-update-', dir='/dev/shm') as namespace:
        env = dict(os.environ, DISPLAY=args.display, OFFSCREEN='1',
          OPENPILOT_PREFIX=Path(namespace).name.removeprefix('msgq_'))
        with (args.output / f'{label}.log').open('w') as log:
          subprocess.run([str(args.binary.resolve()), str(root), str(path.resolve()), str(output.resolve())],
            env=env, stdout=log, stderr=subprocess.STDOUT, check=True, timeout=60)
      rows = json.loads(output.with_suffix('.json').read_text())
      final = rows[-1]
      reboot = final['params']['DoReboot']
      if request == 'Reboot':
        assert reboot == '1', (label, final)
      else:
        assert reboot != '1', (label, 'unexpected reboot', final)
        if request:
          assert {'updater': request} in final['effects'], (label, final)
      if name == 'tested-branch-selection':
        assert final['params']['UpdaterTargetBranch'] == 'candidate', (label, final)
      assert output.stat().st_size > 0
      results.append({'case': label, 'request': request, 'reboot': reboot, 'capture': str(output)})
  (args.output / 'results.json').write_text(json.dumps(results, indent=2))
  print(f'PASS: {len(results)} native update preparation/branch/restart scenarios')


if __name__ == '__main__':
  main()
