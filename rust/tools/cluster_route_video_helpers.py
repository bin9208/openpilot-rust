# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# Run: source interpreter rust/tools/cluster_route_video_helpers.py ROOT OUTPUT [NATIVE].
from __future__ import annotations
from dataclasses import asdict
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys


def main() -> None:
  root, output = (Path(value).resolve() for value in sys.argv[1:3])
  output.mkdir(parents=True, exist_ok=True)
  sys.path[:0] = [str(root), str(root / 'openpilot/selfdrive/carrot/cluster')]
  import cluster_route_replay as source

  folder = output / 'filenames'
  folder.mkdir(exist_ok=True)
  segments = [
    source.RouteVideoSegment(None, folder / 'first', -5.0, -2.0),
    source.RouteVideoSegment(3, folder / 'second', 1.0, 5.0),
    source.RouteVideoSegment(5, folder / 'third', 20.0, 25.0),
  ]
  times = [-100.0, -5.0, -2.0, -1.5, -1.499999, 0.0, 1.0, 5.0, 5.5, 5.50001, 19.0, 20.0, 25.5, 100.0]
  times.extend(i / 4 for i in range(-32, 128))
  query = {
    'segments': [asdict(s) for s in segments],
    'times': times,
    'folder': str(folder),
    'streams': ['road', 'wide', 'other'],
    'config': {'cwd': str(root), 'ffmpeg': None},
  }
  scenarios = []
  for index, files in enumerate(
    [[], ['fcamera.ts', 'ecamera.ts'], ['fcamera.ts', 'fcamera.hevc', 'ecamera.ts', 'ecamera.hevc'], ['qcamera.ts', 'fcamera.hevc'], []]
  ):
    for path in folder.iterdir():
      if path.is_file():
        path.unlink()
      elif path.is_dir():
        path.rmdir()
    for name in files:
      (folder / name).write_bytes(b'owned selection input')
    if index == 4:
      (folder / 'qcamera.ts').mkdir()
    expected = {
      'segments': [asdict(source.route_video_segment_at(segments, t)) for t in times],
      'paths': [source.route_video_path_for_stream(folder, stream) for stream in query['streams']],
      'discovered': source.route_video_ffmpeg_path(),
    }
    expected = json.loads(json.dumps(expected, default=str))
    record = {'case': index, 'source': expected}
    if len(sys.argv) > 3:
      native = Path(sys.argv[3]).resolve()
      run = subprocess.run([str(native), 'helpers'], input=json.dumps(query, default=str), capture_output=True, text=True, timeout=5)
      run.check_returncode()
      actual = json.loads(run.stdout)
      assert actual == expected, (index, actual, expected)
      record['native'] = actual
    scenarios.append(record)
  if len(sys.argv) > 3:
    search = output / 'path-bin'
    search.mkdir(exist_ok=True)
    exe = search / 'ffmpeg'
    exe.write_text('#!/bin/sh\nexit 0\n')
    exe.chmod(0o700)
    nonexe = search / 'noexecute'
    nonexe.write_text('owned existing non-executable source override')
    for name, override, path in [
      ('default', '', str(search)),
      ('whitespace', ' \tffmpeg\n ', str(search)),
      ('existing-not-executable', str(nonexe), ''),
      ('invalid-falls-through', str(search / 'absent'), str(search)),
      ('none', '', ''),
    ]:
      env = dict(os.environ, PATH=path, CLUSTER_ROUTE_FFMPEG=override)
      old = dict(os.environ)
      try:
        os.environ.update(env)
        expected = source.route_video_ffmpeg_path()
      finally:
        os.environ.clear()
        os.environ.update(old)
      run = subprocess.run([str(native), 'helpers'], input=json.dumps(query, default=str), env=env, capture_output=True, text=True, timeout=5)
      run.check_returncode()
      actual = json.loads(run.stdout)['discovered']
      assert actual == expected, (name, actual, expected)
      scenarios.append({'name': name, 'discovered': actual})
    packaged = output / 'packaged-ffmpeg'
    query['bundled'] = str(packaged)
    run = subprocess.run(
      [str(native), 'helpers'],
      input=json.dumps(query, default=str),
      env=dict(os.environ, PATH='', CLUSTER_ROUTE_FFMPEG=''),
      capture_output=True,
      text=True,
      timeout=5,
    )
    run.check_returncode()
    assert json.loads(run.stdout)['discovered'] == str(packaged)
    scenarios.append({'name': 'explicit-build-provider', 'discovered': str(packaged)})
  report = {
    'all_equal': True,
    'segment_observations': len(times) * 5,
    'path_observations': 15,
    'scenarios': scenarios,
    'source_sha256': hashlib.sha256(Path(source.__file__).read_bytes()).hexdigest(),
  }
  (output / 'report.json').write_text(json.dumps(report, indent=2))
  print(json.dumps({'all_equal': True, 'segment_observations': len(times) * 5, 'path_observations': 15, 'scenarios': len(scenarios)}))


if __name__ == '__main__':
  main()
