# /// script
# requires-python = ">=3.12"
# dependencies = ["pillow"]
# ///
# Run: source interpreter rust/tools/cluster_jpeg_async_compare.py ROOT OUTPUT NATIVE.
from __future__ import annotations
import hashlib
import json
import os
from pathlib import Path
import select
import signal
import socket
import subprocess
import sys
import time
import tempfile


def source_driver(root: Path) -> None:
  sys.path[:0] = [str(root), str(root / 'openpilot/selfdrive/carrot/cluster')]
  from cluster_usb_display import TuringUsbDisplay
  from cluster_usb_pipeline import AsyncJpegUsbPipeline

  query = json.loads(sys.stdin.readline())
  display = TuringUsbDisplay(jpeg_quality=query['quality'], jpeg_encoder=query['encoder'])
  display.set_profile_enabled(query['profile'])
  recipient = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
  recipient.connect(query['socket'])

  class Output:
    def clear_profile_samples(self):
      display.clear_profile_samples()

    def profile_samples(self):
      return display.profile_samples()

    def encode_jpeg(self, rgba, width, height):
      return display.encode_jpeg(rgba, width, height)

    def send_jpeg(self, jpeg):
      recipient.sendall(len(jpeg).to_bytes(8, 'little') + jpeg)
      if recipient.recv(1) != b'\1':
        raise RuntimeError('owned recipient rejected JPEG')

  pipeline = AsyncJpegUsbPipeline(Output())
  pipeline.start()
  print(json.dumps({'started': True, 'dimensions': None}), flush=True)
  try:
    for line in sys.stdin:
      command = json.loads(line)
      try:
        match command['op']:
          case 'submit':
            value = pipeline.submit_rgba(Path(command['pixels']).read_bytes(), command['width'], command['height'])
          case 'capacity':
            value = pipeline.wait_for_capacity(command['timeout'])
          case 'samples':
            value = pipeline.profile_samples()
          case 'check':
            value = pipeline.check_error()
          case 'close':
            pipeline.close()
            value = not pipeline._thread.is_alive()
          case unknown:
            raise RuntimeError(f'unknown QA command {unknown}')
        response = {'ok': value}
      except (RuntimeError, ValueError) as exc:
        response = {'error': str(exc), 'cause': None if exc.__cause__ is None else str(exc.__cause__)}
      print(json.dumps(response), flush=True)
  finally:
    pipeline.close()
    recipient.close()


def exact(peer: socket.socket, count: int) -> bytes:
  result = bytearray()
  while len(result) < count:
    part = peer.recv(count - len(result))
    if not part:
      raise RuntimeError('owned recipient ended mid-frame')
    result.extend(part)
  return bytes(result)


class Session:
  """Owned live-process observation accumulator; mutable only while the fixture runs."""

  def __init__(self, process, peer, prefix):
    self.process, self.peer, self.prefix = process, peer, prefix
    self.steps, self.frames, self.raw = [], [], []

  def response(self):
    if not select.select([self.process.stdout], [], [], 5)[0]:
      raise RuntimeError('missing native/source pipeline observable')
    return json.loads(self.process.stdout.readline())

  def action(self, command):
    self.process.stdin.write(json.dumps(command) + '\n')
    self.process.stdin.flush()
    value = self.response()
    self.raw.append(json.loads(json.dumps(value)))
    self.steps.append({'command': command, 'response': value})
    return value

  def submit(self, path, width, height):
    return self.action({'op': 'submit', 'pixels': str(path), 'width': width, 'height': height})

  def receive(self):
    length = int.from_bytes(exact(self.peer, 8), 'little')
    data = exact(self.peer, length)
    self.frames.append({'bytes': length, 'sha256': hashlib.sha256(data).hexdigest()})
    Path(str(self.prefix) + f'-{len(self.frames)}.jpg').write_bytes(data)


def main() -> None:
  root, output, native = (Path(v).resolve() for v in sys.argv[1:4])
  if len(sys.argv) > 4 and sys.argv[4] == 'source-driver':
    source_driver(root)
    return
  output.mkdir(parents=True, exist_ok=True)
  width, height = 63, 35
  files = []
  for seed in [1, 2, 3]:
    path = output / f'{seed}.rgba'
    path.write_bytes(bytes((i * 29 + seed * 71) % 256 for i in range(width * height * 4)))
    files.append(path)
  invalid = output / 'invalid.rgba'
  invalid.write_bytes(b'not one complete RGBA image')
  records = []
  sockets = tempfile.TemporaryDirectory(prefix='cluster-jpeg-')
  for scenario in ['latest', 'send-error', 'blocked-close', 'encode-error']:
    observations = []
    for kind in ['source', 'native']:
      path = Path(sockets.name) / f'{scenario}-{kind}.sock'
      listener = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
      listener.bind(str(path))
      listener.listen(1)
      listener.settimeout(5)
      argv = [sys.executable, __file__, str(root), str(output), str(native), 'source-driver'] if kind == 'source' else [str(native)]
      with (output / f'{scenario}-{kind}.stderr').open('w') as log:
        process = subprocess.Popen(argv, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=log, text=True, start_new_session=True)
        query = {
          'mode': 'pipeline',
          'encoder': 'pillow',
          'quality': 82,
          'turbo_library': str(output / 'missing-turbo.so'),
          'profile': True,
          'socket': str(path),
        }
        process.stdin.write(json.dumps(query) + '\n')
        process.stdin.flush()
        peer, _ = listener.accept()
        peer.settimeout(5)
        steps = []
        frames = []

        session = Session(process, peer, output / f'{scenario}-{kind}')
        response, action, submit, receive = session.response, session.action, session.submit, session.receive
        steps, frames = session.steps, session.frames
        elapsed = None
        try:
          assert response() == {'started': True, 'dimensions': None}
          if scenario == 'encode-error':
            action({'op': 'submit', 'pixels': str(invalid), 'width': width, 'height': height})
            time.sleep(0.05)
            failed = action({'op': 'check'})
            assert failed['error'] == 'asynchronous USB JPEG pipeline failed' and failed['cause'] is not None
            # Native boundary errors describe their typed pixel contract.
            steps[-1]['response']['cause'] = 'typed encode failure'
          else:
            submit(files[0], width, height)
            receive()
            if scenario == 'latest':
              submit(files[1], width, height)
              submit(files[2], width, height)
              assert action({'op': 'capacity', 'timeout': 0}) == {'ok': False}
              peer.sendall(b'\1')
              receive()
              assert action({'op': 'capacity', 'timeout': 0.5}) == {'ok': True}
              peer.sendall(b'\1')
            elif scenario == 'send-error':
              submit(files[1], width, height)
              peer.sendall(b'\0')
              failed = action({'op': 'capacity', 'timeout': 1})
              assert failed == {'error': 'asynchronous USB JPEG pipeline failed', 'cause': 'owned recipient rejected JPEG'}
              assert submit(files[2], width, height) == failed
            else:
              start = time.monotonic()
              pending = action({'op': 'close'})
              elapsed = time.monotonic() - start
              assert pending == {'ok': False} and 2.9 < elapsed < 3.6, (pending, elapsed)
              peer.sendall(b'\1')
          assert action({'op': 'close'}) == {'ok': True}
          samples = action({'op': 'samples'})['ok']
          assert all(milliseconds >= 0 for _, milliseconds in samples)
          stages = [name for name, _ in samples]
          steps[-1]['response']['ok'] = stages
          assert action({'op': 'samples'}) == {'ok': []}
          process.stdin.close()
          process.wait(timeout=4)
          assert process.returncode == 0
          observations.append(
            {'kind': kind, 'frames': frames, 'steps': steps, 'raw_responses': session.raw, 'blocked_close_seconds': elapsed, 'exit_code': process.returncode}
          )
        finally:
          peer.close()
          listener.close()
          path.unlink()
          if process.poll() is None:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait(timeout=4)
    assert observations[0]['frames'] == observations[1]['frames'], scenario
    assert observations[0]['steps'] == observations[1]['steps'], (scenario, observations)
    records.append({'scenario': scenario, 'observations': observations})
  sockets.cleanup()
  (output / 'report.json').write_text(json.dumps({'all_equal': True, 'scenarios': records}, indent=2))
  print(json.dumps({'all_equal': True, 'scenarios': len(records)}))


if __name__ == '__main__':
  main()
