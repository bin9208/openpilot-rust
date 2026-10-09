# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Actual terminal peers with owned real PTY, native/original CLI and tmux adapters."""

from __future__ import annotations

import json
from pathlib import Path
import subprocess
import sys

import anyio
from anyio.streams.buffered import BufferedByteReceiveStream
from carrot_server_dashcam_upload import save
from carrot_server_terminal_cli_fixture import Fixture, Inputs
from carrot_server_terminal_edges import cleanup_groups
from carrot_server_terminal_providers import prepare


def programs(root: Path, source: bool, cli: Path, config: Path) -> None:
  python = root / 'bin/python3'
  python.write_text(f'#!{sys.executable}\nfrom carrot_server_terminal_provider import main\nmain()\n')
  python.chmod(0o755)
  command = root / 'bin/carrot-command'
  command.write_text(f'#!/bin/sh\nexec "{cli}" --config "{config}" "$@"\n')
  command.chmod(0o755)
  tmux = root / 'bin/tmux'
  tmux.write_text(
    '\n'.join(
      [
        f'#!{sys.executable}',
        'import sys,json',
        'from pathlib import Path',
        f'root=Path({str(root)!r})',
        'args=sys.argv[1:]',
        'with (root/"tmux-provider.jsonl").open("a") as log: log.write(json.dumps(args)+"\\n")',
        'if args[0]=="has-session": raise SystemExit(0 if (root/"tmux-session").exists() else 1)',
        'if args[0]=="new-session": (root/"tmux-session").write_text("owned")',
        'if args[0]=="capture-pane": print("owned pane\\nsecond line")',
      ]
    )
    + '\n'
  )
  tmux.chmod(0o755)


class Peer:
  def __init__(self, root: Path, source: bool, inputs: Inputs, server: Path, application: bool) -> None:
    self.fixture = Fixture(root, 'source' if source else 'native', inputs)
    self.root, self.source, self.server, self.application = root, source, server, application
    self.process: anyio.abc.Process | None = None
    self.reader: BufferedByteReceiveStream | None = None
    self.log: anyio.AsyncFile[bytes] | None = None
    self.port = 0
    self.shell: Path | None = None

  async def start(self) -> None:
    await self.fixture.setup()
    owned = await anyio.to_thread.run_sync(prepare, self.root)
    environment = self.fixture.environment.copy()
    for name in ['PATH', 'SHELL', 'HOME', 'USER', 'PS1', 'TMUX', 'TERM', 'COLORTERM', 'BASH_ENV', 'ENV']:
      if name in owned:
        environment[name] = owned[name]
      else:
        environment.pop(name, None)
    if self.shell:
      environment['SHELL'] = str(self.shell)
    await anyio.to_thread.run_sync(programs, self.root, self.source, self.fixture.inputs.native, self.fixture.config)
    await anyio.Path(self.root / 'web').mkdir()
    await anyio.Path(self.root / 'web/index.html').write_text('owned terminal application')
    argv = [sys.executable, '-P', str(Path(__file__).with_name('carrot_server_terminal_source.py'))] if self.source else [str(self.server)]
    config = {
      'owned_root': str(self.root),
      'launcher': str(self.fixture.inputs.launcher),
      'cli_config': str(self.fixture.config),
      'application': self.application,
    }
    self.log = await anyio.Path(self.root / 'server.log').open('wb')
    self.process = await anyio.open_process(argv, env=environment, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=self.log.wrapped)
    assert self.process.stdin and self.process.stdout
    self.reader = BufferedByteReceiveStream(self.process.stdout)
    await self.process.stdin.send((json.dumps(config) + '\n').encode())
    with anyio.fail_after(8):
      ready = json.loads(await self.reader.receive_until(b'\n', 65536))
    self.port = ready['port']
    await anyio.to_thread.run_sync(save, self.root / 'server-invocation.json', {'argv': argv, 'config': config, 'ready': ready})

  async def close(self) -> None:
    errors = []
    if self.process:
      if self.port == 0 and self.process.returncode is None:
        self.process.terminate()
      try:
        if self.port != 0 and self.process.returncode is None and self.process.stdin:
          await self.process.stdin.send(b'cleanup\n')
          if self.reader:
            with anyio.move_on_after(3):
              await self.reader.receive_until(b'\n', 65536)
          if self.source and self.process.returncode is None:
            await self.process.stdin.send(b'exit\n')
      except (OSError, anyio.BrokenResourceError, anyio.EndOfStream, anyio.IncompleteRead) as error:
        errors.append(error)
      groups = await anyio.to_thread.run_sync(cleanup_groups, self.root)
      with anyio.move_on_after(4):
        await self.process.wait()
      if self.process.returncode is None:
        self.process.terminate()
        with anyio.move_on_after(3):
          await self.process.wait()
      if self.process.returncode is None:
        self.process.kill()
        await self.process.wait()
      await anyio.to_thread.run_sync(save, self.root / 'server-cleanup.json', {'exit': self.process.returncode, 'owned_groups': groups})
      try:
        await self.process.aclose()
      except OSError as error:
        errors.append(error)
    try:
      await anyio.to_thread.run_sync(self.fixture.cleanup)
    except (OSError, TimeoutError, ExceptionGroup) as error:
      errors.append(error)
    finally:
      if self.log:
        await self.log.aclose()
    if errors:
      raise ExceptionGroup('terminal peer cleanup', errors)
