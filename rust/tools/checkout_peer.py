import ctypes
import json
import os
from pathlib import Path
import selectors
import signal
import struct
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from typing import Literal, TypedDict


class Response(TypedDict):
  commit: str | None
  running_commit: str | None
  reboot_required: bool
  returned: bool | None
  elapsed: float


@dataclass(frozen=True, slots=True)
class Binaries:
  trace: Path
  fixture: Path
  launcher: Path


def enable_subreaper() -> None:
  libc = ctypes.CDLL(None, use_errno=True)
  libc.prctl.argtypes = [ctypes.c_int, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_ulong]
  libc.prctl.restype = ctypes.c_int
  if libc.prctl(36, 1, 0, 0, 0) != 0:
    raise OSError(ctypes.get_errno(), 'checkout fixture subreaper setup failed')


class Peer:
  def __init__(self, binaries: Binaries, implementation: Literal['python', 'rust'], output: Path, fixture_mode: bool = False):
    self.binaries = binaries
    self.output = output / implementation
    self.output.mkdir(parents=True)
    self.temporary = tempfile.TemporaryDirectory(prefix='checkout-status-')
    self.root = Path(self.temporary.name)
    self.repo = self.root / 'repo'
    self.repo.mkdir()
    self.control = self.root / 'control'
    self.control.mkdir()
    self.bin = self.root / 'bin'
    self.bin.mkdir()
    self.bin.joinpath('git').symlink_to(binaries.fixture.resolve())
    self.held = self.root.joinpath('held-descriptor').open('wb')
    self.launches = self.root / 'launch-descriptors'
    self.launches.mkdir()
    self.events = []
    self.process = None
    self.env = {k: v for k, v in os.environ.items() if not k.startswith('GIT_')}
    self.env.update(GIT_CONFIG_GLOBAL='/dev/null', GIT_CONFIG_SYSTEM='/dev/null',
                    GIT_AUTHOR_NAME='checkout fixture', GIT_AUTHOR_EMAIL='fixture@example.invalid',
                    GIT_COMMITTER_NAME='checkout fixture', GIT_COMMITTER_EMAIL='fixture@example.invalid',
                    GIT_AUTHOR_DATE='2000-01-01T00:00:00+00:00', GIT_COMMITTER_DATE='2000-01-01T00:00:00+00:00')
    self.env['CHECKOUT_GIT_FIXTURE'] = str(self.control)
    self.env['CHECKOUT_HELD_FD'] = str(self.held.fileno())
    self.env['MANAGER_DAEMON'] = 'checkout-parent'
    self.env['TMPDIR'] = str(self.launches)
    self.fallback_bin = self.root / 'fallback-bin'
    self.fallback_bin.mkdir()
    self.env['PATH'] = os.pathsep.join([str(self.bin), str(self.fallback_bin)]) if fixture_mode else '/usr/bin:/bin'
    self.stderr = (self.output / 'stderr.log').open('wb')
    match implementation:
      case 'python':
        command = [sys.executable, str(Path(__file__).with_name('checkout_source.py'))]
      case 'rust':
        command = [str(binaries.trace.resolve())]
    self.process = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=self.stderr, text=True,
                                    env=self.env, pass_fds=(self.held.fileno(),))
    captured_env = {k: v for k, v in self.env.items() if k.startswith(('GIT_', 'CHECKOUT_')) or k in ['PATH', 'MANAGER_DAEMON', 'TMPDIR']}
    (self.output / 'command.json').write_text(json.dumps({'command': command, 'repo': str(self.repo), 'pid': self.process.pid,
                                                        'environment': captured_env}, indent=2) + '\n')
    self.selector = selectors.DefaultSelector()
    self.selector.register(self.process.stdout, selectors.EVENT_READ)
    self.process.stdin.write(json.dumps({'repo': str(self.repo), 'launcher': str(binaries.launcher.resolve())}) + '\n')
    self.process.stdin.flush()
    assert self.read() == {'ready': True}

  def read(self):
    assert self.selector.select(4), 'checkout RPC timeout'
    line = self.process.stdout.readline()
    assert line, (self.process.poll(), (self.output / 'stderr.log').read_text())
    return json.loads(line)

  def record(self, event) -> None:
    self.events.append(event)
    (self.output / 'transcript.json').write_text(json.dumps(self.events, indent=2) + '\n')

  def write(self, raw: bytes) -> None:
    self.repo.joinpath('build.json').write_bytes(raw)
    self.record({'write': 'build.json', 'hex': raw.hex()})

  def metadata(self, commit: str) -> None:
    self.write(json.dumps({'openpilot': {'git_commit': commit}}).encode())

  def op(self, operation: Literal['read', 'capture', 'update'], now: float = 0.0) -> Response:
    request = {'op': operation}
    if operation == 'update':
      request['now_bits'] = struct.unpack('<Q', struct.pack('<d', now))[0]
    self.process.stdin.write(json.dumps(request) + '\n')
    self.process.stdin.flush()
    response = self.read()
    children = [pid for task in Path(f'/proc/{self.process.pid}/task').glob('*/children') for pid in task.read_text().split()]
    descriptors = [str(path) for path in self.launches.iterdir()]
    self.record({'request': request, 'response': response, 'children_after': children, 'launch_descriptors_after': descriptors})
    assert not children, children
    assert not descriptors, descriptors
    return response

  def fixture(self, **behavior) -> None:
    self.control.joinpath('behavior.json').write_text(json.dumps(behavior) + '\n')
    self.record({'fixture': behavior})

  def calls(self):
    path = self.control / 'calls.jsonl'
    return [json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []

  def helper_calls(self):
    path = self.control / 'helper-calls.jsonl'
    return [json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []

  def git(self, *args: str) -> str:
    command = ['/usr/bin/git', *args]
    result = subprocess.run(command, cwd=self.repo, env=self.env, capture_output=True, text=True, timeout=5, check=True)
    self.record({'git_command': command, 'stdout': result.stdout, 'stderr': result.stderr, 'returncode': result.returncode})
    return result.stdout.strip()

  def close(self) -> None:
    try:
      if self.process is not None:
        if self.process.poll() is None:
          self.process.stdin.close()
          assert self.process.wait(timeout=3) == 0
    finally:
      if self.process is not None:
        if self.process.poll() is None:
          self.process.kill()
          self.process.wait(timeout=3)
        self.process.stdout.close()
        self.selector.close()
      calls = self.calls()
      (self.output / 'git-calls.json').write_text(json.dumps(calls, indent=2) + '\n')
      helper_calls = self.helper_calls()
      (self.output / 'helper-calls.json').write_text(json.dumps(helper_calls, indent=2) + '\n')
      calls += helper_calls
      for call in calls:
        pid = call['pid']
        path = Path(f'/proc/{pid}')
        if path.exists():
          try:
            waited, _ = os.waitpid(pid, os.WNOHANG)
          except ChildProcessError:
            assert not path.exists(), (pid, 'PID identity changed')
            continue
          if waited == 0:
            os.kill(pid, signal.SIGKILL)
            os.waitpid(pid, 0)
      cleanup = [{'pid': call['pid'], 'exists_after': Path(f"/proc/{call['pid']}").exists()} for call in calls]
      remaining = Path(f'/proc/self/task/{os.getpid()}/children').read_text().split()
      (self.output / 'cleanup.json').write_text(json.dumps({'fixtures': cleanup, 'remaining_children': remaining}, indent=2) + '\n')
      assert all(not row['exists_after'] for row in cleanup) and not remaining, (cleanup, remaining)
      self.stderr.close()
      self.held.close()
      self.temporary.cleanup()

  def __enter__(self):
    return self

  def __exit__(self, *_):
    self.close()
