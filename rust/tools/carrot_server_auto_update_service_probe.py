from __future__ import annotations

import json
import os
from pathlib import Path
import queue
import selectors
import subprocess
import threading
import time

from carrot_server_auto_update_service import ROOT


class Probe:
  def __init__(self, command: list[str], config: dict, environment: dict[str, str], artifact: Path) -> None:
    self.argv, self.config, self.artifact = command, config, artifact
    self.process = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
      text=True, env=environment, cwd=ROOT, start_new_session=True)
    self.messages: queue.Queue[str] = queue.Queue()
    self.stdout: list[str] = []
    self.reader = threading.Thread(target=self._read)
    self.reader.start()
    try:
      self.send(config)
      self.receive('ready')
    except BaseException:
      self.cleanup()
      raise

  def _read(self) -> None:
    assert self.process.stdout is not None
    for line in self.process.stdout:
      self.stdout.append(line)
      if line.startswith('{'):
        self.messages.put(line)

  def send(self, command: dict) -> None:
    assert self.process.stdin is not None
    self.process.stdin.write(json.dumps(command) + '\n')
    self.process.stdin.flush()

  def receive(self, key: str, timeout: float = 3.) -> dict:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
      row = json.loads(self.messages.get(timeout=max(.001, deadline - time.monotonic())))
      if key in row:
        return row
    raise TimeoutError(f'owned probe missing {key}')

  def command(self, command: dict, key: str = 'state') -> dict:
    self.send(command)
    return self.receive(key)

  def stop(self, *, wait: bool = True) -> None:
    self.send({'stop': True})
    assert self.process.stdin is not None
    self.process.stdin.close()
    if wait:
      self.finish()

  def finish(self) -> None:
    status = self.process.wait(timeout=5)
    self.reader.join(timeout=1)
    assert self.process.stderr is not None
    stderr = self.process.stderr.read()
    self.artifact.write_text(json.dumps({'argv': self.argv, 'config': self.config, 'exit': status, 'stdout': ''.join(self.stdout),
      'stderr': stderr, 'reader_stopped': not self.reader.is_alive()}, indent=2))
    assert status == 0 and not self.reader.is_alive(), (status, stderr)

  def cleanup(self) -> None:
    if self.process.poll() is None:
      try:
        os.killpg(self.process.pid, 9)
      except ProcessLookupError:
        pass
      self.process.wait(timeout=2)
    if self.process.stdin is not None and not self.process.stdin.closed:
      self.process.stdin.close()
    self.reader.join(timeout=1)
    if not self.artifact.exists():
      assert self.process.stderr is not None
      self.artifact.write_text(json.dumps({'argv': self.argv, 'exit': self.process.returncode,
        'stdout': ''.join(self.stdout), 'stderr': self.process.stderr.read(), 'reader_stopped': not self.reader.is_alive(), 'failure_cleanup': True}, indent=2))


def wait_file(path: Path, timeout: float = 3.) -> dict:
  deadline = time.monotonic() + timeout
  while not path.exists():
    if time.monotonic() >= deadline:
      raise TimeoutError(f'owned readiness absent: {path}')
    time.sleep(.01)
  return json.loads(path.read_text())


def exited(pidfd: int) -> bool:
  with selectors.DefaultSelector() as selector:
    selector.register(pidfd, selectors.EVENT_READ)
    return bool(selector.select(.5))
