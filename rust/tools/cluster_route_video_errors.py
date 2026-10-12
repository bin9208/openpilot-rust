# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# Run: source interpreter rust/tools/cluster_route_video_errors.py ROOT OUTPUT FROZEN_NATIVE.
from __future__ import annotations
import json
from pathlib import Path
import subprocess
import sys


def main() -> None:
  root, output, native = (Path(value).resolve() for value in sys.argv[1:4])
  output.mkdir(parents=True, exist_ok=True)
  sys.path[:0] = [str(root), str(root / 'openpilot/selfdrive/carrot/cluster')]
  import cluster_route_replay as source

  base = json.loads((output.parent / 'comparison/request.json').read_text())
  selected = output / 'not-executable'
  selected.write_text('owned unavailable executable artifact')
  selected.chmod(0o600)
  malformed = output / 'bad-format'
  malformed.write_bytes(b'invalid owned executable format')
  malformed.chmod(0o700)
  records = []
  for name, provider in [('missing', output / "absent 'provider'"), ('permission', selected), ('format', malformed), ('directory', output)]:
    reader = source.RouteVideoFrameReader([source.RouteVideoSegment(**dict(s, path=Path(s['path']))) for s in base['segments']], 'road')
    reader._ffmpeg = str(provider)
    try:
      frame = reader.frame_at(0.0)
      expected = {'status': reader.status_text(), 'process': reader._process is not None, 'frame': None}
      assert frame is None
    finally:
      reader.close()
    query = dict(base, config=dict(base['config'], ffmpeg=str(provider)), steps=[{'time': 0.0}], output=str(output / name))
    run = subprocess.run([str(native)], input=json.dumps(query), capture_output=True, text=True, timeout=3)
    run.check_returncode()
    actual = json.loads(run.stdout)['steps'][0]
    assert actual == expected, (name, actual, expected)
    records.append({'scenario': name, 'source': expected, 'native': actual, 'argv': [str(native)], 'exit_code': run.returncode})
  (output / 'report.json').write_text(json.dumps({'all_equal': True, 'scenarios': records}, indent=2))
  print(json.dumps({'all_equal': True, 'scenarios': len(records)}))


if __name__ == '__main__':
  main()
