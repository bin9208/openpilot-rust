# /// script
# dependencies = ["numpy", "pillow"]
# ///
# How to run: <existing-ui-python> rust/tools/check_ui_mici_offroad_alerts.py --binary PATH --output DIR --display :127
"""Compare real source/native Mici offroad alert pixels, lifecycle and touch outcomes."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
from typing import Literal, TypedDict
import numpy as np
from PIL import Image


class Step(TypedDict, total=False):
  frame: int
  now: float
  params: dict[str, str]
  remove: list[str]
  show: bool
  refresh: bool
  offset: float
  events: list[dict[str, int | float | bool | dict[str, int]]]


def event(frame: int, y: int, kind: Literal['press', 'move', 'release']) -> Step:
  return {'frame': frame, 'events': [{'pos': {'x': 260, 'y': y}, 'slot': 0, 'pressed': kind == 'press', 'released': kind == 'release', 'down': kind != 'release', 'time': frame / 20}]}


def payload(text: str, extra: str = '') -> str:
  return json.dumps({'text': text, 'extra': extra}, ensure_ascii=False)


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--display', required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  root = Path(__file__).resolve().parents[2]
  catalog = json.loads((root / 'openpilot/selfdrive/selfdrived/alerts_offroad.json').read_text())
  templates: list[tuple[str, dict[str, str], list[Step]]] = [
    ('empty', {}, []),
    ('update-version', {'UpdateAvailable': '1', 'UpdaterNewDescription': '0.10.0 / dev / abc123 / 2026-10-02'}, [event(2, 120, 'press'), event(4, 120, 'release'), {'frame': 6, 'offset': -84.0}]),
    ('update-fallback', {'UpdateAvailable': '1', 'UpdaterNewDescription': '0.10.0 / dev / abc123'}, []),
    ('small', {'Offroad_UpdateFailed': payload('Check hardware. Restore connection.')}, [event(2, 120, 'press'), event(4, 120, 'release')]),
    ('medium', {'Offroad_UpdateFailed': payload(catalog['Offroad_ConnectivityNeeded']['text'])}, []),
    ('big', {'Offroad_UpdateFailed': payload('Check hardware. ' + 'Check the cable and make sure all connectors are securely seated before continuing. ' * 4)}, [{'frame': 4, 'offset': -84.0}]),
    ('body-only', {'Offroad_TemperatureTooHigh': payload('No punctuation followed by whitespace: %1 then %1', 'extra')}, []),
    ('sentence-edges', {'Offroad_UpdateFailed': payload('  ?\u001fDone  ')}, []),
    ('korean', {'Offroad_UpdateFailed': payload('연결을 확인하세요. 모든 커넥터가 제대로 연결되었는지 확인한 후 장치를 다시 시작하세요.')}, [event(2, 120, 'press'), event(4, 120, 'release')]),
    ('catalog-scroll', {key: payload(config['text'], 'synthetic value') for key, config in catalog.items()}, [event(2, 210, 'press'), event(3, 180, 'move'), event(4, 130, 'move'), event(5, 70, 'move'), event(6, 45, 'release'), {'frame': 13, 'offset': -500.0}, {'frame': 18, 'show': True}]),
    ('refresh-boundary', {}, [
      {'frame': 1, 'now': 4.999, 'params': {'Offroad_UpdateFailed': payload('Alert becomes active. Body text.')}},
      {'frame': 2, 'now': 5.0},
      {'frame': 3, 'now': 9.999, 'remove': ['Offroad_UpdateFailed']},
      {'frame': 4, 'now': 10.0},
      {'frame': 6, 'params': {'UpdateAvailable': '1'}, 'show': True},
      {'frame': 8, 'params': {'UpdateAvailable': '0'}, 'refresh': True},
    ]),
    ('empty-json', {key: 'null' for key in catalog}, []),
    ('invalid-json', {'Offroad_UpdateFailed': '{malformed'}, []),
  ]
  results = []
  for language in ['en', 'ko']:
    for name, params, steps in templates:
      label = f'{name}-{language}'
      scene = {'config': {'big': False, 'large_viewport': False, 'pc': True, 'scale': 1.0}, 'language': language, 'frames': 24, 'prime': 0, 'params': params.copy(), 'raw_params': {}, 'steps': steps, 'capture_frames': [0, 2, 3, 4, 6, 8, 13, 18, 23], 'rect': {'x': 0, 'y': 0, 'width': 536, 'height': 240}}
      path = args.output / f'{label}.json'
      path.write_text(json.dumps(scene, ensure_ascii=False))
      outputs = []
      with tempfile.TemporaryDirectory(prefix='msgq_rust-probe-mici-offroad-alerts148-', dir='/dev/shm') as namespace:
        env = dict(os.environ, DISPLAY=args.display, PYTHONPATH=str(root), OFFSCREEN='1', OPENPILOT_PREFIX=Path(namespace).name.removeprefix('msgq_'))
        for lane in ['source', 'native']:
          output = args.output / f'{lane}-{label}.png'
          command = ([sys.executable, str(root / 'rust/tools/ui_application_qa/mici_offroad_alerts_source.py'), str(path), str(output)] if lane == 'source' else [str(args.binary), str(root), str(path), str(output)])
          with (args.output / f'{lane}-{label}.log').open('w') as log:
            subprocess.run(command, env=env, stdout=log, stderr=subprocess.STDOUT, check=True)
          outputs.append(output)
      states = [json.loads(output.with_suffix('.json').read_text()) for output in outputs]
      for index, (source, native) in enumerate(zip(*states, strict=True)):
        assert source == native, (label, index, source, native)
      if name == 'update-version':
        assert states[1][-1]['effects'] == ['reboot'], label
      if name == 'catalog-scroll':
        assert any(not frame['scrolling'] for frame in states[1]), label
        assert states[1][0]['active'] == len(catalog), label
      if name == 'refresh-boundary':
        assert [states[1][index]['active'] for index in [0, 1, 2, 3, 4, 6, 8]] == [0, 0, 1, 1, 0, 1, 0], label
      if name in ['small', 'medium', 'big'] and language == 'en':
        assert [item['size'] for item in states[1][0]['items'] if item['visible']] == [['small', 'medium', 'big'].index(name)], label
      row = {'scenario': label, 'trace_frames': len(states[0]), 'capture_frames': scene['capture_frames'], 'different_pixels': 0, 'max_channel_difference': 0}
      for index in scene['capture_frames']:
        images = [np.asarray(Image.open(output.with_name(f'{output.stem}-{index}.png'))).astype(int) for output in outputs]
        delta = abs(images[0] - images[1])
        row['different_pixels'] += int(np.any(delta != 0, axis=-1).sum())
        row['max_channel_difference'] = max(row['max_channel_difference'], int(delta.max()))
      results.append(row)
      (args.output / 'results.json').write_text(json.dumps(results, indent=2))
      print(json.dumps(row), flush=True)
      assert row['different_pixels'] == 0, row
  print(f'PASS: {len(results)} Mici offroad alert scenarios with identical traces and pixels')


if __name__ == '__main__':
  main()
