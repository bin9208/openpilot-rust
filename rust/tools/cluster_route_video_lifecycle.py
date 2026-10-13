# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# Run: source interpreter rust/tools/cluster_route_video_lifecycle.py ROOT OUTPUT FROZEN_NATIVE.
from __future__ import annotations
import json
import os
from pathlib import Path
import select
import signal
import subprocess
import sys
import time


def source_driver(root: Path) -> None:
  sys.path[:0] = [str(root), str(root / 'openpilot/selfdrive/carrot/cluster')]
  import cluster_route_replay as source

  query = json.loads(sys.stdin.readline())
  reader = source.RouteVideoFrameReader([source.RouteVideoSegment(**dict(s, path=Path(s['path']))) for s in query['segments']], query['stream'])
  reader._ffmpeg = query['config']['ffmpeg']
  try:
    for line in sys.stdin:
      step = json.loads(line)
      if step.get('close'):
        reader.close()
        frame = None
      else:
        frame = reader.frame_at(step['time'])
      print(
        json.dumps(
          {
            'snapshot': {
              'status': reader.status_text(),
              'process': reader._process is not None,
              'frame': None if frame is None else {'width': frame.width, 'height': frame.height, 'frame_id': frame.frame_id},
            },
            'pid': None if reader._process is None else reader._process.pid,
          }
        ),
        flush=True,
      )
  finally:
    reader.close()


def observe(process: subprocess.Popen[str]) -> dict:
  if not select.select([process.stdout], [], [], 5.0)[0]:
    raise RuntimeError('missing live reader observable')
  return json.loads(process.stdout.readline())


def main() -> None:
  root, output, native = (Path(value).resolve() for value in sys.argv[1:4])
  if len(sys.argv) > 4 and sys.argv[4] == 'source-driver':
    source_driver(root)
    return
  output.mkdir(parents=True, exist_ok=True)
  base = json.loads((output.parent / 'comparison/request.json').read_text())
  records = []
  for name, argv in [('source', [sys.executable, __file__, str(root), str(output), str(native), 'source-driver']), ('native', [str(native), 'live'])]:
    with (output / f'{name}.stderr').open('w') as log:
      process = subprocess.Popen(argv, text=True, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=log, start_new_session=True)
      query = dict(base, steps=[], output=str(output / name))
      child = None
      try:
        process.stdin.write(json.dumps(query) + '\n' + json.dumps({'time': 0.0}) + '\n')
        process.stdin.flush()
        first = observe(process)
        child = first['pid']
        assert child is not None and first['snapshot']['frame'] is not None
        cmdline = Path(f'/proc/{child}/cmdline').read_bytes().split(b'\0')[:-1]
        os.kill(child, signal.SIGSTOP)
        start = time.monotonic()
        process.stdin.write(json.dumps({'close': True}) + '\n')
        process.stdin.flush()
        closed = observe(process)
        elapsed = time.monotonic() - start
        process.stdin.close()
        process.wait(timeout=3)
        assert process.returncode == 0 and closed['pid'] is None and not Path(f'/proc/{child}').exists()
        assert 0.29 <= elapsed < 0.9, elapsed
        records.append(
          {
            'name': name,
            'argv': argv,
            'first': first,
            'closed': closed,
            'elapsed': elapsed,
            'ffmpeg_cmdline': [s.decode() for s in cmdline],
            'child_gone': True,
            'exit_code': process.returncode,
          }
        )
      finally:
        if process.poll() is None:
          os.killpg(process.pid, signal.SIGKILL)
          process.wait(timeout=3)
  assert records[0]['first']['snapshot'] == records[1]['first']['snapshot']
  assert records[0]['closed']['snapshot'] == records[1]['closed']['snapshot']
  assert records[0]['ffmpeg_cmdline'] == records[1]['ffmpeg_cmdline']
  (output / 'report.json').write_text(json.dumps({'all_equal': True, 'stopped_ffmpeg': records}, indent=2))
  print(json.dumps({'all_equal': True, 'close_seconds': [r['elapsed'] for r in records]}))


if __name__ == '__main__':
  main()
