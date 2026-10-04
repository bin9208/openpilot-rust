import argparse
from dataclasses import dataclass
import json
import math
import os
from pathlib import Path
import subprocess
import sys
import tempfile

import numpy as np
from PIL import Image

from openpilot.cereal import log


@dataclass(frozen=True, slots=True)
class Scenario:
  name: str
  mode: int
  empty: bool = False
  torque: bool = True
  stale: bool = False
  fractional: bool = False


def messages(index: int, scenario: Scenario) -> list[list[int]]:
  value = math.sin(index / 19) * 7
  data = {
    'carState': {'aEgo': value, 'vEgo': value + 12, 'steeringAngleDeg': value * 4},
    'longitudinalPlan': {'accels': [] if scenario.empty else [value / 2], 'speeds': [] if scenario.empty else [value + 13]},
    'carControl': {'actuators': {'accel': -value / 4, 'steeringAngleDeg': value * 3, 'curvature': value / 1000}},
    'controlsState': {'lateralControlState': {'torqueState': {'actualLateralAccel': value / 10, 'desiredLateralAccel': value / 8,
                                                           'output': value / 7}} if scenario.torque else {'angleState': {}}},
    'modelV2': {'position': {'x': [] if scenario.empty else [value + i for i in range(33)]},
               'velocity': {'x': [] if scenario.empty else [value / 2 + i for i in range(33)]}},
    'radarState': {'leadOne': {'aLeadK': value / 3, 'vRel': value / 4, 'aLead': value / 5, 'jLead': -value / 6}},
    'liveParameters': {'angleOffsetDeg': value / 10},
  }
  result = []
  for name, payload in data.items():
    if scenario.stale and 30 <= index < 65 and name in ['longitudinalPlan', 'modelV2']:
      continue
    event = log.Event.new_message(valid=True)
    event.init(name)
    getattr(event, name).from_dict(payload)
    result.append(list(event.to_bytes()))
  return result


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--display', required=True)
  parser.add_argument('--filter', default='')
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  root = Path(__file__).resolve().parents[2]
  cases = [Scenario(f'mode-{mode}', mode) for mode in range(1, 9)]
  cases.extend([Scenario('empty-arrays', 3, empty=True), Scenario('angle-control', 6, torque=False),
                Scenario('freshness-resume', 1, stale=True), Scenario('mode-switch', 1),
                Scenario('fractional-rect', 4, fractional=True)])
  results = []
  for language in ['en', 'ko']:
    for scenario in cases:
      name = f'{scenario.name}-{language}'
      if args.filter and args.filter not in name:
        continue
      steps = []
      for index in range(325):
        mode = scenario.mode
        if scenario.name == 'mode-switch':
          mode = [1, 0, 2, 6, 9][min(index // 65, 4)]
        steps.append({'frame': index, 'mode': mode, 'now': index / 20, 'messages': messages(index, scenario)})
      scene = {'kind': 'plot', 'config': {'big': False, 'large_viewport': False, 'pc': True, 'scale': 1.0},
               'language': language, 'rect': {'x': -5.3 if scenario.fractional else 0,
                                             'y': 7.6 if scenario.fractional else 0,
                                             'width': 523.7 if scenario.fractional else 536,
                                             'height': 216.4 if scenario.fractional else 240},
               'frames': len(steps), 'prime': 0, 'params': {}, 'background': [31, 37, 43, 255],
               'capture_frames': list(range(len(steps))), 'plot': {'steps': steps}}
      path = args.output / f'{name}.json'
      path.write_text(json.dumps(scene))
      outputs = []
      with tempfile.TemporaryDirectory(prefix='msgq_rust-probe-plot148-', dir='/dev/shm') as namespace:
        env = dict(os.environ, DISPLAY=args.display, PYTHONPATH=os.environ['UI_MSGQ_PYTHON'] + os.pathsep + str(root),
                   OFFSCREEN='1', OPENPILOT_PREFIX=Path(namespace).name.removeprefix('msgq_'))
        commands = [([sys.executable, str(root / 'rust/tools/ui_application_qa/product_source.py')], 'source'),
                    ([str(args.binary), str(root)], 'native')]
        for command, lane in commands:
          output = args.output / f'{lane}-{name}.png'
          with output.with_suffix('.log').open('w') as stream:
            subprocess.run([*command, str(path), str(output)], env=env, stdout=stream, stderr=subprocess.STDOUT, check=True)
          outputs.append(output)
      traces = [json.loads(output.with_suffix('.json').read_text()) for output in outputs]
      states = [{'frame': frame, 'source': a, 'native': b} for frame, (a, b) in enumerate(zip(*traces, strict=True)) if a != b]
      pixels = []
      for frame in range(scene['frames']):
        images = [np.asarray(Image.open(output.with_name(f'{output.stem}-frame-{frame:04}.png'))) for output in outputs]
        if not np.array_equal(*images):
          pixels.append({'frame': frame, 'pixels': int(np.any(images[0] != images[1], axis=-1).sum())})
      result = {'scene': name, 'frames': len(traces[0]), 'state_differences': states, 'pixel_differences': pixels}
      results.append(result)
      (args.output / 'results.json').write_text(json.dumps(results, indent=2))
      print(json.dumps(result), flush=True)
      assert not states and not pixels, result
  assert results, 'no selected DebugPlot scenario'
  print(f'PASS {len(results)} original/native DebugPlot cases; exact messages, samples, gates and RGBA frames')


if __name__ == '__main__':
  main()
