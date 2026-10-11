# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
from __future__ import annotations

import json
import time

import anyio
from aiohttp import ClientSession
from carrot_server_support_fixture import Fixture
from carrot_server_terminal_cleanup import identity


async def overlong(fixture: Fixture, stream: str) -> None:
  cloud = anyio.Path(fixture.root / 'cloudflared')
  script = await cloud.read_text()
  script = script.replace('print("https://owned-support.trycloudflare.com",flush=True)', f'print("x"*65537,file=sys.{stream},flush=True)')
  await cloud.write_text(script)
  async with ClientSession() as client:
    began = time.monotonic()
    async with client.post(fixture.url + '/api/support_terminal/start', json={}) as response:
      session = await response.json()
      elapsed = time.monotonic() - began
      child = json.loads(await anyio.Path(fixture.root / 'cloud-child.json').read_text())
      child_gone = await anyio.to_thread.run_sync(identity, child['pid']) is None
      await fixture.record('overlong-result.json', {'status': response.status, 'error': session['error'], 'state': session['state'], 'seconds': elapsed, 'child_reaped': child_gone, 'stream': stream})
      assert response.status == 500 and session['state'] == 'error' and 'limit' in session['error']
      assert elapsed < 3 and child_gone
