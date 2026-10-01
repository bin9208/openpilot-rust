import json
import os
from pathlib import Path
import queue
import shutil
import signal
import subprocess
import tempfile
import threading
import time
import uuid
from contextlib import contextmanager
from cryptography.hazmat.primitives.asymmetric import ec
from cryptography.hazmat.primitives import serialization
from websockets.sync.server import serve
from websockets.exceptions import ConnectionClosed


def wait_for(predicate, timeout=5):
  end = time.monotonic() + timeout
  while time.monotonic() < end:
    value = predicate()
    if value:
      return value
    time.sleep(0.01)
  raise TimeoutError('owned Athena fixture condition')


class Peer:
  def __init__(self, socket):
    self.socket = socket
    self.replies = queue.Queue()
    self.messages = []
    self.counter = 0
    self.headers = dict(socket.request.headers)
    self.path = socket.request.path

  def receive(self):
    try:
      while True:
        chunks = list(self.socket.recv_streaming())
        text = ''.join(chunks)
        packet = json.loads(text)
        self.messages.append({'packet': packet, 'chunks': [len(chunk) for chunk in chunks]})
        if 'method' in packet:
          self.socket.send(json.dumps({'jsonrpc': '2.0', 'id': packet['id'], 'result': {'success': 1}}))
        else:
          self.replies.put(packet)
    except ConnectionClosed:
      return

  def rpc(self, method, params=None, timeout=5):
    self.counter += 1
    packet = {'jsonrpc': '2.0', 'id': self.counter, 'method': method}
    if params is not None:
      packet['params'] = params
    self.socket.send(json.dumps(packet))
    response = self.replies.get(timeout=timeout)
    assert response['id'] == self.counter, response
    return response


@contextmanager
def websocket_server(ping_interval=20, process_request=None):
  connected = queue.Queue()
  peers = []

  def handler(socket):
    peer = Peer(socket)
    peers.append(peer)
    connected.put(peer)
    peer.receive()

  with serve(handler, '127.0.0.1', 0, max_size=None, ping_interval=ping_interval, process_request=process_request) as server:
    thread = threading.Thread(target=server.serve_forever)
    thread.start()
    try:
      yield server.socket.getsockname()[1], connected, peers
    finally:
      for peer in peers:
        peer.socket.close()
      server.shutdown()
      thread.join(timeout=5)


class Environment:
  def __init__(self, root, port):
    self.root = root
    self.prefix = 'athena146_' + uuid.uuid4().hex[:24]
    self.home = Path.home() / ('.comma' + self.prefix)
    self.shm = Path('/dev/shm') / ('msgq_' + self.prefix)
    self.params = root / 'params' / self.prefix
    self.logs = root / 'data'
    for directory in [self.home / 'persist/comma', self.home / 'stats', self.home / 'log', self.shm, self.params, self.logs]:
      directory.mkdir(parents=True)
    self.key = ec.generate_private_key(ec.SECP256R1())
    private = self.key.private_bytes(serialization.Encoding.PEM, serialization.PrivateFormat.TraditionalOpenSSL, serialization.NoEncryption())
    self.public = self.key.public_key().public_bytes(serialization.Encoding.PEM, serialization.PublicFormat.SubjectPublicKeyInfo)
    (self.home / 'persist/comma/id_ecdsa').write_bytes(private)
    (self.home / 'persist/comma/id_ecdsa.pub').write_bytes(self.public)
    (self.params / 'DongleId').write_text('synthetic146')
    (root / 'build.json').write_text(json.dumps({'channel': 'fixture', 'openpilot': {'version': '1.2.3', 'git_origin': 'https://github.com/synthetic/athena.git', 'git_commit': '0' * 40}}))
    self.env = dict(os.environ, PARAMS_ROOT=str(root / 'params'), OPENPILOT_PREFIX=self.prefix, LOG_ROOT=str(self.logs), OPENPILOT_BASEDIR=str(root), ATHENA_HOST=f'ws://127.0.0.1:{port}', LOGPRINT='debug')

  def close(self):
    shutil.rmtree(self.home)
    shutil.rmtree(self.shm)
    Path('/tmp/logmessage' + self.prefix).unlink(missing_ok=True)


@contextmanager
def daemon(binary, root, env, trace=False):
  log = (root / 'daemon.log').open('wb')
  command = [str(binary)]
  if trace:
    command = ['strace', '-f', '-e', 'trace=network,read,write,poll,ppoll', '-o', str(root / 'sockets.log'), *command]
  process = subprocess.Popen(command, env=env, stdout=log, stderr=subprocess.STDOUT)
  pid = process.pid
  if trace:
    pid = wait_for(lambda: Path(f'/proc/{process.pid}/task/{process.pid}/children').read_text().strip())
    pid = int(pid.split()[0])
  try:
    wait_for(lambda: Path(f'/proc/{pid}/exe').resolve() == binary.resolve())
    yield process, pid
  finally:
    if process.poll() is None:
      os.kill(pid, signal.SIGTERM)
    try:
      process.wait(timeout=36)
    except subprocess.TimeoutExpired:
      os.kill(pid, signal.SIGKILL)
      process.kill()
      process.wait(timeout=5)
    log.close()


@contextmanager
def private_environment(port):
  with tempfile.TemporaryDirectory(prefix='athena146-') as temp:
    environment = Environment(Path(temp), port)
    try:
      yield environment
    finally:
      environment.close()


class Publisher:
  def __init__(self, binary, service, env):
    self.process = subprocess.Popen([binary, 'publish', service], env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    assert self.process.stdout.readline().strip() == 'READY'
    self.lock = threading.Lock()

  def send(self, packet):
    with self.lock:
      self.process.stdin.write(json.dumps(list(packet)) + '\n')
      self.process.stdin.flush()
      assert self.process.stdout.readline().strip() == 'OK'

  def close(self):
    self.process.stdin.close()
    assert self.process.wait(timeout=3) == 0


@contextmanager
def published(binary, service, env, initial):
  publisher = Publisher(binary, service, env)
  packet = [initial]
  stop = threading.Event()
  errors = []

  def stream():
    try:
      while not stop.is_set():
        publisher.send(packet[0])
        stop.wait(0.05)
    except (AssertionError, OSError) as error:
      errors.append(str(error))

  thread = threading.Thread(target=stream)
  thread.start()
  try:
    yield packet
  finally:
    stop.set()
    thread.join(timeout=3)
    publisher.close()
    assert not errors, errors
