from __future__ import annotations

from pathlib import Path
import ssl
import time
from typing import TypedDict

import anyio
from aiohttp import web


class Receiver(TypedDict):
  received: anyio.Event
  chunks: list[float]
  runner: web.AppRunner


async def held_receiver(root: Path, enabled: bool) -> Receiver:
  received = anyio.Event()
  chunks = []
  runner = web.AppRunner(web.Application())
  if not enabled:
    await runner.setup()
    return {'received': received, 'chunks': chunks, 'runner': runner}

  async def heartbeat(request: web.Request) -> web.StreamResponse:
    await request.read()
    response = web.StreamResponse(headers={'Content-Length': '100'})
    await response.prepare(request)
    received.set()
    try:
      for _ in range(100):
        await response.write(b'a')
        chunks.append(time.monotonic())
        await anyio.sleep(1)
    except ConnectionResetError as error:
      (root / 'receiver-closed.txt').write_text(str(error))
    return response

  application = web.Application()
  application.router.add_post('/carrot/api_heartbeat.php', heartbeat)
  runner = web.AppRunner(application, shutdown_timeout=0.1)
  await runner.setup()
  context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
  context.load_cert_chain(root.parent / 'owned-cert.pem', root.parent / 'owned-key.pem')
  await web.TCPSite(runner, '127.0.0.1', 443, ssl_context=context).start()
  return {'received': received, 'chunks': chunks, 'runner': runner}
