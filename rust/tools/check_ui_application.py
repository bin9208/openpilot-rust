"""Real X11 keyboard/click, profiling and ffmpeg lifecycle gate for native application."""

import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import time
from Xlib import X, XK, display
from Xlib.ext import xtest
import zmq

parser = argparse.ArgumentParser()
parser.add_argument('--binary', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
parser.add_argument('--display', required=True)
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
root = Path(__file__).resolve().parents[2]
prefix = f'ui-framework-125-{os.getpid()}'
transport = zmq.Context()
receiver = transport.socket(zmq.PULL)
receiver.bind(f'ipc:///tmp/logmessage{prefix}')
env = dict(os.environ, DISPLAY=args.display, PARAMS_ROOT=str(args.output / 'params'), OPENPILOT_PREFIX=prefix)
records = []


def drain_logs():
  while receiver.poll(100):
    packet = receiver.recv()
    record = json.loads(packet[1:])
    assert packet[0] == record['levelnum']
    records.append(record)


results = []


def run(name, mode, extra):
  output = args.output / f'{name}.png'
  command = [str(args.binary), str(root), mode, str(output)]
  with (args.output / f'{name}.log').open('w') as log:
    subprocess.run(command, env=env | extra, stdout=log, stderr=subprocess.STDOUT, check=True, timeout=45)
  drain_logs()
  result = json.loads(output.with_suffix('.json').read_text())
  results.append({'scenario': name, **result})
  source = args.output / f'source-{name}.png'
  source_extra = dict(extra)
  if 'RECORD_OUTPUT' in source_extra:
    source_extra['RECORD_OUTPUT'] = str(args.output / 'source-record.mp4')
  with (args.output / f'source-{name}.log').open('w') as log:
    subprocess.run(
      [sys.executable, str(root / 'rust/tools/ui_qa/application_source.py'), mode, str(source)],
      env=env | source_extra | {'PYTHONPATH': str(root)},
      stdout=log,
      stderr=subprocess.STDOUT,
      check=True,
      timeout=45,
    )
  expected = json.loads(source.with_suffix('.json').read_text())
  assert all(result[key] == value for key, value in expected.items()), (name, expected, result)
  if result['frames'] > 0:
    from PIL import Image, ImageChops

    assert ImageChops.difference(Image.open(output).convert('RGB'), Image.open(source).convert('RGB')).getbbox() is None, name
  return result


assert run('profile', 'profile', {'PROFILE_RENDER': '6', 'OFFSCREEN': '1'})['frames'] == 6
startup = run('startup-profile', 'profile', {'PROFILE_STARTUP': '1', 'OFFSCREEN': '1'})
assert startup['frames'] == 0
paused = run('paused', 'paused', {'OFFSCREEN': '1'})
assert paused['frames'] == 4 and paused['skipped'] == 2
record = args.output / 'record.mp4'
recorded = run('record', 'record', {'RECORD': '1', 'RECORD_OUTPUT': str(record), 'PROFILE_RENDER': '9', 'OFFSCREEN': '1'})
assert recorded['frames'] == 9
probe = subprocess.run(
  ['ffprobe', '-v', 'error', '-select_streams', 'v:0', '-show_entries', 'stream=width,height,nb_frames,r_frame_rate', '-of', 'json', str(record)],
  capture_output=True,
  text=True,
  check=True,
)
(args.output / 'record-ffprobe.json').write_text(probe.stdout)
stream = json.loads(probe.stdout)['streams'][0]
assert stream['nb_frames'] == '3' and (stream['width'], stream['height']) == (536, 240)
run('dynamic', 'dynamic', {'PROFILE_RENDER': '9', 'OFFSCREEN': '1'})
videos = list((args.output / 'dynamic.videos').glob('*.mp4'))
assert len(videos) == 1
probe = subprocess.run(
  ['ffprobe', '-v', 'error', '-select_streams', 'v:0', '-show_entries', 'stream=nb_frames', '-of', 'json', str(videos[0])],
  capture_output=True,
  text=True,
  check=True,
)
(args.output / 'dynamic-ffprobe.json').write_text(probe.stdout)
assert json.loads(probe.stdout)['streams'][0]['nb_frames'] == '3'
connection = display.Display(args.display)
protocol_errors = []
connection.set_error_handler(lambda error, request: protocol_errors.append(str(error)))
output = args.output / 'live.png'
with (args.output / 'live.log').open('w') as log:
  process = subprocess.Popen([str(args.binary), str(root), 'live', str(output)], env=env, stdout=log, stderr=subprocess.STDOUT)
  try:
    deadline = time.monotonic() + 20
    window = None
    while time.monotonic() < deadline:
      if process.poll() is not None:
        raise RuntimeError('native application exited before interaction')
      for child in connection.screen().root.query_tree().children:
        if child.get_wm_name() == 'Rust framework lifecycle':
          window = child
          break
      if window is not None:
        break
      time.sleep(0.05)
    assert window is not None
    executable = Path(f'/proc/{process.pid}/exe').resolve()
    mappings = Path(f'/proc/{process.pid}/maps').read_text()
    assert executable == args.binary.resolve() and 'libpython' not in mappings.lower()
    (args.output / 'live-native-process.json').write_text(json.dumps({'pid': process.pid, 'exe': str(executable), 'python_library_mapped': False}, indent=2))
    while not output.exists() and time.monotonic() < deadline:
      time.sleep(0.05)
    assert output.exists(), 'application did not render its first frame'
    window.set_input_focus(X.RevertToParent, X.CurrentTime)
    connection.sync()
    time.sleep(0.15)
    for character in 'native':
      key = connection.keysym_to_keycode(XK.string_to_keysym(character))
      xtest.fake_input(connection, X.KeyPress, key)
      connection.sync()
      time.sleep(0.12)
      xtest.fake_input(connection, X.KeyRelease, key)
      connection.sync()
      time.sleep(0.08)
    attempts = 0
    deadline = time.monotonic() + 15
    while process.poll() is None and time.monotonic() < deadline:
      attempts += 1
      window.set_input_focus(X.RevertToParent, X.CurrentTime)
      position = connection.screen().root.translate_coords(window, 270, 190)
      xtest.fake_input(connection, X.MotionNotify, x=position.x, y=position.y)
      connection.sync()
      time.sleep(0.1)
      xtest.fake_input(connection, X.ButtonPress, 1)
      connection.sync()
      time.sleep(0.25)
      xtest.fake_input(connection, X.ButtonRelease, 1)
      connection.sync()
      try:
        process.wait(timeout=0.5)
      except subprocess.TimeoutExpired:
        continue
    assert process.poll() == 0, 'application did not close after real clicks'
    (args.output / 'live-click-attempts.json').write_text(json.dumps({'attempts': attempts}))
  finally:
    if process.poll() is None:
      process.kill()
      process.wait()
connection.close()
assert not protocol_errors, protocol_errors
live = json.loads(output.with_suffix('.json').read_text())
assert live['text'] == 'native' and live['closed'], live
results.append({'scenario': 'live', **live})
assert all(row['ticks'] == row['frames'] for row in results), results
drain_logs()
assert any(record['msg'].startswith('raylib: ') for record in records), records
assert all(record['pathname'].endswith('logging.rs') for record in records), records
(args.output / 'native-log-transport.json').write_text(json.dumps(records, indent=2))
receiver.close()
transport.term()
(args.output / 'results.json').write_text(json.dumps(results, indent=2))
print(json.dumps(results, indent=2))
print('PASS: actual native XTest keyboard/click, duplicate tick suppression, pause, profile and global/dynamic recording shutdown')
