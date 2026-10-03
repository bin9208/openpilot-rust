"""Run original/native Home layouts with isolated state and compare every frame."""

import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import numpy as np
from PIL import Image

parser = argparse.ArgumentParser()
parser.add_argument('--binary', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
parser.add_argument('--display', default=':127')
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
root = Path(__file__).resolve().parents[2]


def event(frame, x, y, down):
  return {'frame': frame, 'events': [{'pos': {'x': x, 'y': y}, 'slot': 0, 'pressed': down, 'released': not down, 'down': down, 'time': frame / 20}]}


cases = []
for experimental in [False, True]:
  cases.append(
    (
      f'experimental-{experimental}',
      {
        'kind': 'experimental',
        'rect': {'x': 40, 'y': 145, 'width': 750, 'height': 125},
        'params': {'ExperimentalMode': str(int(experimental))},
        'steps': [event(1, 100, 180, True), event(2, 100, 180, False)],
      },
    )
  )
cases.append(
  (
    'sidebar',
    {
      'kind': 'sidebar',
      'rect': {'x': 0, 'y': 0, 'width': 300, 'height': 1080},
      'frames': 14,
      'steps': [
        {
          'frame': i,
          'now': 100.0 + i / 20,
          'device': {'network': i % 7, 'strength': i % 5, 'thermal': i % 4, 'ping': [0, 20_000_000_001, 20_000_000_000, 110_000_000_000][i % 4]},
          'panda': i % 2,
          'recording': bool(i % 2),
        }
        for i in range(14)
      ],
    },
  )
)
cases.append(('home-pair', {'kind': 'home', 'prime': -2}))
cases.append(('home-paid', {'kind': 'home', 'prime': 1, 'params': {'UpdaterCurrentDescription': '2026.10 fixture', 'ExperimentalMode': '1'}}))
cases.append(
  (
    'home-update',
    {'kind': 'home', 'params': {'UpdateAvailable': '1', 'UpdaterNewReleaseNotes': '<h2>Native runtime</h2><p>Release notes and <b>important changes.</b></p>'}},
  )
)
cases.append(('home-alert', {'kind': 'home', 'params': {'Offroad_ConnectivityNeeded': json.dumps({'text': 'Connect within %1 days.', 'extra': '3'})}}))
cases.append(
  (
    'mici-home',
    {
      'kind': 'mici-home',
      'config': {'big': False, 'large_viewport': False, 'pc': True, 'scale': 1.0},
      'rect': {'x': 0, 'y': 0, 'width': 536, 'height': 240},
      'params': {'Version': '0.10.0', 'GitBranch': 'feature-native-port', 'GitCommit': '0123456789abcdef', 'GitCommitDate': "'1790812800 +0000'"},
      'address': '192.0.2.4',
      'steps': [{'frame': 1, 'now': 5.0}, {'frame': 2, 'now': 5.1}],
    },
  )
)
cases.append(
  (
    'terms-decline-back',
    {
      'kind': 'onboarding',
      'rect': {'x': 0, 'y': 0, 'width': 2160, 'height': 1080},
      'frames': 7,
      'steps': [dict(event(1, 300, 930, True), flush_training=True), event(2, 300, 930, False), event(4, 300, 930, True), event(5, 300, 930, False)],
    },
  )
)
cases.append(
  (
    'terms-decline-uninstall',
    {
      'kind': 'onboarding',
      'rect': {'x': 0, 'y': 0, 'width': 2160, 'height': 1080},
      'frames': 7,
      'steps': [dict(event(1, 300, 930, True), flush_training=True), event(2, 300, 930, False), event(4, 1500, 930, True), event(5, 1500, 930, False)],
    },
  )
)
points = [
  (300, 900),
  (1950, 400),
  (1950, 400),
  (1700, 650),
  (1750, 550),
  (1950, 400),
  (1900, 700),
  (1600, 400),
  (1800, 550),
  (1000, 900),
  (1700, 300),
  (1950, 400),
  (1600, 400),
  (1950, 400),
  (1800, 300),
  (1500, 600),
  (1950, 400),
  (1950, 400),
  (900, 900),
]
steps = [{'frame': 1, 'flush_training': True}]
for i, (x, y) in enumerate(points):
  steps.extend([event(2 + i * 2, x, y, True), event(3 + i * 2, x, y, False)])
cases.append(
  (
    'training-all',
    {'kind': 'onboarding', 'rect': {'x': 0, 'y': 0, 'width': 2160, 'height': 1080}, 'params': {'HasAcceptedTerms': '2'}, 'frames': 42, 'steps': steps},
  )
)

alert = json.dumps({'text': 'Connect within %1 days.', 'extra': '3'})
cases.append(
  (
    'home-transitions',
    {
      'kind': 'home',
      'frames': 12,
      'steps': [
        {'frame': 1, 'now': 9.999, 'params': {'UpdateAvailable': '1', 'Offroad_ConnectivityNeeded': alert}},
        {'frame': 2, 'now': 10.0},
        {'frame': 3, 'now': 10.05},
        dict(event(4, 650, 85, True), now=10.1),
        dict(event(5, 650, 85, False), now=10.15),
        {'frame': 6, 'now': 20.0, 'params': {'Offroad_ConnectivityNeeded': None}},
        {'frame': 7, 'now': 30.0, 'params': {'UpdateAvailable': '0', 'Offroad_ConnectivityNeeded': alert}},
        {'frame': 8, 'now': 40.0, 'params': {'Offroad_ConnectivityNeeded': None}},
        {'frame': 9, 'now': 50.0, 'params': {'UpdateAvailable': '1', 'Offroad_ConnectivityNeeded': alert}},
        {'frame': 10, 'now': 60.0, 'params': {'UpdateAvailable': '0'}},
        {'frame': 11, 'now': 70.0, 'params': {'Offroad_ConnectivityNeeded': None}},
      ],
    },
  )
)
for name, params, x in [
  ('dismiss', {'UpdateAvailable': '1'}, 550),
  ('reboot', {'UpdateAvailable': '1'}, 1800),
  ('snooze', {'Offroad_ConnectivityNeeded': alert}, 1850),
  ('acknowledge', {'Offroad_ConnectivityNeeded': alert, 'Offroad_ExcessiveActuation': json.dumps({'text': 'Excessive actuation'})}, 1600),
]:
  cases.append((f'home-{name}', {'kind': 'home', 'frames': 6, 'params': params, 'steps': [event(2, x, 930, True), event(3, x, 930, False)]}))
catalog = json.loads((root / 'openpilot/selfdrive/selfdrived/alerts_offroad.json').read_text())
alerts = {key: json.dumps({'text': f'{key}: ' + ('Long alert text with emoji 🔥 and wrapping. ' * 8), 'extra': ''}) for key in catalog}
cases.append(('home-alert-order-scroll', {'kind': 'home', 'frames': 4, 'params': alerts, 'steps': [{'frame': 2, 'scroll': -700.0}]}))
cases.append(('home-malformed-alerts', {'kind': 'home', 'params': dict(zip(list(catalog)[:6], ['{broken', 'null', 'false', '[]', '0', '""'], strict=True))}))
cases.append(
  (
    'sidebar-actions-gate',
    {
      'kind': 'sidebar',
      'rect': {'x': 0, 'y': 0, 'width': 300, 'height': 1080},
      'frames': 12,
      'steps': [
        {'frame': 0, 'device': {'network': 1, 'strength': 4, 'thermal': 0, 'ping': 0}, 'recording': True},
        event(1, 100, 80, True),
        event(2, 100, 80, False),
        event(3, 150, 950, True),
        event(4, 150, 950, False),
        event(5, 200, 260, True),
        event(6, 200, 260, False),
        {'frame': 7, 'recording': False},
        {'frame': 8, 'device': {'network': 0, 'strength': 0, 'thermal': 3, 'ping': 0}},
        event(9, 200, 260, True),
        event(10, 200, 260, False),
      ],
    },
  )
)
cases.append(
  (
    'sidebar-ping-boundary',
    {
      'kind': 'sidebar',
      'rect': {'x': 0, 'y': 0, 'width': 300, 'height': 1080},
      'frames': 3,
      'steps': [
        {'frame': i, 'now': 100.0 + i, 'device': {'network': 6, 'strength': 0, 'thermal': 0, 'ping': int((100 + i) * 1e9) - 80_000_000_000 + delta}}
        for i, delta in enumerate([1, 0, -1])
      ],
    },
  )
)
compact = {
  'kind': 'mici-home',
  'config': {'big': False, 'large_viewport': False, 'pc': True, 'scale': 1.0},
  'rect': {'x': 0, 'y': 0, 'width': 536, 'height': 240},
}
for name, car, x in [
  ('long-allowed', {'openpilot_longitudinal_control': True}, 20),
  ('long-gated', {'openpilot_longitudinal_control': False}, 20),
  ('web-hold', {'openpilot_longitudinal_control': True}, 165),
]:
  cases.append(
    (
      f'mici-{name}',
      {
        **compact,
        'car': {'alpha_longitudinal_available': False, 'max_lateral_accel': 2.5, **car},
        'frames': 17,
        'steps': [event(1, x, 210, True), event(14, x, 210, False)],
      },
    )
  )
for name, params, address in [
  ('release', {'Version': '0.10', 'GitBranch': 'nightly', 'GitCommit': 'abcdef0123', 'GitCommitDate': 'invalid'}, '0.0.0.0'),
  ('missing', {}, '  '),
  (
    'long-branch',
    {
      'Version': '0.10',
      'GitBranch': 'feature-native-very-long-branch-name-for-scrolling-text-overflow',
      'GitCommit': 'abcdef0123',
      'GitCommitDate': '1790812800 +0000',
    },
    '2001:db8::1',
  ),
]:
  cases.append(
    (
      f'mici-{name}',
      {**compact, 'params': params, 'address': address, 'frames': 4, 'steps': [{'frame': 1, 'now': 5.0}, {'frame': 2, 'now': 5.1}, {'frame': 3, 'now': 7.0}]},
    )
  )
cases.append(
  (
    'terms-accepted-pretrained',
    {
      'kind': 'onboarding',
      'rect': {'x': 0, 'y': 0, 'width': 2160, 'height': 1080},
      'params': {'CompletedTrainingVersion': '0.2.0'},
      'frames': 6,
      'steps': [{'frame': 1, 'flush_training': True}, event(2, 1500, 930, True), event(3, 1500, 930, False)],
    },
  )
)
fast = []
for x, y in points[:-1]:
  fast.extend(event(1, x, y, True)['events'] + event(1, x, y, False)['events'])
cases.append(
  (
    'training-fast-fallback',
    {
      'kind': 'onboarding',
      'rect': {'x': 0, 'y': 0, 'width': 2160, 'height': 1080},
      'params': {'HasAcceptedTerms': '2'},
      'frames': 5,
      'steps': [{'frame': 1, 'flush_training': True, 'events': fast}],
    },
  )
)
restart = []
for i, (x, y) in enumerate(points):
  if i in [9, 18]:
    x, y = 300, 900
  restart.extend([event(2 + i * 2, x, y, True), event(3 + i * 2, x, y, False)])
cases.append(
  (
    'training-no-record-restart',
    {
      'kind': 'onboarding',
      'rect': {'x': 0, 'y': 0, 'width': 2160, 'height': 1080},
      'params': {'HasAcceptedTerms': '2', 'CompletedTrainingVersion': '0.2.0'},
      'frames': 42,
      'steps': [{'frame': 1, 'flush_training': True}, *restart],
    },
  )
)
cases.append(('training-cancel-on-destruction', {'kind': 'onboarding', 'rect': {'x': 0, 'y': 0, 'width': 2160, 'height': 1080}, 'frames': 1}))
cases.append(
  (
    'sidebar-unknown-enums',
    {
      'kind': 'sidebar',
      'rect': {'x': 0, 'y': 0, 'width': 300, 'height': 1080},
      'frames': 2,
      'steps': [{'frame': i, 'device': {'network': raw, 'strength': raw, 'thermal': raw, 'ping': 0}} for i, raw in enumerate([123, 65535])],
    },
  )
)
cases.append(
  (
    'mici-icons',
    {
      **compact,
      'frames': 37,
      'steps': [
        {
          'frame': i,
          'now': i * 5.1,
          'device': {'network': i // 5 if i < 35 else 123, 'strength': i % 5 if i < 35 else 123, 'thermal': 0, 'ping': 0},
          'recording': bool(i % 2),
          'params': {'ExperimentalMode': str(i % 2)},
        }
        for i in range(37)
      ],
    },
  )
)
cases.append(
  (
    'mici-invalid-version',
    {**compact, 'raw_params': {'Version': [255]}, 'params': {'GitBranch': 'nightly', 'GitCommit': 'abcdef0123', 'GitCommitDate': '1790812800'}},
  )
)
cases.append(('home-whitespace-notes', {'kind': 'home', 'params': {'UpdateAvailable': '1', 'UpdaterNewReleaseNotes': '\x1c <h2>Release notes</h2> \x1f'}}))
for name, date, address in [('whitespace', '1790812800\x1c+0000', '\x1c192.0.2.4\x1f'), ('year-outside', '253402300800', '\x1c')]:
  cases.append(
    (
      f'mici-date-{name}',
      {
        **compact,
        'params': {'Version': '0.10', 'GitBranch': 'nightly', 'GitCommit': 'abcdef0123', 'GitCommitDate': date},
        'address': address,
        'frames': 3,
        'steps': [{'frame': 1, 'now': 5.1}, {'frame': 2, 'now': 5.2}],
      },
    )
  )
results = []
for language in ['en', 'ko']:
  for name, overrides in cases:
    scene = {
      'kind': 'home',
      'config': {'big': True, 'large_viewport': True, 'pc': True, 'scale': 1.0},
      'language': language,
      'rect': {'x': 300, 'y': 0, 'width': 1860, 'height': 1080},
      'frames': 3,
      'prime': 0,
    }
    scene.update(overrides)
    case = f'{name}-{language}'
    path = args.output / f'{case}.json'
    path.write_text(json.dumps(scene))
    with tempfile.TemporaryDirectory(prefix='msgq_rust-probe-home148-', dir='/dev/shm') as ns:
      env = dict(os.environ, DISPLAY=args.display, PYTHONPATH=str(root), OFFSCREEN='1', OPENPILOT_PREFIX=Path(ns).name.removeprefix('msgq_'))
      for lane in ['source', 'native']:
        output = args.output / f'{lane}-{case}.png'
        command = (
          [sys.executable, str(root / 'rust/tools/ui_application_qa/home_source.py'), str(path), str(output)]
          if lane == 'source'
          else [str(args.binary), str(root), str(path), str(output)]
        )
        with (args.output / f'{lane}-{case}.log').open('w') as log:
          subprocess.run(command, env=env, stdout=log, stderr=subprocess.STDOUT, check=True)
    source = json.loads((args.output / f'source-{case}.json').read_text())
    native = json.loads((args.output / f'native-{case}.json').read_text())
    if name == 'sidebar-actions-gate':
      assert source[-1]['effects'] == ['settings', 'web', 'microphone'], source[-1]
    if name == 'home-reboot':
      assert source[-1]['effects'] == ['updater:Reboot'], source[-1]
    if name == 'home-snooze':
      assert source[-1]['snooze'] and source[-1]['state']['current'] == 'Home', source[-1]
    if name == 'home-acknowledge':
      assert not source[-1]['excessive'] and source[-1]['state']['current'] == 'Home', source[-1]
    if name == 'training-fast-fallback':
      assert source[2]['state']['step'] == 18 and source[2]['state']['uploaded'] == 3, source[2]
    if name == 'training-no-record-restart':
      assert source[-1]['state']['step'] == 0 and not source[-1]['record_front'] and not source[-1]['effects'], source[-1]
    if name == 'mici-long-allowed':
      assert source[-1]['state']['experimental'] and not source[-1]['effects'], source[-1]
    if name == 'mici-web-hold':
      assert source[-1]['effects'] == ['web'] and not source[-1]['state']['experimental'], source[-1]
    row = {'scene': case, 'state_equal': source == native, 'frames': []}
    for index in range(scene['frames']):
      a = np.asarray(Image.open(args.output / f'source-{case}-{index}.png'))
      b = np.asarray(Image.open(args.output / f'native-{case}-{index}.png'))
      row['frames'].append({'index': index, 'different_pixels': int(np.any(a != b, axis=-1).sum())})
    results.append(row)
    print(json.dumps(row), flush=True)
    (args.output / 'results.json').write_text(json.dumps(results, indent=2))
assert all(r['state_equal'] and all(f['different_pixels'] == 0 for f in r['frames']) for r in results), results
print('PASS exact state/actions/Params and every rendered frame')
