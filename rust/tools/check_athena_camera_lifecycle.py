#!/usr/bin/env python3
import argparse
import json
from pathlib import Path
import signal
import subprocess
from athena_fixture import private_environment, published, wait_for
from check_athena_snapshot import camera, setup_alert, expected_jpeg
import base64


def children(pid):
  return [int(value) for value in Path(f'/proc/{pid}/task/{pid}/children').read_text().split()]


def camera_children(pid: int, vision: Path) -> list[int]:
  matches = []
  for child in children(pid):
    try:
      executable = Path(f'/proc/{child}/exe').resolve()
    except FileNotFoundError:
      continue
    if executable == vision.resolve():
      matches.append(child)
  return matches


def main():
  parser = argparse.ArgumentParser()
  for name in ['snapshot', 'launcher', 'ipc', 'vision', 'output']:
    parser.add_argument(name, type=Path)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  result = {'pass': False}
  with private_environment(1) as env:
    setup_alert(env)
    (env.params / 'IsOffroad').write_text('1')
    (env.params / 'RecordFront').write_text('1')
    directory = env.root / 'openpilot/system/camerad'
    directory.mkdir(parents=True)
    (directory / 'camerad').symlink_to(args.vision.resolve())
    with published(args.ipc, 'wideRoadCameraState', env.env, camera(79)) as packets:
      with (args.output / 'owned.log').open('w') as output:
        process = subprocess.Popen([args.snapshot, args.launcher], env=dict(env.env, ATHENA_VISION_FIXTURE_AUTO='1'), stdout=output, stderr=subprocess.STDOUT)
        try:
          child = wait_for(lambda: camera_children(process.pid, args.vision), 6)[0]
          assert (env.params / 'IsTakingSnapshot').read_text() == '1'
          packets[0] = camera(80)
          assert process.wait(timeout=7) == 0
          assert not Path(f'/proc/{child}').exists()
          assert (env.params / 'IsTakingSnapshot').read_text() == '0'
          assert not (env.params / 'Offroad_IsTakingSnapshot').exists()
          result['owned_child_reaped'] = child
        finally:
          if process.poll() is None:
            process.send_signal(signal.SIGTERM)
            process.wait(timeout=10)
      lines = (args.output / 'owned.log').read_text().splitlines()
      snapshot = next(json.loads(line) for line in lines if line.startswith('{"jpegBack"'))
      assert all(base64.b64decode(snapshot[key]) == expected_jpeg() for key in ['jpegBack', 'jpegFront'])
      result['owned_snapshot_source_exact'] = True
    (directory / 'camerad').unlink()
    (directory / 'camerad').symlink_to('/usr/bin/sleep')
    already = subprocess.Popen([directory / 'camerad', '30'])
    try:
      captured = subprocess.run([args.snapshot, args.launcher], env=env.env, text=True, capture_output=True, timeout=6)
      (args.output / 'already-running.log').write_text(captured.stdout + captured.stderr)
      assert captured.returncode == 0
      assert json.loads(captured.stdout.splitlines()[-1]) == {'jpegBack': None, 'jpegFront': None}
      assert already.poll() is None
      assert (env.params / 'IsTakingSnapshot').read_text() == '0'
      assert not (env.params / 'Offroad_IsTakingSnapshot').exists()
      result.update(already_running_not_stopped=True, **{'pass': True})
    finally:
      already.terminate()
      already.wait(timeout=5)
      (args.output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
  print('PASS: actual process-supervision camera start,realtime VisionIPC snapshot,owned child reaping,source-exact JPEGs,already-running camera preservation and Params cleanup')


if __name__ == '__main__':
  main()
