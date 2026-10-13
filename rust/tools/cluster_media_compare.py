from __future__ import annotations

import ast
from dataclasses import asdict
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
from types import SimpleNamespace


def original(root):
  sys.path.insert(0, str(root))
  sys.path.insert(0, str(root / 'openpilot/selfdrive/carrot/cluster'))
  from check_message_state import source
  from cluster_models import NaviDashboardState, NaviItemStatus, NaviMediaFrame

  path = root / 'openpilot/selfdrive/carrot/cluster/cluster_navi_source.py'
  tree = ast.parse(path.read_text())
  selected = [
    node for node in tree.body if isinstance(node, (ast.ClassDef, ast.FunctionDef)) and node.name in ('NaviIpcMediaSource', '_ipc_value', '_is_new_h264_result')
  ]
  clock, queued = [10.0], []

  class Worker:
    def reset(self):
      pass

    def discard(self, _):
      pass

    def poll(self):
      return ()

    def close(self):
      pass

  catalog_tree = ast.parse((root / 'openpilot/selfdrive/carrot/carrot_navi.py').read_text())
  names = {}
  for node in catalog_tree.body:
    if isinstance(node, ast.Assign) and isinstance(node.targets[0], ast.Name) and node.targets[0].id in ('JSON_NAMES', 'IMAGE_NAMES', 'RENDER_NAMES'):
      names[node.targets[0].id] = ast.literal_eval(node.value)
  keys = tuple(
    f'{kind}:{name}' for kind, values in [('json', names['JSON_NAMES']), ('image', names['IMAGE_NAMES']), ('render', names['RENDER_NAMES'])] for name in values
  )
  scope = {
    'Any': object,
    'NaviDashboardState': NaviDashboardState,
    'NaviItemStatus': NaviItemStatus,
    'NaviMediaFrame': NaviMediaFrame,
    'H264DecodeWorker': Worker,
    'NAVI_IPC_ITEM_KEYS': keys,
    'NAVI_IPC_WAITING_ITEMS': tuple(NaviItemStatus(key, 0, False, 'waiting') for key in keys),
    'time': SimpleNamespace(monotonic=lambda: clock[0]),
    'NAVI_IPC_DISCONNECT_TIMEOUT_S': 3.0,
    'MAP_FRAME_STALE_TIMEOUT_MS': 3000,
  }
  exec(compile(ast.fix_missing_locations(ast.Module(body=selected, type_ignores=[])), str(path), 'exec'), scope)
  owner = scope['NaviIpcMediaSource'](SimpleNamespace(sub_sock=lambda *_, **__: None, drain_sock=lambda _: tuple(queued)))
  messaging, _ = source()
  return owner, messaging['new_message'], clock, queued


def run(root, binary, output):
  output.mkdir(parents=True, exist_ok=False)
  owner, message, clock, queued = original(root)
  steps, expected = [], []

  def packet(fields):
    event = message('carrotNaviMedia')
    event.carrotNaviMedia.from_dict(
      {
        'schemaVersion': 1,
        'sessionId': 'source',
        'kind': 'render',
        'name': 'map_main',
        'sequence': 1,
        'present': True,
        'messageType': 1,
        'formatOrReason': 1,
        'width': 24,
        'height': 16,
        'payload': b'owned PNG/JPEG packet',
        **fields,
      }
    )
    return event

  def step(now, packets=(), session=None):
    clock[0] = now
    queued[:] = [event.as_reader() for event in packets]
    paths = []
    for index, event in enumerate(packets):
      path = output / f'{len(steps):04}-{index}.capnp'
      path.write_bytes(event.to_bytes())
      paths.append(str(path))
    projected = asdict(owner.update(SimpleNamespace(session_id=session) if session is not None else None))
    for frame in projected['media']:
      if frame['data'] is not None:
        frame['data'] = list(frame['data'])
    expected.append(json.loads(json.dumps(projected)))
    steps.append({'now': now, 'messages': paths, 'session': session})

  step(10.0)
  step(10.1, [packet({'sequence': 1}), packet({'sequence': 18446744073709551615})])
  for now in [13.1, 13.101, 13.102]:
    step(now, session='source')
  step(14.0, [packet({'kind': 'image', 'name': 'tbt_current_full', 'formatOrReason': 2, 'sequence': 10})])
  step(14.1, [packet({'schemaVersion': 0, 'sequence': 11}), packet({'kind': 'json', 'sequence': 12})])
  step(14.2, [packet({'messageType': 2, 'sequence': 13})])
  step(14.3, [packet({'messageType': 7, 'sequence': 14})])
  step(14.4, [packet({'present': False, 'reason': 'cleared', 'sequence': 15})])
  step(14.5, [packet({'messageType': 4, 'sequence': 16})])
  step(14.6, [packet({'sessionId': 'other', 'sequence': 1})])
  for now in [17.6, 17.6001, 18.0]:
    step(now)
  step(18.1, session='still-connected')
  step(18.2, [packet({'sessionId': '', 'name': 'custom_media', 'sequence': 3})])
  step(18.3, [packet({'name': '', 'sequence': 4})])
  case = output / 'input.json'
  case.write_text(json.dumps(steps))
  with tempfile.TemporaryDirectory(prefix='msgq_rust-probe-cluster-media-', dir='/dev/shm') as namespace:
    command = [str(binary), str(case)]
    result = subprocess.run(
      command, env={**os.environ, 'OPENPILOT_PREFIX': Path(namespace).name.removeprefix('msgq_')}, capture_output=True, text=True, timeout=30
    )
  (output / 'invocation.json').write_text(
    json.dumps({'command': command, 'exit_code': result.returncode, 'stdout': result.stdout, 'stderr': result.stderr}, indent=2)
  )
  assert result.returncode == 0, result.stderr
  actual = json.loads(result.stdout)
  (output / 'expected.json').write_text(json.dumps(expected, indent=2))
  (output / 'actual.json').write_text(json.dumps(actual, indent=2))
  mismatches = [{'step': index, 'expected': left, 'actual': right} for index, (left, right) in enumerate(zip(expected, actual, strict=True)) if left != right]
  receipt = {
    'steps': len(steps),
    'pass': not mismatches,
    'mismatches': mismatches[:3],
    'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
    'scope': 'Original non-conflated media projection/epochs/stale/cache; codec worker excluded',
  }
  (output / 'result.json').write_text(json.dumps(receipt, indent=2))
  print(json.dumps(receipt))
  assert not mismatches


if __name__ == '__main__':
  run(*(Path(value).resolve() for value in sys.argv[1:]))
