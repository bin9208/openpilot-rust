# /// script
# dependencies = ["numpy", "pillow"]
# ///
# How to run: UI_MSGQ_PYTHON=<owned extension> <ui-venv>/python rust/tools/check_ui_alerts.py --binary <product_render> --output <evidence> --display :125
"""Compare real source/native alert rendering, state transitions and exact pixels."""

import argparse
import copy
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

import numpy as np
from PIL import Image


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--display', required=True)
  parser.add_argument('--filter', default='')
  parser.add_argument('--cache', action='store_true')
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  root = Path(__file__).resolve().parents[2]
  results = []
  alerts = [
    ('small', {'text1': 'Ready to engage', 'text2': 'Check road', 'size': 1, 'status': 0}),
    ('mid', {'text1': 'Keep eyes on road', 'text2': 'Driver distracted', 'size': 2, 'status': 1}),
    ('full-short', {'text1': 'BRAKE', 'text2': 'Collision Warning', 'size': 3, 'status': 2}),
    ('full-long', {'text1': 'TAKE CONTROL IMMEDIATELY', 'text2': 'System Unresponsive', 'size': 3, 'status': 2}),
    ('full-newline', {'text1': 'Turn left\nthen continue', 'text2': 'Watch traffic', 'size': 3, 'status': 0}),
    ('calibration', {'text1': 'Calibrating: 54%', 'text2': 'Drive above 24 mph on a straight road', 'size': 2, 'status': 0}),
    ('korean', {'text1': '전방을 주시하세요', 'text2': '즉시 핸들을 잡으세요', 'size': 2, 'status': 1}),
    ('fade', {'text1': 'Standstill', 'text2': 'Resume driving', 'size': 1, 'status': 0}),
  ]
  for big in [True, False]:
    templates = []
    for name, alert in alerts:
      steps = [{'frame': 0, 'alert': alert}]
      if name == 'fade':
        steps += [{'frame': 8, 'alert': {'text1': '', 'text2': '', 'size': 0, 'status': 0}}]
      templates.append((name, steps, None))
    if not big:
      lane_alert = {'text1': 'Change lane', 'text2': 'Apply steering', 'size': 1, 'status': 0}
      templates.append(('lane-icons', [
        {'frame': 0, 'alert': dict(lane_alert, alert_type='preLaneChangeLeft/warning')},
        {'frame': 6, 'alert': dict(lane_alert, alert_type='laneChange/warning'), 'right': True},
        {'frame': 12, 'alert': dict(lane_alert, alert_type='laneChangeBlocked/warning'), 'left': True},
        {'frame': 18, 'alert': dict(lane_alert, alert_type='laneChangeBlocked/warning')},
        {'frame': 24, 'alert': dict(lane_alert, alert_type='preLaneChangeRight/warning')},
        {'frame': 30, 'alert': dict(lane_alert, alert_type='laneChange/warning'), 'left': True},
      ], None))
      templates.append(('lane-no-blinker', [
        {'frame': 0, 'alert': dict(lane_alert, alert_type='laneChange/warning')},
        {'frame': 10, 'alert': dict(lane_alert, alert_type='laneChangeBlocked/warning')},
        {'frame': 20, 'alert': dict(lane_alert, alert_type='unknown/warning')},
      ], {'x': 32.3, 'y': 4.7, 'width': 486.4, 'height': 227.3}))
    if args.cache:
      pending = {'text1': 'openpilot Unavailable', 'text2': 'Waiting to start', 'size': 2, 'status': 0}
      templates = [('fallback-cache', [
        {'frame': 0, 'alert': pending, 'publish': False, 'now': 6.0},
        {'frame': 8, 'alert': {'text1': '', 'text2': '', 'size': 0, 'status': 0}, 'now': 6.4},
      ], None)]
    for language in ['en', 'ko']:
      for label, steps, rect in templates:
        name = f'{"large" if big else "compact"}-{label}-{language}'
        if args.filter and args.filter not in name:
          continue
        scene = {
          'kind': 'alert', 'config': {'big': big, 'large_viewport': big, 'pc': True, 'scale': 1.0},
          'language': language, 'rect': rect or {'x': 0, 'y': 0, 'width': 2160 if big else 536, 'height': 1080 if big else 240},
          'frames': 40, 'prime': 0, 'params': {}, 'steps': [], 'capture_frames': list(range(40)),
          'alert': {'steps': copy.deepcopy(steps)},
        }
        if args.cache:
          scene['alert'].update(initial=pending, started_frame=100)
        path = args.output / f'{name}.json'
        path.write_text(json.dumps(scene))
        outputs = []
        with tempfile.TemporaryDirectory(prefix='msgq_rust-probe-alert148-', dir='/dev/shm') as namespace:
          env = dict(os.environ, DISPLAY=args.display,
                     PYTHONPATH=os.environ['UI_MSGQ_PYTHON'] + os.pathsep + str(root),
                     OFFSCREEN='1', OPENPILOT_PREFIX=Path(namespace).name.removeprefix('msgq_'))
          for lane in ['source', 'native']:
            output = args.output / f'{lane}-{name}.png'
            command = ([sys.executable, str(root / 'rust/tools/ui_application_qa/product_source.py'), str(path), str(output)]
                       if lane == 'source' else [str(args.binary), str(root), str(path), str(output)])
            with (args.output / f'{lane}-{name}.log').open('w') as log:
              subprocess.run(command, env=env, stdout=log, stderr=subprocess.STDOUT, check=True)
            outputs.append(output)
        states = [json.loads(output.with_suffix('.json').read_text()) for output in outputs]
        for index, (source, native) in enumerate(zip(*states, strict=True)):
          assert source == native, (name, index, source, native)
        row = {'scene': name, 'frames': len(states[0]), 'different_pixels': 0, 'max_channel_difference': 0}
        for index in scene['capture_frames']:
          images = [np.asarray(Image.open(output.with_name(f'{output.stem}-frame-{index:04}.png'))).astype(int) for output in outputs]
          delta = abs(images[0] - images[1])
          row['different_pixels'] += int(np.any(delta != 0, axis=-1).sum())
          row['max_channel_difference'] = max(row['max_channel_difference'], int(delta.max()))
        results.append(row)
        (args.output / 'results.json').write_text(json.dumps(results, indent=2))
        print(json.dumps(row), flush=True)
        assert row['different_pixels'] == 0, row
  print(f'PASS: {len(results)} source/native alert cases with exact frames and render-return/selection traces')


if __name__ == '__main__':
  main()
