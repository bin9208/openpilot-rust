"""Capture original worker ownership with actual file capture and IPC peers."""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import selectors
import shutil
import subprocess
import sys
import tempfile
import time

import av
import numpy as np


def fixture(path: Path, width: int, height: int, count: int) -> None:
  with av.open(str(path), 'w', format='matroska') as container:
    stream = container.add_stream('ffv1', rate=25)
    stream.width, stream.height, stream.pix_fmt = width, height, 'bgr0'
    for index in range(count):
      array = np.random.default_rng(index + width).integers(0, 256, (height, width, 3), dtype=np.uint8)
      for packet in stream.encode(av.VideoFrame.from_ndarray(array, format='bgr24')):
        container.mux(packet)
    for packet in stream.encode():
      container.mux(packet)


def ready(process: subprocess.Popen[bytes], marker: bytes = b'READY') -> str:
  assert process.stdout is not None
  deadline, captured = time.monotonic() + 5, b''
  with selectors.DefaultSelector() as selector:
    selector.register(process.stdout, selectors.EVENT_READ)
    while time.monotonic() < deadline:
      if not selector.select(max(0, deadline - time.monotonic())):
        break
      chunk = os.read(process.stdout.fileno(), 4096)
      assert chunk, f'child exited before readiness: {captured!r}'
      captured += chunk
      if any(line.startswith(marker) for line in captured.splitlines()):
        return captured.decode()
  raise TimeoutError(f'owned process readiness failed: {captured!r}')


def stop(process: subprocess.Popen[bytes] | None) -> None:
  if process is not None and process.poll() is None:
    process.terminate()
    try:
      process.wait(timeout=3)
    except subprocess.TimeoutExpired:
      process.kill()
      process.wait(timeout=3)


def frame_projection(items: list[dict[str, object]]) -> list[tuple[object, ...]]:
  fields = ('stream', 'frame_id', 'sof', 'eof', 'valid', 'sha256')
  return sorted(tuple(item[key] for key in fields) for item in items)


def source_case(folder: Path, name: str, binary: Path | None = None, source: Path | None = None) -> dict[str, object]:
  folder.mkdir()
  road, wide = folder / 'road.mkv', folder / 'wide.mkv'
  if source is not None:
    shutil.copyfile(source / 'road.mkv', road)
    if (source / 'wide.mkv').is_file():
      shutil.copyfile(source / 'wide.mkv', wide)
  else:
    fixture(road, 128, 72, 8)
  if source is None and name == 'unequal-eof':
    fixture(wide, 128, 72, 3)
  elif source is None and name == 'odd-worker':
    fixture(wide, 3, 6, 2)
  elif source is None and name == 'odd-height':
    fixture(wide, 4, 3, 2)
  elif source is None and name == 'unaligned-even':
    fixture(wide, 2, 2, 3)
  elif name not in ('unequal-eof', 'odd-worker', 'unavailable-worker', 'odd-height', 'unaligned-even'):
    raise ValueError(f'unknown case {name}')
  specification = folder / 'spec.json'
  specification.write_text(json.dumps([{'kind': 'road', 'input': str(road)}, {'kind': 'wide', 'input': str(wide)}]) + '\n')
  tools = Path(__file__).parent
  argv = [sys.executable, '-P', '-u', str(tools / 'webcam_worker_source.py'), str(folder), str(specification)]
  if binary is not None:
    argv = [str(binary), str(folder), str(specification)]
  peer_argv = [sys.executable, '-P', '-u', str(tools / 'webcam_ipc_peer.py'), str(folder / 'peer-result.json')]
  producer = peer = None
  started = time.monotonic()
  with tempfile.TemporaryDirectory(prefix='msgq_webcam255_', dir='/dev/shm') as namespace:
    environment = dict(os.environ, OPENPILOT_PREFIX=Path(namespace).name.removeprefix('msgq_'), WEBCAM_OWNED_ROOT=str(folder))
    preload = environment.pop('WEBCAM_NATIVE_PRELOAD', None)
    producer_environment = dict(environment)
    if binary is not None and preload:
      producer_environment['LD_PRELOAD'] = preload
    with (folder / 'producer.stderr').open('wb') as stderr, (folder / 'peer.stderr').open('wb') as peer_stderr:
      try:
        producer = subprocess.Popen(argv, env=producer_environment, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=stderr)
        pre = ready(producer)
        peer = subprocess.Popen(peer_argv, env=environment, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=peer_stderr)
        peer_pre = ready(peer)
        stdout, _ = producer.communicate(b'RUN\n', timeout=5)
        peer_stdout, _ = peer.communicate(b'STOP\n', timeout=3)
        (folder / 'producer.stdout').write_text(pre + stdout.decode())
        (folder / 'peer.stdout').write_text(peer_pre + peer_stdout.decode())
        row = {
          'case': name,
          'argv': argv,
          'peer_argv': peer_argv,
          'prefix': environment['OPENPILOT_PREFIX'],
          'returncode': producer.returncode,
          'peer_returncode': peer.returncode,
          'elapsed': time.monotonic() - started,
          'worker': json.loads((folder / 'worker-result.json').read_text()),
          'peer': json.loads((folder / 'peer-result.json').read_text()),
        }
        (folder / 'result.json').write_text(json.dumps(row, indent=2) + '\n')
        return row
      finally:
        try:
          stop(peer)
        finally:
          stop(producer)


def main() -> None:
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument('output', type=Path)
  parser.add_argument('--reserve-mib', type=int, default=32)
  parser.add_argument('--native', type=Path)
  parser.add_argument('--source', type=Path)
  parser.add_argument('--cases', nargs='+', default=['unequal-eof', 'odd-worker', 'unavailable-worker'])
  args = parser.parse_args()
  output = args.output.resolve()
  if shutil.disk_usage(output.parent).free < 25 * 1024**3 + args.reserve_mib * 1024**2:
    raise OSError('owned webcam worker fixtures require disk reserve')
  output.mkdir(exist_ok=False)
  assert (args.native is None) == (args.source is None)
  rows = [source_case(output / name, name, args.native, args.source / name if args.source else None) for name in args.cases]
  (output / 'result.json').write_text(json.dumps(rows, indent=2) + '\n')
  print(
    json.dumps(
      [
        {
          'case': row['case'],
          'returncode': row['returncode'],
          'peer_returncode': row['peer_returncode'],
          'layouts': row['peer']['layouts'],
          'vision_frames': len(row['peer']['frames']),
          'failures': row['worker'].get('failures'),
          'camera_end': row['worker']['cameras_before_caller_cleanup'],
        }
        for row in rows
      ],
      indent=2,
    )
  )
  for row in rows:
    assert row['returncode'] == row['peer_returncode'] == 0
    road = [item['frame_id'] for item in row['peer']['frames'] if item['stream'] == 0]
    assert road == list(range(8)), road
    wide = [item['frame_id'] for item in row['peer']['frames'] if item['stream'] == 2]
    assert wide == (list(range(3)) if row['case'] in ('unequal-eof', 'unaligned-even') else []), wide
    if args.source:
      reference = json.loads((args.source / row['case'] / 'result.json').read_text())
      assert row['peer']['layouts'] == reference['peer']['layouts']
      assert frame_projection(row['peer']['frames']) == frame_projection(reference['peer']['frames'])
      from openpilot.cereal import log

      def values(messages):
        result = {}
        for item in messages:
          with log.Event.from_bytes(Path(item['raw']).read_bytes()) as message:
            data = message.to_dict()
            data.pop('logMonoTime')
            result[item['service'], item['frame_id']] = data
        return result

      assert values(row['worker']['messages']) == values(reference['worker']['messages'])
      for native, original in zip(row['worker']['cameras_before_caller_cleanup'], reference['worker']['cameras_before_caller_cleanup'], strict=True):
        assert (native['service'], native['info']['width'], native['info']['height'], native['frame_id'], native['opened']) == (
          original['service'],
          original['width'],
          original['height'],
          original['frame_id'],
          original['opened'],
        )
      assert bool(row['worker']['cameras_before_caller_cleanup'][1]['failure']) == (row['case'] in ('odd-worker', 'odd-height'))
      descriptors = row['worker']['descriptors']
      assert descriptors['before'] == descriptors['after_drop']
      assert row['worker']['listener_removed']
      if row['case'] in ('odd-worker', 'odd-height', 'unavailable-worker'):
        assert row['worker']['native_client_rejections'][0]['service'] == 'wideRoadCameraState'
      elif row['case'] == 'unaligned-even':
        observed = row['worker']['native_unaligned_frames']
        assert len(observed) == 1 and observed[0]['frame_id'] == observed[0]['buffer_frame_id'] == 0
        assert bytes(observed[0]['bytes']) == (args.source / row['case'] / 'vision-2-0.nv12').read_bytes()


if __name__ == '__main__':
  main()
