# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Owned CLI roots/process identities shared by terminal bridge and vision controls."""

from __future__ import annotations

from dataclasses import dataclass
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import sys
import time
from typing import TypeAlias, TypedDict
import uuid

import anyio
from carrot_server_dashcam_upload import save
from carrot_server_terminal_cleanup import close, identity

Json: TypeAlias = str | int | float | bool | None | list['Json'] | dict[str, 'Json']


@dataclass(frozen=True, slots=True)
class Inputs:
  native: Path
  launcher: Path
  binding: Path
  vision_root: Path


class Result(TypedDict):
  argv: list[str]
  out: str
  error: str
  rc: int
  seconds: float


class Fixture:
  def __init__(self, root: Path, provider: str, inputs: Inputs) -> None:
    self.root, self.provider, self.inputs = root, provider, inputs
    self.config = root / 'cli-config.json'
    self.params = root / 'params'
    self.prefix = 'owned_terminal_' + uuid.uuid4().hex
    self.environment = os.environ | {
      'PARAMS_ROOT': str(self.params),
      'OPENPILOT_PREFIX': self.prefix,
      'CARROT_DATA_DIR': str(root),
      'CARROT_SETTINGS_PATH': str(root / 'settings.json'),
      'OWNED_TERMINAL_CONFIG': str(self.config),
      'ORIGINAL_PARAMS_BINDING': str(inputs.binding),
    }
    self.observations: list[Result] = []

  async def setup(self) -> None:
    await anyio.Path('/dev/shm', 'msgq_' + self.prefix).mkdir()
    for name in ['params/' + self.prefix, 'state', 'repository/openpilot/selfdrive/selfdrived']:
      await anyio.Path(self.root / name).mkdir(parents=True)
    await anyio.to_thread.run_sync(shutil.copyfile, Path('openpilot/selfdrive/carrot_settings.json'), self.root / 'settings.json')
    await anyio.to_thread.run_sync(
      shutil.copyfile, Path('openpilot/selfdrive/carrot/web/src/features/drive/core/content_catalog.json'), self.root / 'content-catalog.json'
    )
    await anyio.to_thread.run_sync(
      shutil.copyfile, Path('openpilot/selfdrive/selfdrived/alerts_offroad.json'), self.root / 'repository/openpilot/selfdrive/selfdrived/alerts_offroad.json'
    )
    for key, value in [('DisableDM', b'12'), ('IsOffroad', b'1'), ('IsOnroad', b'0'), ('IsTakingSnapshot', b'0')]:
      await anyio.Path(self.params / self.prefix / key).write_bytes(value)
    with socket.socket() as listener:
      listener.bind(('127.0.0.1', 0))
      port = listener.getsockname()[1]
    for role in ['camerad', 'stream_encoderd', 'webrtcd']:
      child = self.root / role
      await anyio.Path(child).write_text(f'#!{sys.executable}\nfrom carrot_server_terminal_vision_children import main\nmain({role!r})\n')
      await anyio.Path(child).chmod(0o755)
    value = {
      'owned_root': str(self.root),
      'params_root': str(self.params),
      'prefix': self.prefix,
      'camera': str(self.root / 'camerad'),
      'encoder': str(self.root / 'stream_encoderd'),
      'webrtc': str(self.root / 'webrtcd'),
      'launcher': str(self.inputs.launcher),
      'vision_root': str(self.inputs.vision_root),
      'port': port,
    }
    await anyio.to_thread.run_sync(save, self.config, value)

  async def command(self, args: list[str]) -> Result:
    argv = (
      [sys.executable, '-P', str(Path(__file__).with_name('carrot_server_terminal_command_source.py'))]
      if self.provider == 'source'
      else [str(self.inputs.native)]
    )
    argv += ['--config', str(self.config), *args]
    began = time.monotonic()
    with anyio.fail_after(35):
      result = await anyio.run_process(argv, env=self.environment, stdin=subprocess.DEVNULL, check=False)
    observation: Result = {
      'argv': argv,
      'out': result.stdout.decode(),
      'error': result.stderr.decode(),
      'rc': result.returncode,
      'seconds': time.monotonic() - began,
    }
    self.observations.append(observation)
    await anyio.to_thread.run_sync(save, self.root / 'commands.json', self.observations)
    return observation

  async def state(self):
    path = anyio.Path(self.root / 'vision-state.json')
    return json.loads(await path.read_text()) if await path.exists() else {}

  async def settled(self) -> None:
    with anyio.fail_after(8):
      while True:
        state = await self.state()
        if not state.get('children') and not await anyio.Path(f'/proc/{state.get("runner_pid", 0)}/cmdline').exists():
          break
        # A source orphan can briefly be a zombie while its original parent exits.
        cmdline = anyio.Path(f'/proc/{state.get("runner_pid", 0)}/cmdline')
        if not state.get('children') and await cmdline.exists() and not await cmdline.read_bytes():
          break
        await anyio.sleep(0.05)
    assert await anyio.Path(self.params / self.prefix / 'IsTakingSnapshot').read_bytes() == b'0'
    for role in ['camerad', 'stream_encoderd', 'webrtcd']:
      record = anyio.Path(self.root / (role + '-started.json'))
      if await record.exists():
        child = json.loads(await record.read_text())
        cmdline = anyio.Path(f'/proc/{child["pid"]}/cmdline')
        assert not await cmdline.exists() or not await cmdline.read_bytes()
    await anyio.to_thread.run_sync(
      save, self.root / 'settled-before-fixture-close.json', {'state': state, 'snapshot_cleared': True, 'owned_children_exited': True}
    )

  def cleanup(self) -> None:
    state = json.loads((self.root / 'vision-state.json').read_text()) if (self.root / 'vision-state.json').exists() else {}
    pid = state.get('runner_pid', 0)
    owned = []
    if pid:
      try:
        environment = Path(f'/proc/{pid}/environ').read_bytes()
        if ('OWNED_TERMINAL_CONFIG=' + str(self.config)).encode() in environment.split(b'\0'):
          current = identity(pid)
          if current:
            owned.append((pid, current[0]))
      except (FileNotFoundError, ProcessLookupError):
        pid = 0
    for name in ['camerad-started.json', 'stream_encoderd-started.json', 'webrtcd-started.json', 'capture-child.json']:
      record = self.root / name
      if record.exists():
        child = json.loads(record.read_text())
        try:
          current = identity(child['pid'])
          if current and current[0] == child['starttime']:
            owned.append((child['pid'], child['starttime']))
        except (FileNotFoundError, ProcessLookupError):
          continue
    results = []
    errors = []
    for pid, starttime in owned:
      try:
        results.append(close(pid, starttime))
      except (OSError, TimeoutError) as error:
        errors.append(error)
    save(self.root / 'fixture-cleanup.json', {'owned_processes': results, 'errors': [str(error) for error in errors]})
    if errors:
      raise ExceptionGroup('owned CLI fixture cleanup', errors)
