import ctypes
import json
import os
from pathlib import Path
import selectors
import signal
import subprocess
import sys
import tempfile
import time
import uuid

import zmq

from openpilot.cereal import log


def enable_subreaper():
  libc = ctypes.CDLL(None, use_errno=True)
  libc.prctl.argtypes = [ctypes.c_int, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_ulong]
  libc.prctl.restype = ctypes.c_int
  if libc.prctl(36, 1, 0, 0, 0) != 0:
    raise OSError(ctypes.get_errno(), 'test subreaper setup failed')


def wait_until(predicate, timeout=3):
  deadline = time.monotonic() + timeout
  while True:
    value = predicate()
    if value:
      return value
    assert time.monotonic() < deadline, 'fixture observation timed out'
    time.sleep(0.001)


class Peer:
  def __init__(self, args, implementation, name):
    self.args, self.implementation = args, implementation
    self.output = args.output / name / implementation
    self.output.mkdir(parents=True)
    self.temporary = tempfile.TemporaryDirectory(prefix='sv-')
    self.root = Path(self.temporary.name)
    self.prefix = 'sv_' + uuid.uuid4().hex
    self.context = zmq.Context()
    self.socket = self.context.socket(zmq.PULL)
    self.endpoint = 'ipc://' + str(self.root / 'log')
    self.socket.bind(self.endpoint)
    self.process = None
    self.records = []
    self.transcript = []
    self.commands = []
    self.owned = {}
    self.stderr = (self.output / 'stderr.log').open('wb')

  def native(self, name='child', mode='normal', **policy):
    work = self.root / name
    work.mkdir()
    return {'kind': 'native', 'name': name, 'cwd': name,
            'argv': [str(self.args.fixture.resolve()), str(work), mode, 'owned-' + self.prefix], **policy}

  def persistent(self, name='persistent', **policy):
    work = self.root / name
    work.mkdir()
    identity = 'persistent-' + name + '-' + self.prefix
    return {'kind': 'persistent', 'name': name, 'identity': identity, 'param': 'AthenadPid',
            'argv': [str(self.args.fixture.resolve()), str(work), 'normal', identity], **policy}

  def launch(self, specs, environment=None, inherited=()):
    config = {'basedir': str(self.root), 'launcher': str(self.args.launcher.resolve()), 'params_root': str(self.root / 'params'),
              'prefix': self.prefix, 'endpoint': self.endpoint, 'binding': str(self.args.binding.resolve()), 'processes': specs}
    command = [sys.executable, str(Path(__file__).with_name('supervision_source.py'))] if self.implementation == 'python' else [str(self.args.binary.resolve())]
    env = dict(os.environ, OPENPILOT_PREFIX=self.prefix, PARAMS_ROOT=config['params_root'], PROCESS_FIXTURE_INHERITED='inherited-value',
               MANAGER_DAEMON='parent-daemon', LOGPRINT='warning')
    env.update(environment or {})
    self.process = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=self.stderr, text=True,
                                    env=env, start_new_session=True, pass_fds=inherited)
    self.commands.append({'command': command, 'config': config, 'pid': self.process.pid})
    (self.output / 'command.json').write_text(json.dumps(self.commands, indent=2) + '\n')
    self.selector = selectors.DefaultSelector()
    self.selector.register(self.process.stdout, selectors.EVENT_READ)
    self.process.stdin.write(json.dumps(config) + '\n')
    self.process.stdin.flush()
    assert self.read() == {'ready': True}

  def read(self):
    assert self.selector.select(10), f'{self.implementation} RPC timeout'
    line = self.process.stdout.readline()
    assert line, f'{self.implementation} exited {self.process.poll()}: {(self.output / "stderr.log").read_text()}'
    return json.loads(line)

  def drain(self, timeout=0):
    while self.socket.poll(timeout):
      packet = self.socket.recv()
      record = json.loads(packet[1:])
      assert packet[0] == record['levelnum']
      self.records.append(record)
      timeout = 0
    (self.output / 'logs.json').write_text(json.dumps(self.records, indent=2) + '\n')

  def op(self, op, **values):
    request = {'op': op, **values}
    self.process.stdin.write(json.dumps(request) + '\n')
    self.process.stdin.flush()
    response = self.read()
    for snapshot in response['snapshots']:
      wire = bytes(snapshot['wire'])
      with log.ManagerState.ProcessState.from_bytes(wire) as state:
        assert state.to_dict() == snapshot['state']
    self.transcript.append({'request': request, 'response': response})
    (self.output / 'transcript.json').write_text(json.dumps(self.transcript, indent=2) + '\n')
    self.drain()
    return response

  def state(self, name='child'):
    response = self.op('state', name=name)
    assert response['error'] is None, response
    return response['result']

  def ready(self, name='child'):
    path = self.root / name / 'ready.json'
    wait_until(path.exists)
    ready = json.loads(path.read_text())
    self.owned[ready['pid']] = ready
    assert ready['pid'] > 1 and str(self.root) in ready['argv'][1]
    (self.output / (name + '-ready.json')).write_text(json.dumps(ready, indent=2) + '\n')
    return ready

  def signals(self, name='child'):
    path = self.root / name / 'signals.jsonl'
    return [] if not path.exists() else [json.loads(line) for line in path.read_text().splitlines()]

  def put_pid(self, raw):
    path = self.root / 'params' / self.prefix / 'AthenadPid'
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(raw)

  def pid(self):
    return int((self.root / 'params' / self.prefix / 'AthenadPid').read_bytes())

  def close_supervisor(self):
    if self.process is not None and self.process.poll() is None:
      self.op('exit')
      self.process.stdin.close()
      assert self.process.wait(timeout=3) == 0
      self.process.stdout.close()
      self.selector.close()
    self.drain(100)

  def close(self):
    cleanup = []
    try:
      self.close_supervisor()
    finally:
      if self.process is not None and self.process.poll() is None:
        self.process.kill()
        self.process.wait(timeout=3)
      for ready_path in self.root.glob('*/ready.json'):
        ready = json.loads(ready_path.read_text())
        self.owned[ready['pid']] = ready
      for ready in self.owned.values():
        pid = ready['pid']
        cmdline = Path(f'/proc/{pid}/cmdline')
        try:
          raw = cmdline.read_bytes()
        except FileNotFoundError:
          raw = b''
        if raw:
          assert str(self.root).encode() in raw, (pid, 'PID identity changed')
          os.kill(pid, signal.SIGKILL)
        try:
          os.waitpid(pid, 0)
        except ChildProcessError:
          assert not Path(f'/proc/{pid}').exists(), pid
        cleanup.append({'pid': pid, 'exists_after': Path(f'/proc/{pid}').exists()})
      assert all(not row['exists_after'] for row in cleanup)
      children = Path(f'/proc/self/task/{os.getpid()}/children').read_text().strip()
      (self.output / 'cleanup.json').write_text(json.dumps({'owned_processes': cleanup, 'remaining_children': children.split()}, indent=2) + '\n')
      assert not children, f'unreaped task descendants: {children}'
      self.stderr.close()
      self.socket.close(linger=0)
      self.context.term()
      self.temporary.cleanup()

  def __enter__(self):
    return self

  def __exit__(self, *_):
    self.close()
