#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy", "pillow", "pycapnp", "python-xlib"]
# ///
# ─── How to run ───
# 1. Install uv (if not installed):
#      curl -LsSf https://astral.sh/uv/install.sh | sh
# 2. Run with repository native msgq modules on PYTHONPATH:
#      uv run check_ui_root.py [ARGS]
# 3. Or use the existing startup-ui Python environment without installing dependencies.
# ──────────────────

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import numpy as np
from PIL import Image
from check_ui_augmented import messages
from ui_application_qa.camera_lane import capture
from typing import TypedDict


class Position(TypedDict):
  x: int
  y: int


class MouseEvent(TypedDict):
  pos: Position
  slot: int
  pressed: bool
  released: bool
  down: bool
  time: float


class MouseStep(TypedDict):
  frame: int
  events: list[MouseEvent]


def click(frame: int, x: int, y: int) -> list[MouseStep]:
  return [
    {
      'frame': frame + index,
      'events': [{'pos': {'x': x, 'y': y}, 'slot': 0, 'pressed': index == 0, 'released': index == 1, 'down': index == 0, 'time': (frame + index) / 20}],
    }
    for index in range(2)
  ]


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--peer', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--display', required=True)
  parser.add_argument('--filter', default='')
  args = parser.parse_args()
  root = Path(__file__).resolve().parents[2]
  assert shutil.disk_usage(root).free > (25 + 0.5) * 1024**3
  args.output.mkdir(parents=True, exist_ok=True)
  results = []
  for big in [False, True]:
    for language in ['en', 'ko']:
      names = ['home', 'settings-click', 'transitions', 'sidebar'] if big else ['home', 'settings-click', 'transitions', 'alerts', 'plot', 'standstill']
      for name in names:
        label = f'{"big" if big else "compact"}-{name}-{language}'
        if args.filter and args.filter not in label:
          continue
        base = {'frame': 0, 'messages': messages(20.0, False), 'started': False, 'status': 0}
        scene = {
          'kind': 'main-root',
          'config': {'big': big, 'large_viewport': big, 'pc': True, 'scale': 1.0},
          'language': language,
          'rect': {'x': 0, 'y': 0, 'width': 2160 if big else 536, 'height': 1080 if big else 240},
          'frames': 32,
          'prime': 0,
          'params': {'IsMetric': '1', 'CustomSR': '135', 'DongleId': 'fixture', 'UpdaterState': 'idle'},
          'camera': {'stream': 0},
          'road': {'steps': [base]},
          'root': {'steps': []},
          'egpu': {'devices': []},
          'wifi': {
            'networks': [],
            'wifi_state': {'ssid': None, 'status': 'Disconnected'},
            'ipv4_address': '',
            'current_network_metered': 'Unknown',
            'connecting_to_ssid': None,
            'connected_ssid': None,
            'tethering_password': 'owned-password',
            'tethering_active': False,
            'saved_ssids': [],
          },
          'steps': [],
        }
        if name == 'settings-click':
          scene['steps'] = click(10, 150 if big else 32, 90 if big else 214)
        elif name in ['transitions', 'sidebar']:
          scene['frames'] = 100
          scene['road']['steps'] += [dict(base, frame=5, started=True), dict(base, frame=85, started=False)]
          if name == 'transitions':
            scene['root']['steps'] = [{'frame': 0, 'page': 'device'}]
          elif big:
            scene['steps'] = click(30, 1400, 500) + click(60, 1400, 500)
        elif name == 'alerts':
          scene['params']['UpdateAvailable'] = '1'
        elif name == 'plot':
          scene['frames'] = 130
          scene['road']['steps'] += [
            dict(base, frame=5, started=True),
            dict(base, frame=10, started=True, params={'ShowPlotMode': '1'}),
            dict(base, frame=45, started=True, params={'ClusterHudConnected': '1'}),
            dict(base, frame=60, started=True, params={'ClusterHudConnected': '0', 'ShowPlotMode': '8'}),
            dict(base, frame=105, started=False),
            dict(base, frame=110, started=False, params={'ShowPlotMode': '0'}),
            dict(base, frame=115, started=True),
          ]
        elif name == 'standstill':
          scene['frames'] = 130
          stopped = messages(0.0, False, standstill=True)
          scene['road']['steps'] += [
            dict(base, frame=5, started=True, messages=stopped),
            dict(base, frame=20, started=True),
            dict(base, frame=65, started=True, messages=stopped),
            dict(base, frame=95, started=True),
            dict(base, frame=110, started=False),
          ]
          scene['root']['steps'] = [
            {'frame': 0, 'page': 'device'},
            {'frame': 15, 'timeout': True},
            {'frame': 75, 'page': 'device'},
            {'frame': 80, 'timeout': True},
            {'frame': 115, 'page': 'device'},
            {'frame': 125, 'timeout': True},
          ]
        scene['capture_frames'] = list(range(scene['frames']))
        path = args.output / f'{label}.json'
        path.write_text(json.dumps(scene))
        traces = [
          capture(root, args.binary.resolve(), args.peer.resolve(), path, args.output / f'{lane}-{label}.png', args.display, scene['frames'], lane)
          for lane in ['source', 'native']
        ]
        state = [{'frame': index, 'source': a, 'native': b} for index, (a, b) in enumerate(zip(*traces, strict=True)) if a != b]
        pixels = []
        for frame in range(scene['frames']):
          images = [np.asarray(Image.open(args.output / f'{lane}-{label}-frame-{frame:04}.png')).astype(np.int16) for lane in ['source', 'native']]
          delta = np.abs(images[0] - images[1])
          if np.any(delta):
            pixels.append({'frame': frame, 'different_pixels': int(np.any(delta, axis=-1).sum()), 'maximum_channel': int(delta.max())})
        result = {'scene': label, 'frames': len(traces[0]), 'state_differences': state, 'pixel_differences': pixels}
        results.append(result)
        (args.output / 'results.json').write_text(json.dumps(results, indent=2))
        print(json.dumps(result), flush=True)
        assert not state and not pixels, result
  (args.output / 'receipt.json').write_text(
    json.dumps(
      {
        'verdict': 'PASS',
        'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest(),
        'scenarios': len(results),
        'frames': sum(result['frames'] for result in results),
      },
      indent=2,
    )
  )
  print('PASS unchanged original MainLayout/MiciMainLayout versus native root pixels, navigation and transitions')


if __name__ == '__main__':
  main()
