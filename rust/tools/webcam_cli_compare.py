"""Compare real Linux webcam entrypoints with exact owned open aliases."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time

from webcam_worker_compare import fixture, ready, stop


def run(folder: Path, argv: list[str], shim: Path) -> dict[str, object]:
  folder.mkdir()
  environment = dict(os.environ, WEBCAM_OWNED_ROOT=str(folder), LD_PRELOAD=str(shim))
  aliases = {}
  for name, count in (('ROAD', 8), ('WIDE', 3), ('DRIVER', 5)):
    target = folder / f'{name.lower()}.mkv'
    fixture(target, 128, 72, count)
    alias = f'/dev/videowebcam255_{folder.name}_{name.lower()}.mkv'
    assert not Path(alias).exists() and not Path(alias).is_symlink()
    environment[f'{name}_CAM'] = alias.removeprefix('/dev/video')
    environment[f'WEBCAM_OWNED_{name}_ALIAS'] = alias
    environment[f'WEBCAM_OWNED_{name}_FILE'] = str(target)
    aliases[alias] = str(target)
  peer_argv = [sys.executable, '-P', '-u', str(Path(__file__).with_name('webcam_ipc_peer.py')), str(folder / 'peer-result.json'), '--before-server', '--cereal']
  producer = peer = None
  started = time.monotonic()
  with tempfile.TemporaryDirectory(prefix='msgq_webcam255_', dir='/dev/shm') as namespace:
    environment['OPENPILOT_PREFIX'] = Path(namespace).name.removeprefix('msgq_')
    peer_environment = dict(environment)
    peer_environment.pop('LD_PRELOAD')
    with (folder / 'producer.stderr').open('wb') as stderr, (folder / 'peer.stderr').open('wb') as peer_stderr:
      try:
        peer = subprocess.Popen(peer_argv, env=peer_environment, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=peer_stderr)
        prefix = ready(peer, b'PEER_READY')
        producer = subprocess.Popen(argv, env=environment, stdout=subprocess.PIPE, stderr=stderr)
        stdout, _ = producer.communicate(timeout=5)
        peer_stdout, _ = peer.communicate(b'STOP\n', timeout=3)
        (folder / 'producer.stdout').write_bytes(stdout)
        (folder / 'peer.stdout').write_text(prefix + peer_stdout.decode())
        row = {
          'argv': argv,
          'peer_argv': peer_argv,
          'aliases': aliases,
          'prefix': environment['OPENPILOT_PREFIX'],
          'returncode': producer.returncode,
          'peer_returncode': peer.returncode,
          'elapsed': time.monotonic() - started,
          'stdout': stdout.decode(),
          'peer': json.loads((folder / 'peer-result.json').read_text()),
        }
        (folder / 'result.json').write_text(json.dumps(row, indent=2) + '\n')
        assert row['returncode'] == row['peer_returncode'] == 0, row
        trace = (folder / 'producer.stderr').read_text()
        for alias, target in aliases.items():
          assert f'OWNED_OPEN {alias} => {target}' in trace, trace
          assert alias in row['stdout'], row['stdout']
          assert not Path(alias).exists() and not Path(alias).is_symlink()
        assert not Path(f'/tmp/{environment["OPENPILOT_PREFIX"]}_visionipc_camerad').exists()
        return row
      finally:
        try:
          stop(peer)
        finally:
          stop(producer)


def message_fields(messages: list[dict[str, object]]) -> dict[tuple[str, int], object]:
  from openpilot.cereal import log

  values = {}
  for item in messages:
    with log.Event.from_bytes(Path(item['raw']).read_bytes()) as message:
      data = message.to_dict()
      data.pop('logMonoTime')
      values[item['service'], item['frame_id']] = data
  return values


def main() -> None:
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument('output', type=Path)
  parser.add_argument('--native', type=Path, required=True)
  parser.add_argument('--shim', type=Path, required=True)
  parser.add_argument('--reserve-mib', type=int, default=32)
  parser.add_argument('--source-result', type=Path)
  args = parser.parse_args()
  output = args.output.resolve()
  assert shutil.disk_usage(output.parent).free >= 25 * 1024**3 + args.reserve_mib * 1024**2
  output.mkdir(exist_ok=False)
  source = (
    json.loads(args.source_result.read_text())
    if args.source_result
    else run(output / 'source', [sys.executable, '-P', '-u', '-m', 'openpilot.tools.webcam.camerad'], args.shim)
  )
  native = run(output / 'native', [str(args.native)], args.shim)
  fields = ('stream', 'frame_id', 'sof', 'eof', 'valid', 'sha256')

  # Original publisher generation reset and descriptor connection can precede
  # the first recipient read. Retain every capture, but this entrypoint control
  # compares the declared post-startup interval; worker controls cover ID zero.
  def frames(row):
    return sorted(tuple(frame[key] for key in fields) for frame in row['peer']['frames'] if frame['frame_id'] >= 1)

  assert source['peer']['layouts'] == native['peer']['layouts']
  assert frames(source) == frames(native), (frames(source), frames(native))
  for row in (source, native):
    for stream, count in ((0, 8), (2, 3), (1, 5)):
      assert [frame['frame_id'] for frame in row['peer']['frames'] if frame['stream'] == stream and frame['frame_id'] >= 1] == list(range(1, count))
    for service, count in (('roadCameraState', 8), ('wideRoadCameraState', 3), ('driverCameraState', 5)):
      assert [item['frame_id'] for item in row['peer']['messages'] if item['service'] == service and item['frame_id'] >= 1] == list(range(1, count))

  def compared(row):
    return [item for item in row['peer']['messages'] if item['frame_id'] >= 1]

  assert message_fields(compared(source)) == message_fields(compared(native))
  clocks = []
  from openpilot.cereal import log

  for row in (source, native):
    for service in ('roadCameraState', 'wideRoadCameraState', 'driverCameraState'):
      stamps = []
      for item in row['peer']['messages']:
        if item['service'] == service:
          with log.Event.from_bytes(Path(item['raw']).read_bytes()) as message:
            stamps.append(message.logMonoTime)
      gaps = [(right - left) / 1e9 for left, right in zip(stamps, stamps[1:], strict=False)]
      assert all(0.025 < gap < 0.1 for gap in gaps), gaps
      clocks.append({'owner': row['argv'], 'service': service, 'gaps': gaps})
  result = {
    'source': source,
    'native': native,
    'clocks': clocks,
    'native_sha256': hashlib.sha256(args.native.read_bytes()).hexdigest(),
    'shim_sha256': hashlib.sha256(args.shim.read_bytes()).hexdigest(),
    'source_result_reused': str(args.source_result) if args.source_result else None,
    'scope': (
      'actual unmodified Linux entrypoints, exact owned file aliases, three original IPC/Cereal recipients; '
      + 'comparison IDs1..EOF, raw initial observations retained; no physical V4L2 camera'
    ),
  }
  (output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
  print(
    json.dumps(
      {
        'frames_each': len(native['peer']['frames']),
        'messages_each': len(native['peer']['messages']),
        'source_exit': source['returncode'],
        'native_exit': native['returncode'],
        'clocks': clocks,
      },
      indent=2,
    )
  )


if __name__ == '__main__':
  main()
