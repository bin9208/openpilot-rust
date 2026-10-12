"""Exercise original/native cluster EGL ownership with a controlled foreign provider."""

from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import sys

import numpy as np
from PIL import Image

from cluster_camera_compare import Run, Scenario, Step, capture


def provider_trace(path: Path) -> list[list[str | int | bool]]:
  rows = [json.loads(line) for line in path.read_text().splitlines()]
  for row in rows:
    if row[0] == 'bind':
      row[2] = 'camera-texture'
  return rows


def compare(case: str, run: Run) -> None:
  os.environ['EGL_CASE'] = case
  scenario = Scenario(
    case,
    tuple(Step(10 + index * 0.125, index in (6, 7, 8, 9), draw=index != 7) for index in range(11)),
    (
      ('streams both',),
      ('send road 32',),
      (),
      (),
      (),
      ('send road 36',),
      ('send road 37',),
      ('send wide 44',),
      (),
      ('send wide 45',),
      ('send road 48',),
    ),
  )
  paths = []
  for lane in ('source', 'native'):
    path = run.output / case / lane
    path.mkdir(parents=True)
    provider = path / 'egl.jsonl'
    provider.write_text('')
    os.environ['EGL_TRACE'] = str(provider)
    paths.append(capture(scenario, run, lane))
  states = [json.loads((path / 'trace.json').read_text()) for path in paths]
  assert states[0] == states[1], (case, states)
  providers = [provider_trace(path / 'egl.jsonl') for path in paths]
  assert providers[0] == providers[1], (case, providers)
  fds = [json.loads((path / 'fd-delta.json').read_text()) for path in paths]
  assert fds == [{'delta': 0}, {'delta': 0}], (case, fds)
  copied = []
  for index, row in enumerate(states[0]['steps']):
    if not row['zero_copy']:
      images = [np.asarray(Image.open(path / f'frame-{index:03}.png')).astype(int) for path in paths]
      delta = abs(images[0] - images[1])
      assert not delta.any(), (case, index, int(delta.max()))
      copied.append(index)
  if case in ('fail', 'retry-fail'):
    assert states[0]['steps'][1]['zero_copy'] and not states[0]['steps'][3]['zero_copy']
    assert sum(row[0] == 'create' for row in providers[0]) == (6 if case == 'retry-fail' else 3)
  if case in ('success', 'retry', 'fallback'):
    assert all(row['zero_copy'] for row in states[0]['steps'])
    assert sum(row[0] == 'active' for row in providers[0]) == 9
    assert sum(row[0] == 'destroy' for row in providers[0]) >= 2
  result = {
    'case': case,
    'state_equal': True,
    'provider_equal': True,
    'fd_deltas': fds,
    'copy_frames_exact': copied,
    'external_pixels_verified': False,
    'create_calls': sum(row[0] == 'create' for row in providers[0]),
    'pass': True,
  }
  (run.output / case / 'result.json').write_text(json.dumps(result, indent=2))
  print(json.dumps(result), flush=True)


def main() -> None:
  binary, peer, output = map(Path, sys.argv[1:])
  run = Run(binary.resolve(), peer.resolve(), output.resolve(), external=True)
  cases = ('success', 'retry', 'fallback', 'fail', 'retry-fail', 'no-display', 'init-fail')
  for case in cases:
    compare(case, run)
  result = {'pass': True, 'cases': len(cases), 'frames': len(cases) * 11, 'binary_sha256': hashlib.sha256(run.binary.read_bytes()).hexdigest()}
  (run.output / 'result.json').write_text(json.dumps(result, indent=2))


if __name__ == '__main__':
  main()
