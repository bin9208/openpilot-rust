# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Owned CLI/service inputs and durable child identities for both providers."""

from __future__ import annotations

import json
import os
from pathlib import Path
import subprocess
import sys
import time
from dataclasses import dataclass
from typing import TypedDict

import anyio
from carrot_server_dashcam_upload import save
from carrot_server_youtube_fixture import Peer, Provider
from carrot_server_youtube_ingest import Ingest


@dataclass(frozen=True, slots=True)
class Inputs:
  service: Path
  cli: Path
  launcher: Path
  vision_root: Path
  frames: Path
  certificate: tuple[Path, Path]


class CommandResult(TypedDict):
  argv: list[str]
  stdout: str
  stderr: str
  exit: int
  seconds: float


class Fixture:
  def __init__(self, root: Path, provider: str, inputs: Inputs) -> None:
    self.root, self.provider, self.inputs = root, provider, inputs
    self.peer = Peer(Provider(provider, inputs.service, True), root, Ingest(root, inputs.certificate))
    self.config = root / 'cli-config.json'
    self.environment = os.environ | {
      'PARAMS_ROOT': str(self.peer.params),
      'OPENPILOT_PREFIX': self.peer.prefix,
      'CARROT_DATA_DIR': str(root),
      'OWNED_YOUTUBE_TEST_CONFIG': str(self.config),
      'SSL_CERT_FILE': str(inputs.certificate[0]),
    }
    self.argv = [sys.executable, '-P', str(Path(__file__).with_name('carrot_server_youtube_test_source.py'))] if provider == 'source' else [str(inputs.cli)]
    self.runner = None

  async def setup(self) -> None:
    await anyio.Path(self.peer.params / self.peer.prefix).mkdir(parents=True)
    await anyio.Path(self.root / 'state').mkdir()
    await anyio.Path('/dev/shm', 'msgq_' + self.peer.prefix).mkdir()
    for key, value in [('CarrotYouTubeLive', b'0'), ('IsOffroad', b'1'), ('CarrotYouTubeTimestamp', b'0')]:
      await anyio.Path(self.peer.params / self.peer.prefix / key).write_bytes(value)
    await anyio.Path(self.root / 'state/youtube_live_secret.json').write_text('{"stream_key":"owned-stream-key"}')
    await self.peer.start(self.inputs.certificate[0])
    for name in ['camerad', 'encoderd']:
      source = ''.join(
        [
          f'#!{sys.executable}\nimport sys\n',
          f'sys.path.insert(0, {str(Path(__file__).parent.resolve())!r})\n',
          f'from carrot_server_youtube_test_children import main\nmain({name!r})\n',
        ]
      )
      child = anyio.Path(self.root / name)
      await child.write_text(source)
      await child.chmod(0o755)
    value = {
      'owned_root': str(self.root),
      'params_root': str(self.peer.params),
      'prefix': self.peer.prefix,
      'repository': str(Path.cwd()),
      'camera': str(self.root / 'camerad'),
      'encoder': str(self.root / 'encoderd'),
      'launcher': str(self.inputs.launcher),
      'vision_root': str(self.inputs.vision_root),
      'frames': str(self.inputs.frames),
      'status_url': f'http://127.0.0.1:{self.peer.port}/api/youtube_live/status',
    }
    await anyio.to_thread.run_sync(save, self.config, value)

  async def command(self, name: str, args: list[str]) -> CommandResult:
    argv = [*self.argv, '--config', str(self.config), *args]
    began = time.monotonic()
    process = await anyio.open_process(argv, env=self.environment, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    try:
      assert process.stdout and process.stderr
      result = {'argv': argv}
      async with anyio.create_task_group() as group:

        async def read(name: str, stream: anyio.abc.ByteReceiveStream) -> None:
          data = bytearray()
          async for chunk in stream:
            data.extend(chunk)
          result[name] = data.decode()

        group.start_soon(read, 'stdout', process.stdout)
        group.start_soon(read, 'stderr', process.stderr)
        with anyio.fail_after(100):
          await process.wait()
      result['exit'] = process.returncode
      result['seconds'] = time.monotonic() - began
    finally:
      if process.returncode is None:
        process.kill()
        await process.wait()
      await process.aclose()
    await anyio.to_thread.run_sync(save, self.root / (name + '-invocation.json'), result)
    state = self.root / 'test-state.json'
    if await anyio.Path(state).exists():
      self.runner = json.loads(await anyio.Path(state).read_text()).get('runner_pid')
    return result

  async def close(self) -> None:
    errors = []
    state = anyio.Path(self.root / 'test-state.json')
    if await state.exists():
      self.runner = json.loads(await state.read_text()).get('runner_pid')
    try:
      if self.runner:
        stopped = await self.command('cleanup-stop', ['stop'])
        if stopped['exit'] != 0:
          errors.append(stopped)
    finally:
      errors.extend(await self.peer.close())
    shared = anyio.Path('/dev/shm', 'msgq_' + self.peer.prefix)
    if await shared.exists():
      async for file in shared.iterdir():
        await file.unlink()
      await shared.rmdir()
    await anyio.to_thread.run_sync(save, self.root / 'fixture-cleanup.json', {'errors': errors})
    assert not errors
