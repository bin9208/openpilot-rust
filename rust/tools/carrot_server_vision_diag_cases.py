# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
from __future__ import annotations

from enum import StrEnum
import json
import time
from typing import assert_never

from aiohttp import ClientSession
import anyio
from carrot_server_dashcam_upload import save
from carrot_server_terminal_http_fixture import Peer


class Mode(StrEnum):
  BASIC = 'basic'
  NEGATIVE = 'negative'
  LARGE = 'large'
  LARGE_CONSOLE = 'large-console'
  HTTP_ERROR = 'http-error'
  TIMEOUT = 'timeout'
  CLOSED = 'closed'
  SKIP = 'skip'
  INVALID = 'invalid'
  ALIAS = 'alias'
  FALLBACK = 'fallback'
  PARAM_READ = 'param-read'
  STATUS_ERROR = 'status-error'
  FACTORY = 'factory'
  LIVE_PROCESS = 'live-process'
  HISTORY = 'history'


def configure(mode: Mode, peer: Peer, hook: str) -> None:
  environment = peer.fixture.environment
  for key in ['CARROT_VISION_DIAG_DISCORD_WEBHOOK_URL', 'CARROT_DISCORD_WEBHOOK_URL', 'DISCORD_WEBHOOK_URL']:
    environment[key] = ''
  environment['CARROT_VISION_DIAG_DISCORD_WEBHOOK_DISABLE'] = '1'
  match mode:
    case Mode.SKIP | Mode.ALIAS:
      return
    case Mode.INVALID:
      environment['CARROT_VISION_DIAG_DISCORD_WEBHOOK_URL'] = 'owned-invalid'
    case Mode.FALLBACK:
      environment['CARROT_DISCORD_WEBHOOK_URL'] = hook
    case Mode.BASIC | Mode.NEGATIVE | Mode.LARGE | Mode.LARGE_CONSOLE | Mode.HTTP_ERROR | Mode.TIMEOUT | Mode.CLOSED | Mode.PARAM_READ | Mode.STATUS_ERROR | Mode.FACTORY | Mode.LIVE_PROCESS | Mode.HISTORY:
      environment['CARROT_VISION_DIAG_DISCORD_WEBHOOK_URL'] = hook
      environment['CARROT_DISCORD_WEBHOOK_URL'] = 'owned-lower-priority-invalid'
    case unreachable:
      assert_never(unreachable)


async def run(mode: Mode, client: ClientSession, peer: Peer) -> bool:
  url = f'http://127.0.0.1:{peer.port}'
  root = peer.root
  match mode:
    case Mode.BASIC:
      return False
    case Mode.PARAM_READ:
      async with client.get(url + '/api/vision_diag/server_snapshot') as response:
        result = await response.json()
        await anyio.to_thread.run_sync(save, root / 'case-result.json', {'status': response.status, 'body': result})
        assert response.status == 200 and result['snapshot']['params'] == {'available': True, 'IsOffroad': False, 'IsTakingSnapshot': False, 'CarParamsPersistent': {'present': False, 'bytes': 0}}
    case Mode.FACTORY:
      await anyio.Path(peer.fixture.params).rename(root / 'params-held')
      await anyio.Path(peer.fixture.params).write_text('owned constructor failure')
      async with client.get(url + '/api/vision_diag/server_snapshot') as response:
        await anyio.to_thread.run_sync(save, root / 'case-result.json', {'status': response.status, 'body': await response.text()})
        assert response.status == 500
    case Mode.LIVE_PROCESS:
      async with client.get(url + '/api/vision_diag/server_snapshot') as response:
        result = await response.json()
        await anyio.to_thread.run_sync(save, root / 'case-result.json', {'status': response.status, 'body': result})
        assert peer.process and response.status == 200
        processes = result['snapshot']['processes']['carrot_web']
        assert any(process['pid'] == peer.process.pid for process in processes)
        if not peer.source:
          assert result['snapshot']['system']['runtime'] == 'rust' and result['snapshot']['system']['python'] is None
    case Mode.HISTORY:
      for index in range(121):
        async with client.post(url + '/stream', json={'client_id': f'owned{index}', 'sdp': 'owned', 'cameras': ['road']}) as response:
          assert response.status == 201 and await response.read() == b'owned-stream'
      async with client.get(url + '/api/vision_diag/server_snapshot') as response:
        result = await response.json()
        history = result['snapshot']['stream_proxy_history']
        await anyio.to_thread.run_sync(save, root / 'case-result.json', {'status': response.status, 'history': history})
        assert response.status == 200 and len(history) == 120
        assert history[0]['client_id'] == 'owned1' and history[-1]['client_id'] == 'owned120'
        assert all(entry['status'] == 201 and entry['response_bytes'] == 12 for entry in history)
    case Mode.NEGATIVE:
      observations = []
      cases = [(b'{', 400), (b'{}', 400), (b'[]', 500), (b'x' * (16 * 1024 * 1024), 413)]
      for body, expected in cases:
        async with client.post(url + '/api/vision_diag/upload_discord', data=body, headers={'Content-Type': 'application/json'}) as response:
          observations.append({'bytes': len(body), 'status': response.status, 'body': await response.text()})
          await anyio.to_thread.run_sync(save, root / 'negative-result.json', observations)
          assert response.status == expected
      for path, method in [('/api/vision_diag/server_snapshot', 'POST'), ('/api/vision_diag/upload_discord', 'GET')]:
        async with client.request(method, url + path) as response:
          assert response.status == 405
          observations.append({'path': path, 'method': method, 'status': response.status, 'allow': response.headers.get('Allow')})
      async with client.head(url + '/api/vision_diag/server_snapshot') as response:
        assert response.status == 200 and not await response.read()
        observations.append({'head': True, 'status': response.status, 'length': response.headers.get('Content-Length')})
      await anyio.to_thread.run_sync(save, root / 'negative-result.json', observations)
    case Mode.LARGE | Mode.LARGE_CONSOLE:
      body = {'bundle': 'owned large', 'filename': 'owned-large.txt', 'console': ''}
      match mode:
        case Mode.LARGE:
          body['bundle'] = '가' * 2_800_000
        case Mode.LARGE_CONSOLE:
          body['console'] = '나' * 2_800_000
        case unreachable:
          assert_never(unreachable)
      async with client.post(url + '/api/vision_diag/upload_discord', data=json.dumps(body, ensure_ascii=False).encode(), headers={'Content-Type': 'application/json'}) as response:
        result = await response.json()
        await anyio.to_thread.run_sync(save, root / 'case-result.json', {'status': response.status, 'body': result})
        assert response.status == 200 and result['ok']
        key = 'bytes' if mode is Mode.LARGE else 'console_bytes'
        assert 8_000_000 < result['discord'][key] <= 8 * 1024 * 1024
    case Mode.HTTP_ERROR | Mode.TIMEOUT | Mode.CLOSED | Mode.SKIP | Mode.INVALID | Mode.ALIAS | Mode.FALLBACK | Mode.STATUS_ERROR:
      began = time.monotonic()
      async with client.post(url + '/api/vision_diag/upload_discord', json={'text': 'owned error control', 'filename': 'owned.txt', 'source': 'owned'}) as response:
        result = await response.json()
        elapsed = time.monotonic() - began
        await anyio.to_thread.run_sync(save, root / 'case-result.json', {'status': response.status, 'body': result, 'seconds': elapsed})
        match mode:
          case Mode.HTTP_ERROR:
            assert response.status == 502 and result['discord']['status'] == 429 and len(result['discord']['error']) == 1000
          case Mode.TIMEOUT:
            assert response.status == 502 and not result['ok'] and 19 <= elapsed < 22.5
          case Mode.CLOSED:
            assert response.status == 502 and not result['ok']
          case Mode.SKIP | Mode.ALIAS:
            assert response.status == 200 and result['discord'] == {'configured': False, 'ok': False, 'skipped': True}
          case Mode.INVALID:
            assert response.status == 502 and result['discord']['error'] == 'invalid webhook url'
          case Mode.FALLBACK:
            assert response.status == 200 and result['ok']
          case Mode.STATUS_ERROR:
            assert response.status == 200 and result['ok']
            captures = json.loads(await anyio.Path(root / 'multipart.json').read_text())
            content = json.loads(captures[0][0]['text'])['content']
            await anyio.to_thread.run_sync(save, root / 'status-message.json', {'content': content, 'unknown': '- Vision test: unknown' in content})
            assert '- Vision test: unknown' in content
          case unreachable:
            assert_never(unreachable)
    case unreachable:
      assert_never(unreachable)
  return True
