# /// script
# requires-python = ">=3.12"
# dependencies = ["aiortc==1.14.0", "av==16.1.0", "aiohttp==3.13.3"]
# ///
"""Compare actual Cython and native CarrotVisionActive ownership bytes."""

import asyncio
import hashlib
import json
import os
from pathlib import Path
import signal
import sys
import tempfile

import aioice.ice
import aiortc
from aiohttp import web, ClientSession
from original_params_binding import load
from webrtc_test_peer import Client, start_service, until, save


async def scenario(mode, binary, output):
  from openpilot.common.params import Params
  from openpilot.system.webrtc import webrtcd, carrot_session

  folder = output / mode
  folder.mkdir(exist_ok=False)
  root = folder / 'params'
  os.environ['PARAMS_ROOT'] = str(root)
  assert root.parent == folder and os.environ['OPENPILOT_PREFIX']
  params = Params(str(root))
  assert params.check_key('CarrotVisionActive')
  key = Path(params.get_param_path('CarrotVisionActive'))
  assert key.parent.resolve().is_relative_to(root.resolve())
  client = Client()
  runner = process = app = None
  retained = []
  original = webrtcd.StreamSession.get_answer
  row = {
    'mode': mode,
    'params_root': str(root),
    'key': str(key),
    'prefix': os.environ['OPENPILOT_PREFIX'],
    'original_params_constructor': 'explicit owned root; unchanged Cython/C++ body',
  }

  async def retained_answer(session):
    retained.append(session)
    return await original(session)

  try:
    if mode == 'source':
      webrtcd.StreamSession.get_answer = retained_answer
      webrtcd._carrot_vision_mode = True
      webrtcd._carrot_vision_params = params
      webrtcd._carrot_vision_active = None
      webrtcd._set_carrot_vision_active(False)
      app = web.Application(middlewares=[webrtcd.cors_middleware])
      app['streams'], app['stream_lock'], app['debug'] = {}, asyncio.Lock(), False
      app.cleanup_ctx.append(carrot_session.stream_session_cleanup_context)
      app.on_shutdown.append(carrot_session.on_shutdown)
      app.router.add_post('/stream', carrot_session.get_stream)
      runner = web.AppRunner(app)
      await runner.setup()
      site = web.TCPSite(runner, '127.0.0.1', 0)
      await site.start()
      port = site._server.sockets[0].getsockname()[1]
    else:
      process = await start_service(binary, 'carrot', folder)
      line = await asyncio.wait_for(process.stdout.readline(), 3)
      port = int(line.decode().rsplit(':', 1)[1])
      row.update({'pid': process.pid, 'listener': line.decode().strip()})
    await until(lambda: key.exists() and key.read_bytes() == b'0')
    row['initial'] = (await asyncio.to_thread(key.read_bytes)).hex()
    request = {'sdp': await client.offer(), 'cameras': ['road'], 'client_id': 'owned-params'}
    save(folder / 'request.json', request)
    async with ClientSession() as http:
      async with http.post(f'http://127.0.0.1:{port}/stream', json=request) as response:
        answer = await response.json()
        save(folder / 'answer.json', {'status': response.status, **answer})
        assert response.status == 200
    await client.answer(answer)
    await until(lambda: key.exists() and key.read_bytes() == b'1')
    row.update({'active': (await asyncio.to_thread(key.read_bytes)).hex(), 'connected': client.peer.connectionState, 'channel': client.channel.readyState})
    if runner is not None:
      await runner.cleanup()
      runner = None
    else:
      process.send_signal(signal.SIGTERM)
      await asyncio.wait_for(process.wait(), 3)
    await until(lambda: key.exists() and key.read_bytes() == b'0')
    await until(lambda: client.peer.connectionState == 'closed')
    row.update({'stopped': (await asyncio.to_thread(key.read_bytes)).hex(), 'peer_before_caller_cleanup': client.peer.connectionState})
  finally:
    try:
      if runner is not None:
        await runner.cleanup()
      if process is not None:
        if process.returncode is None:
          process.kill()
          await process.wait()
        stdout, stderr = await process.communicate()
        row.update({'returncode': process.returncode, 'stdout': stdout.decode(), 'stderr': stderr.decode()})
      save(folder / 'result.json', row)
    finally:
      await client.close()
      for session in retained:
        for track in session._video_tracks:
          track._sock = None
      webrtcd.StreamSession.get_answer = original
      webrtcd._carrot_vision_params = None
  return row


async def main(binary, output, binding):
  output.mkdir(parents=True, exist_ok=False)
  with tempfile.TemporaryDirectory(prefix='msgq_rtc240_params_', dir='/dev/shm') as namespace:
    os.environ['OPENPILOT_PREFIX'] = Path(namespace).name.removeprefix('msgq_')
    os.environ['PARAMS_ROOT'] = str(output / 'bootstrap-params')
    os.environ['WEBRTC_OWNED_ROOT'] = str(output.resolve())
    load(binding, f'ipc://{output}/logs.sock', output / 'logs')
    constructor, addresses = aiortc.RTCPeerConnection, aioice.ice.get_host_addresses
    aiortc.RTCPeerConnection = lambda configuration=None: constructor(configuration or aiortc.RTCConfiguration(iceServers=[]))
    aioice.ice.get_host_addresses = lambda use_ipv4, use_ipv6: ['127.0.0.1']
    try:
      rows = [await scenario(mode, binary, output) for mode in ('source', 'native')]
      assert [(row['initial'], row['active'], row['stopped'], row['peer_before_caller_cleanup']) for row in rows] == [('30', '31', '30', 'closed')] * 2
      assert rows[1]['returncode'] == 0, rows[1]
      save(output / 'summary.json', {'binding': str(binding), 'binding_sha256': hashlib.sha256(binding.read_bytes()).hexdigest(), 'rows': rows})
      print(
        json.dumps(
          {
            'passed': [
              {'mode': row['mode'], 'bytes': [row['initial'], row['active'], row['stopped']], 'peer': row['peer_before_caller_cleanup']} for row in rows
            ]
          }
        )
      )
    finally:
      aiortc.RTCPeerConnection, aioice.ice.get_host_addresses = constructor, addresses


if __name__ == '__main__':
  asyncio.run(main(*(Path(value) for value in sys.argv[1:4])))
