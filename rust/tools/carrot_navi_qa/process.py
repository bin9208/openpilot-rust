from __future__ import annotations

import asyncio
from dataclasses import dataclass, field
import hashlib
import json
import os
from pathlib import Path
import signal
import socket
import subprocess
from typing import BinaryIO

import aiohttp


def digest(path: Path) -> str:
  return hashlib.sha256(path.read_bytes()).hexdigest()


def available_port() -> int:
  with socket.socket() as listener:
    listener.bind(('127.0.0.1', 0))
    return int(listener.getsockname()[1])


@dataclass(slots=True)
class Process:
  command: list[str]
  root: Path
  output: Path
  environment: dict[str, str]
  process: subprocess.Popen[bytes] = field(init=False)
  files: list[BinaryIO] = field(default_factory=list, init=False)

  def start(self) -> None:
    self.output.mkdir(parents=True, exist_ok=False)
    self.files = [(self.output / name).open('wb') for name in ('stdout.log', 'stderr.log')]
    self.process = subprocess.Popen(self.command, cwd=self.root, env=self.environment,
      stdin=subprocess.DEVNULL, stdout=self.files[0], stderr=self.files[1])
    (self.output / 'command.json').write_text(json.dumps({'command': self.command,
      'cwd': str(self.root), 'environment': {name: self.environment[name] for name in
        ('PYTHONPATH', 'PARAMS_ROOT', 'OPENPILOT_PREFIX') if name in self.environment}}, indent=2))

  def capture(self) -> None:
    proc = Path('/proc') / str(self.process.pid)
    executable = proc.joinpath('exe').resolve()
    (self.output / 'process.json').write_text(json.dumps({'pid': self.process.pid,
      'exe': str(executable), 'sha256': digest(executable)}, indent=2))
    (self.output / 'maps.txt').write_text(proc.joinpath('maps').read_text())
    (self.output / 'fds.json').write_text(json.dumps({entry.name: os.readlink(entry)
      for entry in proc.joinpath('fd').iterdir()}, indent=2))

  async def ready(self, client: aiohttp.ClientSession, url: str) -> None:
    deadline = asyncio.get_running_loop().time() + 15
    while self.process.poll() is None:
      try:
        async with client.get(url + '/health') as response:
          if response.status == 200:
            self.capture()
            return
      except (aiohttp.ClientConnectionError, TimeoutError):
        pass
      if asyncio.get_running_loop().time() >= deadline:
        raise TimeoutError('receiver startup health did not become available')
      await asyncio.sleep(.01)
    raise RuntimeError(f'receiver exited before health: {self.process.returncode}')

  async def stop(self, selected_signal: signal.Signals = signal.SIGINT) -> int:
    if self.process.poll() is None:
      self.process.send_signal(selected_signal)
    deadline = asyncio.get_running_loop().time() + 15
    while self.process.poll() is None:
      if asyncio.get_running_loop().time() >= deadline:
        self.process.kill()
        self.process.wait()
        raise TimeoutError('receiver did not terminate after signal')
      await asyncio.sleep(.01)
    for file in self.files:
      file.close()
    return int(self.process.returncode)
