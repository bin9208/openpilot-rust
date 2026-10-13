# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
from __future__ import annotations

import json
import hashlib
from pathlib import Path
import re
from typing import TypedDict

import anyio
from aiohttp import ClientWebSocketResponse, web
from carrot_server_dashcam_upload import save
from carrot_server_terminal_cli_fixture import Inputs, Json
from carrot_server_terminal_http_fixture import Peer
from carrot_server_support_providers import prepare


class Packet(TypedDict, total=False):
  type: str
  id: str
  pid: int
  primary: bool
  granted: bool
  remaining: int
  owner_present: bool
  control_granted: bool
  text: str
  reason: str
  b64: str


def digest(path: Path) -> str:
  with path.open('rb') as stream:
    return hashlib.file_digest(stream, 'sha256').hexdigest()


async def packet(socket: ClientWebSocketResponse, kind: str) -> Packet:
  with anyio.fail_after(6):
    while True:
      data = await socket.receive_json()
      if data['type'] == kind:
        return data


class Fixture:
  """Accumulates captured webhook payloads and owns loopback server lifetime."""

  def __init__(self, root: Path, inputs: Inputs, server: Path, source: bool, application: bool) -> None:
    self.root = root
    self.received: list[Json] = []
    self.runner: web.AppRunner | None = None
    self.peer = Peer(root, source, inputs, server, application)
    self.peer.source_program = Path(__file__).with_name('carrot_server_support_source.py')
    self.url = ''

  async def start(self) -> None:
    async def webhook(request: web.Request) -> web.Response:
      self.received.append(await request.json())
      await anyio.to_thread.run_sync(save, self.root / 'webhook.json', self.received)
      return web.Response(status=204)

    receiver = web.Application()
    receiver.router.add_post('/hook', webhook)
    self.runner = web.AppRunner(receiver)
    await self.runner.setup()
    listener = await anyio.create_tcp_listener(local_host='127.0.0.1', local_port=0)
    port = listener.extra(anyio.abc.SocketAttribute.local_address)[1]
    await listener.aclose()
    await web.TCPSite(self.runner, '127.0.0.1', port).start()
    original_setup = self.peer.fixture.setup

    async def setup() -> None:
      await original_setup()
      await anyio.Path(self.root / 'bin').mkdir()
      await anyio.to_thread.run_sync(prepare, self.root, f'http://127.0.0.1:{port}/hook')

    self.peer.fixture.setup = setup
    await self.peer.start()
    if self.peer.process:
      paths = {'running_executable': Path(f'/proc/{self.peer.process.pid}/exe'), 'command': self.peer.fixture.inputs.native, 'launcher': self.peer.fixture.inputs.launcher, 'params_binding': self.peer.fixture.inputs.binding}
      identities = {}
      for name, path in paths.items():
        hashed = await anyio.to_thread.run_sync(digest, path)
        identities[name] = {'path': str(path), 'sha256': hashed}
      await self.record('identities.json', identities)
    self.url = f'http://127.0.0.1:{self.peer.port}'

  def pin(self) -> str:
    assert self.received
    payload = self.received[-1]
    assert isinstance(payload, dict) and isinstance(payload['content'], str)
    matched = re.search(r'PIN: \*\*__(\d{6})__\*\*', payload['content'])
    assert matched
    return matched.group(1)

  async def close(self) -> None:
    with anyio.CancelScope(shield=True):
      try:
        await self.peer.close()
      finally:
        if self.runner:
          await self.runner.cleanup()

  async def record(self, name: str, value: Json) -> None:
    await anyio.to_thread.run_sync(save, self.root / name, value)
