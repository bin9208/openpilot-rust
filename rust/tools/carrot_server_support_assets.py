# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
from __future__ import annotations

import ast
import hashlib
from pathlib import Path

from aiohttp import ClientSession, WSMsgType
from carrot_server_support_fixture import Fixture


async def run(fixture: Fixture) -> None:
  source = Path('openpilot/selfdrive/carrot/server/services/support_terminal.py')
  tree = ast.parse(source.read_text())
  declaration = next(node for node in tree.body if isinstance(node, ast.Assign) and any(isinstance(target, ast.Name) and target.id == 'SUPPORT_GUEST_ASSETS' for target in node.targets))
  assert isinstance(declaration.value, ast.Dict)
  paths = {}
  for key, value in zip(declaration.value.keys, declaration.value.values, strict=True):
    assert isinstance(key, ast.Constant) and isinstance(value, ast.Call)
    pieces = [argument.value for argument in value.args[1:] if isinstance(argument, ast.Constant)]
    paths[key.value] = Path('openpilot/selfdrive/carrot/web').joinpath(*pieces)
  observations = []
  async with ClientSession(auto_decompress=False, headers={'Accept-Encoding': 'identity'}) as client:
    async with client.get(fixture.url + '/support/terminal/a%22b%26c', headers={'Host': 'owned.example:9876'}) as response:
      page = await response.read()
      assert response.status == 200 and b'a&quot;b&amp;c' in page
      assert response.headers['Cache-Control'] == 'no-store'
      assert response.headers['X-Frame-Options'] == 'DENY'
      assert response.headers['Referrer-Policy'] == 'no-referrer'
      assert response.headers['X-Content-Type-Options'] == 'nosniff'
      assert "connect-src 'self' ws://owned.example:9876 wss://owned.example:9876;" in response.headers['Content-Security-Policy']
      await fixture.record('page.json', {'status': response.status, 'sha256': hashlib.sha256(page).hexdigest(), 'headers': dict(response.headers)})
    for name, path in paths.items():
      async with client.get(fixture.url + '/support-terminal-assets/' + name) as response:
        body = await response.read()
        assert response.status == (200 if path.is_file() else 404)
        if path.is_file():
          assert body == path.read_bytes()
          assert response.headers['Cache-Control'] == 'no-store'
          assert response.headers['Referrer-Policy'] == 'no-referrer'
          assert response.headers['X-Content-Type-Options'] == 'nosniff'
        observations.append({'name': name, 'status': response.status, 'sha256': hashlib.sha256(body).hexdigest(), 'content_type': response.headers['Content-Type']})
    async with client.get(fixture.url + '/support-terminal-assets/unknown.js') as response:
      assert response.status == 404
    async with client.head(fixture.url + '/support-terminal-assets/xterm.js') as response:
      assert response.status == 200 and await response.read() == b''
      assert int(response.headers['Content-Length']) == paths['xterm.js'].stat().st_size
    async with client.get(fixture.url + '/support-terminal-assets/xterm.js', headers={'Range': 'bytes=5-17'}) as response:
      assert response.status == 206 and await response.read() == paths['xterm.js'].read_bytes()[5:18]
    for path in ['/api/support_terminal/start', '/api/support_terminal/stop', '/api/support_terminal/commands/absent/approve']:
      async with client.get(fixture.url + path) as response:
        assert response.status == 405 and response.headers['Allow'] == 'POST'
    async with client.post(fixture.url + '/api/support_terminal/commands/absent/approve') as response:
      assert response.status == 400 and (await response.json())['error'] == 'no active session'
    async with client.ws_connect(fixture.url + '/ws/support_terminal/owner') as socket:
      assert (await socket.receive_json()) == {'type': 'session_status', 'ok': True, 'active': False, 'state': 'idle'}
      message = await socket.receive()
      assert message.type == WSMsgType.CLOSE and message.data == 1000
    async with client.ws_connect(fixture.url + '/ws/support_terminal/absent') as socket:
      assert (await socket.receive_json()) == {'type': 'error', 'message': 'session unavailable'}
      message = await socket.receive()
      assert message.type == WSMsgType.CLOSE and message.data == 1000
  await fixture.record('assets-result.json', {'assets': observations, 'page_escaped': True, 'security_headers': True, 'head': True, 'range': True, 'methods': True, 'idle_owner_close': True, 'unavailable_guest_close': True})
