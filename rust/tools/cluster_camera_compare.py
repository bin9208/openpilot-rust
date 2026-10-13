"""Compare owned VisionIPC camera lifecycles and exact source/native raylib pixels."""

from __future__ import annotations

from dataclasses import asdict, dataclass
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
from typing import Literal

import numpy as np
from PIL import Image


ROOT = Path(__file__).resolve().parents[2]


@dataclass(frozen=True, slots=True)
class Step:
  now: float
  wide: bool = False
  select: bool = True
  draw: bool = True


@dataclass(frozen=True, slots=True)
class Scenario:
  name: str
  steps: tuple[Step, ...]
  commands: tuple[tuple[str, ...], ...]


@dataclass(frozen=True, slots=True)
class Run:
  binary: Path
  peer: Path
  output: Path
  viewport: tuple[int, int] = (384, 192)
  external: bool = False


def cases() -> tuple[Scenario, ...]:
  return (
    Scenario(
      'poll-stale',
      tuple(Step(now) for now in (10, 10.125, 10.1875, 10.25, 11.375, 11.5, 11.625, 11.75)),
      (
        ('streams both',),
        ('send road 32',),
        ('send road 33',),
        ('send road 34',),
        (),
        (),
        (),
        ('send road 35',),
      ),
    ),
    Scenario(
      'switch-cancel',
      tuple(Step(10 + index * 0.125, index in (2, 3, 6)) for index in range(8)),
      (
        ('streams both',),
        ('send road 32',),
        ('send road 33',),
        ('send wide 44',),
        ('send wide 45',),
        ('send road 46',),
        (),
        (),
      ),
    ),
    Scenario(
      'restart',
      tuple(Step(10 + index * 0.125) for index in range(7)),
      (
        ('streams both',),
        ('send road 32',),
        ('send road 33',),
        ('streams both', 'send road 80'),
        (),
        ('send road 81',),
        ('send road 82',),
      ),
    ),
    Scenario(
      'wide-only',
      tuple(Step(10 + index * 0.125) for index in range(5)),
      (
        ('streams wide',),
        ('send wide 60',),
        ('send wide 61',),
        (),
        ('send wide 62',),
      ),
    ),
    Scenario(
      'absence-discovery',
      tuple(Step(now, True) for now in (0.25, 0.375, 0.5, 1.0, 1.125, 1.25)),
      (
        ('streams none',),
        (),
        ('streams both',),
        ('send road 30',),
        ('send wide 40',),
        ('send wide 41',),
      ),
    ),
  )


def reply(peer: subprocess.Popen[str], log: Path, expected: str) -> None:
  assert peer.stdout is not None
  with log.open('a') as capture:
    while True:
      line = peer.stdout.readline()
      assert line, (peer.poll(), expected)
      capture.write(line)
      if line.strip() == expected:
        return


def capture(case: Scenario, run: Run, lane: Literal['source', 'native']) -> Path:
  output = run.output / case.name / lane
  output.mkdir(parents=True, exist_ok=True)
  scene_path = output.parent / 'scene.json'
  width, height = run.viewport
  scene = {
    'width': width,
    'height': height,
    'destination': {'x': 7.25, 'y': 5.5, 'width': width - 22.5, 'height': height - 18.25},
    'steps': [asdict(step) for step in case.steps],
    'external': run.external,
  }
  scene_path.write_text(json.dumps(scene, indent=2))
  with (
    tempfile.TemporaryDirectory(prefix='msgq_cluster253-', dir='/dev/shm') as namespace,
    tempfile.TemporaryDirectory(prefix='cluster253-sync-') as synchronization,
  ):
    environment = dict(os.environ, OFFSCREEN='1', OPENPILOT_PREFIX=Path(namespace).name.removeprefix('msgq_'))
    environment['PYTHONPATH'] = os.environ['UI_MSGQ_PYTHON'] + os.pathsep + str(ROOT)
    with subprocess.Popen([str(run.peer)], env=environment, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True) as peer:
      reply(peer, output / 'peer.log', 'READY')
      match lane:
        case 'source':
          command = [sys.executable, str(ROOT / 'rust/tools/cluster_camera_source.py'), str(ROOT), str(scene_path), str(output), synchronization]
        case 'native':
          command = [str(run.binary), str(ROOT), str(scene_path), str(output), synchronization]
      with (output / 'process.log').open('w') as log:
        (output / 'invocation.json').write_text(
          json.dumps(
            {
              'command': command,
              'peer': str(run.peer),
              'commands': case.commands,
              'display': os.environ['DISPLAY'],
              'binding': os.environ['UI_MSGQ_PYTHON'],
              'clamp_control': os.environ.get('CLUSTER_CAMERA_WRAP_CONTROL') == '1',
            },
            indent=2,
          )
        )
        with subprocess.Popen(command, env=environment, stdout=log, stderr=subprocess.STDOUT) as process:
          try:
            for index, commands in enumerate(case.commands):
              deadline = time.monotonic() + 20
              while not (Path(synchronization) / f'{index}.ready').is_file():
                assert process.poll() is None, (output / 'process.log').read_text()[-3000:]
                assert time.monotonic() < deadline, (lane, case.name, index)
                time.sleep(0.001)
              assert peer.stdin is not None
              for request in commands:
                peer.stdin.write(request + '\n')
                peer.stdin.flush()
                reply(peer, output / 'peer.log', 'OK')
              if run.external:
                with Path(os.environ['EGL_TRACE']).open('a') as provider:
                  provider.write(json.dumps(['frame', index]) + '\n')
              (Path(synchronization) / f'{index}.allow').write_text('allow')
            assert process.wait(timeout=20) == 0, (output / 'process.log').read_text()[-4000:]
            peer.stdin.write('stop\n')
            peer.stdin.flush()
            reply(peer, output / 'peer.log', 'OK')
            assert peer.wait(timeout=5) == 0
          finally:
            if process.poll() is None:
              process.kill()
              process.wait()
            if peer.poll() is None:
              peer.kill()
              peer.wait()
  return output


def compare(case: Scenario, run: Run) -> None:
  lanes = [capture(case, run, lane) for lane in ('source', 'native')]
  traces = [json.loads((lane / 'trace.json').read_text()) for lane in lanes]
  assert traces[0] == traces[1], (case.name, traces)
  assert traces[0]['cleanup'] == {'frame': False, 'streams': 0}
  differences = []
  for index in range(len(case.steps)):
    images = [np.asarray(Image.open(lane / f'frame-{index:03}.png')).astype(int) for lane in lanes]
    delta = abs(images[0] - images[1])
    differences.append({'frame': index, 'pixels': int(np.any(delta, axis=-1).sum()), 'max_channel': int(delta.max())})
  match case.name:
    case 'poll-stale':
      assert [row['frame']['id'] if row['frame'] else None for row in traces[0]['steps']] == [None, 32, 32, 34, 34, None, None, 35]
    case 'switch-cancel':
      assert [row['stream'] for row in traces[0]['steps']] == [0, 0, 0, 2, 2, 0, 0, 0]
    case 'restart':
      assert [row['frame']['id'] if row['frame'] else None for row in traces[0]['steps']] == [None, 32, 33, 33, 33, None, 82]
    case 'wide-only':
      assert traces[0]['steps'][0]['frame'] is None and traces[0]['steps'][-1]['stream'] == 2
    case 'absence-discovery':
      assert traces[0]['steps'][2]['streams'] == [] and traces[0]['steps'][-1]['frame']['id'] == 41
    case _:
      raise AssertionError(case.name)
  result = {
    'scenario': case.name,
    'frames': len(case.steps),
    'matching_state': True,
    'differences': differences,
    'pass': all(row['pixels'] == 0 for row in differences),
  }
  (run.output / case.name / 'result.json').write_text(json.dumps(result, indent=2))
  print(json.dumps(result), flush=True)
  assert result['pass'], result


def main() -> None:
  binary, peer, output = map(Path, sys.argv[1:])
  output = output.resolve()
  output.mkdir(parents=True)
  for viewport in ((384, 192), (800, 480)):
    run = Run(binary.resolve(), peer.resolve(), output / f'{viewport[0]}x{viewport[1]}', viewport)
    for case in cases():
      compare(case, run)
  files = [
    run.binary,
    run.peer,
    ROOT / 'openpilot/selfdrive/carrot/cluster/cluster_live_camera.py',
    ROOT / 'rust/tools/cluster_camera_source.py',
    ROOT / 'rust/tools/cluster_camera_compare.py',
  ]
  result = {
    'pass': True,
    'clamp_control': os.environ.get('CLUSTER_CAMERA_WRAP_CONTROL') == '1',
    'scenarios': len(cases()) * 2,
    'frames': sum(len(case.steps) for case in cases()) * 2,
    'hashes': {str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in files},
  }
  (output / 'result.json').write_text(json.dumps(result, indent=2))


if __name__ == '__main__':
  main()
