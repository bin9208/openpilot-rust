"""Drive the normal positional updater binary with owned commands and loopback probes."""
import argparse
import ctypes
import ctypes.util
import hashlib
import http.server
import json
import os
from pathlib import Path
import signal
import subprocess
import threading
import time
from PIL import ImageGrab, Image
import numpy as np

parser = argparse.ArgumentParser()
parser.add_argument('--binary', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
parser.add_argument('--display', required=True)
parser.add_argument('--expected-mici-waiting', type=Path, required=True)
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
root = Path(__file__).resolve().parents[2]
x = ctypes.CDLL(ctypes.util.find_library('X11'))
t = ctypes.CDLL(ctypes.util.find_library('Xtst'))
x.XOpenDisplay.argtypes = [ctypes.c_char_p]
x.XOpenDisplay.restype = ctypes.c_void_p
display = x.XOpenDisplay(args.display.encode())
assert display
x.XDefaultRootWindow.argtypes = [ctypes.c_void_p]
x.XDefaultRootWindow.restype = ctypes.c_ulong
root_window = x.XDefaultRootWindow(display)
x.XQueryTree.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.POINTER(ctypes.c_ulong), ctypes.POINTER(ctypes.c_ulong), ctypes.POINTER(ctypes.POINTER(ctypes.c_ulong)), ctypes.POINTER(ctypes.c_uint)]
x.XFetchName.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.POINTER(ctypes.c_char_p)]
x.XFree.argtypes = [ctypes.c_void_p]
x.XSync.argtypes = [ctypes.c_void_p, ctypes.c_int]
x.XSetInputFocus.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_int, ctypes.c_ulong]
x.XTranslateCoordinates.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_int, ctypes.c_int, ctypes.POINTER(ctypes.c_int), ctypes.POINTER(ctypes.c_int), ctypes.POINTER(ctypes.c_ulong)]
x.XGetGeometry.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.POINTER(ctypes.c_ulong), ctypes.POINTER(ctypes.c_int), ctypes.POINTER(ctypes.c_int), ctypes.POINTER(ctypes.c_uint), ctypes.POINTER(ctypes.c_uint), ctypes.POINTER(ctypes.c_uint), ctypes.POINTER(ctypes.c_uint)]
t.XTestFakeMotionEvent.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_ulong]
t.XTestFakeButtonEvent.argtypes = [ctypes.c_void_p, ctypes.c_uint, ctypes.c_int, ctypes.c_ulong]

def windows():
  r, parent, children, count = ctypes.c_ulong(), ctypes.c_ulong(), ctypes.POINTER(ctypes.c_ulong)(), ctypes.c_uint()
  assert x.XQueryTree(display, root_window, ctypes.byref(r), ctypes.byref(parent), ctypes.byref(children), ctypes.byref(count))
  result = []
  for index in range(count.value):
    name = ctypes.c_char_p()
    if x.XFetchName(display, children[index], ctypes.byref(name)) and name.value == b'System Update': result.append(children[index])
    if name: x.XFree(name)
  if children: x.XFree(children)
  return result

def geometry(window):
  r, px, py, width, height, border, depth = ctypes.c_ulong(), ctypes.c_int(), ctypes.c_int(), ctypes.c_uint(), ctypes.c_uint(), ctypes.c_uint(), ctypes.c_uint()
  assert x.XGetGeometry(display, window, ctypes.byref(r), ctypes.byref(px), ctypes.byref(py), ctypes.byref(width), ctypes.byref(height), ctypes.byref(border), ctypes.byref(depth))
  child = ctypes.c_ulong()
  assert x.XTranslateCoordinates(display, window, root_window, 0, 0, ctypes.byref(px), ctypes.byref(py), ctypes.byref(child))
  return px.value, py.value, width.value, height.value

def point(window, scale, px, py):
  ox, oy, _, _ = geometry(window)
  t.XTestFakeMotionEvent(display, 0, ox + round(px * scale), oy + round(py * scale), 0)
  x.XSync(display, 0)

def click(window, scale, px, py):
  x.XSetInputFocus(display, window, 1, 0)
  point(window, scale, px, py)
  t.XTestFakeButtonEvent(display, 1, 1, 0)
  x.XSync(display, 0)
  time.sleep(0.12)
  t.XTestFakeButtonEvent(display, 1, 0, 0)
  x.XSync(display, 0)
  time.sleep(0.3)

def drag(window, scale):
  point(window, scale, 480, 125)
  t.XTestFakeButtonEvent(display, 1, 1, 0)
  x.XSync(display, 0)
  for index in range(10):
    point(window, scale, 480 - index * 40, 125)
    time.sleep(0.05)
  t.XTestFakeButtonEvent(display, 1, 0, 0)
  x.XSync(display, 0)
  time.sleep(0.4)

def capture(window, path):
  ox, oy, width, height = geometry(window)
  ImageGrab.grab(xdisplay=args.display).crop((ox, oy, ox + width, oy + height)).save(path)

def wait(predicate, timeout=8):
  deadline = time.monotonic() + timeout
  while not predicate():
    assert time.monotonic() < deadline, 'owned observable deadline exceeded'
    time.sleep(0.02)

class Server(http.server.BaseHTTPRequestHandler):
  observed = []
  def do_HEAD(self):
    self.observed.append({'method': 'HEAD', 'path': self.path})
    self.send_response(503)
    self.end_headers()
  def log_message(self, *a): pass
server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Server)
thread = threading.Thread(target=server.serve_forever)
thread.start()
results = []
try:
  for layout in ['mici', 'tici']:
    for mode in ['waiting-success', 'fatal-retry']:
      directory = (args.output / f'{layout}-{mode}').resolve()
      directory.mkdir(exist_ok=True)
      for artifact in ['argv', 'leaders', 'release']: (directory / artifact).unlink(missing_ok=True)
      manifest = directory / 'manifest.json'
      manifest.write_text(json.dumps([{'url': f'http://127.0.0.1:{server.server_port}/owned-asset'}]))
      updater = directory / 'updater'
      updater.write_text('''#!/bin/sh
printf '%s\\n' "$@" >> argv
printf '%s\\n' "$$" >> leaders
count=$(wc -l < leaders)
printf 'waiting for internet: 0\\n'
if [ "''' + mode + '''" = fatal-retry ] && [ "$count" -eq 1 ]; then
  printf 'verifying_system: 73\\nfatal image hash mismatch\\n'
  exit 9
fi
while [ ! -e release ]; do sleep 0.05; done
printf 'verified inactive slot: 100\\n'
exit 0
''')
      updater.chmod(0o755)
      env = dict(os.environ, DISPLAY=args.display, SCALE='1' if layout == 'mici' else '0.25', BIG='0' if layout == 'mici' else '1', PARAMS_ROOT=str(directory / 'params'), OPENPILOT_PREFIX='agnos-owned-' + layout + '-' + mode, OPENPILOT_ROOT=str(root), DBUS_SYSTEM_BUS_ADDRESS='unix:path=' + '/tmp/agnos-updater-owned-no-bus-' + str(os.getpid()))
      env.pop('OFFSCREEN', None)
      unrelated = subprocess.Popen(['sleep', '60'])
      before = set(windows())
      with (directory / 'native.log').open('w') as log:
        process = subprocess.Popen([str(args.binary.resolve()), str(updater), str(manifest)], cwd=directory, env=env, stdout=log, stderr=subprocess.STDOUT)
        try:
          wait(lambda: (directory / 'leaders').exists())
          wait(lambda: bool(set(windows()) - before))
          window = next(iter(set(windows()) - before))
          scale = 1 if layout == 'mici' else 0.25
          time.sleep(1)
          capture(window, directory / 'initial.png')
          if mode == 'fatal-retry':
            click(window, scale, 200, 120 if layout == 'mici' else 950)
            wait(lambda: len((directory / 'leaders').read_text().splitlines()) == 2)
            time.sleep(0.8)
            capture(window, directory / 'retried.png')
          expected = 2 if mode == 'fatal-retry' else 1
          click(window, scale, 450, 190 if layout == 'mici' else 950)
          time.sleep(0.8)
          capture(window, directory / 'wifi-open.png')
          assert len((directory / 'leaders').read_text().splitlines()) == expected
          if layout == 'mici':
            drag(window, scale)
            drag(window, scale)
            capture(window, directory / 'offline-return-button.png')
            click(window, scale, 200, 120)
          else:
            click(window, scale, 180, 950)
          time.sleep(2.0)
          capture(window, directory / 'returned.png')
          if layout == 'mici':
            assert np.array_equal(np.asarray(Image.open(directory / 'returned.png').convert('RGB')), np.asarray(Image.open(args.expected_mici_waiting).convert('RGB'))), 'visible return to progress differs from source waiting screen'
          assert len((directory / 'leaders').read_text().splitlines()) == expected
          (directory / 'release').touch()
          wait(lambda: 'REBOOT!' in (directory / 'native.log').read_text())
          time.sleep(0.3)
          capture(window, directory / 'success.png')
          process.send_signal(signal.SIGINT)
          process.wait(timeout=8)
          assert process.returncode == 0
          assert unrelated.poll() is None
          leaders = [int(p) for p in (directory / 'leaders').read_text().splitlines()]
          assert all(not Path(f'/proc/{pid}').exists() for pid in leaders)
          argv = (directory / 'argv').read_text().splitlines()
          assert argv == ['--swap', '--retry-network', str(manifest)] * expected
          assert (directory / 'native.log').read_text().count('REBOOT!') == 1
          result = {'layout': layout, 'mode': mode, 'starts': expected, 'argv': argv, 'leaders': leaders, 'leaders_absent': True, 'reboot_prints': 1, 'exit': process.returncode, 'unrelated_alive': True, 'screens': sorted(p.name for p in directory.glob('*.png'))}
          results.append(result)
          (args.output / 'results.json').write_text(json.dumps({'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest(), 'cases': results, 'heads': Server.observed}, indent=2))
          print(layout, mode, 'PASS', flush=True)
        finally:
          if process.poll() is None:
            process.send_signal(signal.SIGINT)
            try: process.wait(timeout=8)
            except subprocess.TimeoutExpired:
              process.kill()
              process.wait()
          unrelated.terminate()
          unrelated.wait()
finally:
  server.shutdown()
  thread.join()
  server.server_close()
