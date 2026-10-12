# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# Run: source interpreter rust/tools/cluster_route_video_compare.py ROOT OUTPUT [FROZEN_NATIVE].
from __future__ import annotations
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time


def main() -> None:
  root, output = (Path(value).resolve() for value in sys.argv[1:3])
  output.mkdir(parents=True, exist_ok=True)
  sys.path[:0] = [str(root), str(root / 'openpilot/selfdrive/carrot/cluster')]
  import cluster_route_replay as source

  ffmpeg = Path('/usr/bin/ffmpeg')
  commands = []
  for name, duration, pattern in [("road '한글'.mkv", 10, 'testsrc2'), ('wide.mkv', 3, 'smptebars')]:
    path = output / name
    if not path.exists():
      argv = [
        str(ffmpeg),
        '-hide_banner',
        '-loglevel',
        'error',
        '-threads',
        '1',
        '-f',
        'lavfi',
        '-i',
        f'{pattern}=size=64x48:rate=10',
        '-t',
        str(duration),
        '-c:v',
        'ffv1',
        str(path),
      ]
      run = subprocess.run(argv, capture_output=True, timeout=15)
      commands.append({'argv': argv, 'code': run.returncode, 'stderr': run.stderr.decode()})
      run.check_returncode()
  bad = output / 'invalid.hevc'
  bad.write_bytes(b'owned invalid video input')
  road = {'index': None, 'path': str(output / "road '한글'.mkv"), 'start_t': 0.0, 'end_t': 10.0}
  wide = {'index': 7, 'path': str(output / 'wide.mkv'), 'start_t': 15.0, 'end_t': 18.0}
  steps = [{'time': t} for t in [-1.0, 0.0, 0.05, 0.0999, 0.1, 0.2, 4.7, 4.8, 9.4, 1.0, 1.0, 10.0, 10.5, 10.50001, 15.0, 15.1, 18.0, 50.0, 15.0, 0.0]]
  steps.extend(
    [
      {'segments': [dict(road, path=str(bad))], 'time': 0.0},
      {'segments': [dict(road, path=str(output / 'absent.hevc'))], 'time': 0.0},
      {'segments': [], 'time': 0.0},
      {'segments': [road], 'time': 0.0},
      {'close': True},
      {'close': True},
      {'time': 0.2},
    ]
  )
  query = {'segments': [road, wide], 'stream': 'road', 'config': {'ffmpeg': str(ffmpeg), 'cwd': str(root)}, 'steps': steps, 'output': str(output / 'native')}
  (output / 'request.json').write_text(json.dumps(query))
  (output / 'source').mkdir(exist_ok=True)
  original = source.subprocess.Popen
  launched = []

  def capture(*args, **kwargs):
    child = original(*args, **kwargs)
    launched.append({'argv': args[0], 'pid': child.pid})
    return child

  source.subprocess.Popen = capture
  os.environ['CLUSTER_ROUTE_FFMPEG'] = str(ffmpeg)
  reader = source.RouteVideoFrameReader([source.RouteVideoSegment(**dict(s, path=Path(s['path']))) for s in query['segments']], query['stream'])
  expected = []
  try:
    for index, step in enumerate(steps):
      if 'segments' in step:
        reader.segments = [source.RouteVideoSegment(**dict(s, path=Path(s['path']))) for s in step['segments']]
      start = time.monotonic()
      frame = None
      if step.get('close'):
        reader.close()
      else:
        frame = reader.frame_at(step['time'])
      item = {'status': reader.status_text(), 'process': reader._process is not None, 'frame': None}
      if frame is not None:
        data = frame.rgba
        (output / 'source' / f'{index}.rgba').write_bytes(data)
        item['frame'] = {'width': frame.width, 'height': frame.height, 'frame_id': frame.frame_id}
      expected.append(item)
      commands.append({'source_step': index, 'elapsed': time.monotonic() - start})
  finally:
    reader.close()
    source.subprocess.Popen = original
  (output / 'source.json').write_text(json.dumps(expected, indent=2))
  (output / 'source-processes.json').write_text(json.dumps(launched, indent=2))
  gone = all(not Path(f'/proc/{entry["pid"]}').exists() for entry in launched)
  (output / 'commands.json').write_text(json.dumps(commands, indent=2))
  assert gone, launched
  if len(sys.argv) > 3:
    native = Path(sys.argv[3]).resolve()
    run = subprocess.run([str(native)], input=json.dumps(query), text=True, capture_output=True, timeout=30)
    (output / 'native.json').write_text(run.stdout)
    (output / 'native.stderr').write_text(run.stderr)
    run.check_returncode()
    actual = json.loads(run.stdout)
    assert actual['steps'] == expected, [(i, a, e) for i, (a, e) in enumerate(zip(actual['steps'], expected, strict=True)) if a != e]
    frames = []
    for index, item in enumerate(expected):
      if item['frame'] is None:
        continue
      original_bytes = (output / 'source' / f'{index}.rgba').read_bytes()
      native_bytes = (output / 'native' / f'{index}.rgba').read_bytes()
      assert native_bytes == original_bytes, index
      frames.append({'step': index, 'bytes': len(native_bytes), 'sha256': hashlib.sha256(native_bytes).hexdigest()})
    report = {
      'all_equal': True,
      'steps': len(expected),
      'frames': frames,
      'source_processes_reaped': gone,
      'native_processes_reaped': all(not Path(f'/proc/{pid}').exists() for pid in actual['pids']),
      'native_sha256': hashlib.sha256(native.read_bytes()).hexdigest(),
    }
    assert report['native_processes_reaped']
    (output / 'report.json').write_text(json.dumps(report, indent=2))
    print(json.dumps(report))
  else:
    print(json.dumps({'source_steps': len(expected), 'processes': len(launched), 'all_reaped': gone}))


if __name__ == '__main__':
  main()
