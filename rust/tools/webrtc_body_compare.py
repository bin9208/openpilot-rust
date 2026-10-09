# /// script
# requires-python = ">=3.12"
# dependencies = ["aiortc==1.14.0", "av==16.1.0", "aiohttp==3.13.3", "dnspython==2.8.0"]
# ///
# Run with the pinned source-oracle environment and owned output/IPC roots.
"""Observe original/native HTTP request text decoding at the real route."""

import asyncio
import json
import os
from pathlib import Path
import signal
import sys
import tempfile

import aiortc
from aiohttp import ClientSession, web

from webrtc_test_peer import save, start_service, until


async def scenario(mode, binary, folder):
  from openpilot.system.webrtc import webrtcd

  folder.mkdir()
  os.environ['PARAMS_ROOT'] = str(folder / 'params')
  runner = process = None
  row = {'mode': mode, 'cases': []}
  client = aiortc.RTCPeerConnection(aiortc.RTCConfiguration(iceServers=[]))
  channel = client.createDataChannel('data', ordered=True)
  messages = []
  channel.on('message', messages.append)
  try:
    if mode == 'source':
      webrtcd._carrot_vision_mode = False
      app = web.Application(middlewares=[webrtcd.cors_middleware])
      app['streams'], app['stream_lock'], app['debug'] = {}, asyncio.Lock(), False
      app.on_shutdown.append(webrtcd.on_shutdown)
      app.router.add_post('/notify', webrtcd.post_notify)
      app.router.add_post('/stream', webrtcd.get_stream)
      runner = web.AppRunner(app)
      await runner.setup()
      site = web.TCPSite(runner, '127.0.0.1', 0)
      await site.start()
      port = site._server.sockets[0].getsockname()[1]
    else:
      process = await start_service(binary, 'standard', folder)
      line = await asyncio.wait_for(process.stdout.readline(), 3)
      port = int(line.decode().rsplit(':', 1)[1])
      row['pid'] = process.pid
    text = json.dumps({'s': 'café 한글'}, ensure_ascii=False)
    cases = [(name, text.encode(name), f'application/json; charset="{name}"') for name in ['utf-8', 'utf-8-sig', 'utf-16', 'utf-16-be', 'utf-32']]
    cases.append(('euc-kr', '{"s":"한글"}'.encode('euc-kr'), 'application/json; charset=euc-kr'))
    cases += [(name, '{"s":"café"}'.encode(name), f'application/json; charset={name}') for name in ['latin-1', 'windows-1252']]
    cases += [
      ('bom-default', text.encode('utf-8-sig'), 'application/json'),
      ('invalid-utf8', b'{"s":"\xff"}', 'application/json'),
      ('unknown-charset', b'{}', 'application/json; charset=does-not-exist'),
    ]
    cases += [
      ('cp1252-undefined', b'{"s":"\x81"}', 'application/json; charset=windows-1252'),
      ('euc-kr-extension', b'{"s":"\x81\x41"}', 'application/json; charset=euc-kr'),
      ('euc-kr-makeup', '{"s":"힣"}'.encode('euc-kr'), 'application/json; charset=euc-kr'),
    ]
    async with ClientSession() as http:
      await client.setLocalDescription(await client.createOffer())
      async with http.post(f'http://127.0.0.1:{port}/stream', json={'sdp': client.localDescription.sdp, 'cameras': []}) as response:
        answer = await response.json()
        assert response.status == 200
      await client.setRemoteDescription(aiortc.RTCSessionDescription(**answer))
      await until(lambda: channel.readyState == 'open')
      for name, data, content_type in cases:
        count = len(messages)
        async with http.post(f'http://127.0.0.1:{port}/notify', data=data, headers={'Content-Type': content_type}) as response:
          observed = {
            'name': name,
            'path': '/notify',
            'wire': data.hex(),
            'content_type': content_type,
            'status': response.status,
            'body': await response.text(),
            'cors': response.headers.get('Access-Control-Allow-Origin'),
          }
          if response.status == 200:
            await until(lambda count=count: len(messages) > count)
            observed['delivered'] = json.loads(messages[-1])
          row['cases'].append(observed)
      async with http.post(f'http://127.0.0.1:{port}/stream', data=b'{"s":"\xff"}', headers={'Content-Type': 'application/json'}) as response:
        row['cases'].append(
          {
            'name': 'invalid-utf8-stream',
            'path': '/stream',
            'status': response.status,
            'body': await response.text(),
            'cors': response.headers.get('Access-Control-Allow-Origin'),
          }
        )
  finally:
    if runner is not None:
      await runner.cleanup()
    if process is not None:
      if process.returncode is None:
        process.send_signal(signal.SIGTERM)
      stdout, stderr = await asyncio.wait_for(process.communicate(), 3)
      row.update({'returncode': process.returncode, 'stdout': stdout.decode(), 'stderr': stderr.decode()})
    await client.close()
    save(folder / 'result.json', row)
  return row


async def main(binary, output):
  output.mkdir()
  os.environ['WEBRTC_OWNED_ROOT'] = str(output.resolve())
  constructor = aiortc.RTCPeerConnection
  aiortc.RTCPeerConnection = lambda configuration=None: constructor(configuration or aiortc.RTCConfiguration(iceServers=[]))
  try:
    with tempfile.TemporaryDirectory(prefix='msgq_rtc240_body_', dir='/dev/shm') as namespace:
      os.environ['OPENPILOT_PREFIX'] = Path(namespace).name.removeprefix('msgq_')
      source = json.loads(await asyncio.to_thread(Path(sys.argv[3]).read_text)) if len(sys.argv) > 3 else await scenario('source', binary, output / 'source')
      rows = [source, await scenario('native', binary, output / 'native')]
      mismatch = [(a, b) for a, b in zip(rows[0]['cases'], rows[1]['cases'], strict=True) if a != b]
      save(output / 'result.json', {'rows': rows, 'mismatches': mismatch})
      print(json.dumps({'cases': len(rows[0]['cases']), 'mismatches': mismatch}, indent=2))
      if len(sys.argv) > 3:
        assert not mismatch, mismatch
  finally:
    aiortc.RTCPeerConnection = constructor


if __name__ == '__main__':
  asyncio.run(main(*(Path(value) for value in sys.argv[1:3])))
