# /// script
# requires-python = ">=3.12"
# dependencies = ["aiortc==1.14.0", "av==16.1.0", "aiohttp==3.13.3"]
# ///
"""Observe Python identifier conversion through real session ownership."""

import asyncio
import json
import os
from pathlib import Path
import signal
import sys
import tempfile

import aioice.ice
import aiortc
from aiohttp import ClientSession, web

from webrtc_test_peer import Client, save, start_service, until


async def scenario(kind, binary, folder, case):
  from openpilot.system.webrtc import carrot_session, webrtcd

  folder.mkdir()
  mode, profile = kind.split('-')
  carrot = profile == 'carrot'
  os.environ['PARAMS_ROOT'] = str(folder / 'params')
  row = {'kind': kind, 'case': case, 'requests': []}
  clients, retained = [], []
  runner = process = app = None
  original = webrtcd.StreamSession.get_answer
  previous = webrtcd._carrot_vision_mode, webrtcd._carrot_vision_params

  async def retain(session):
    retained.append(session)
    return await original(session)

  try:
    if mode == 'source':
      webrtcd._carrot_vision_mode, webrtcd._carrot_vision_params = carrot, None
      webrtcd.StreamSession.get_answer = retain
      app = web.Application(middlewares=[webrtcd.cors_middleware])
      app['streams'], app['stream_lock'], app['debug'] = {}, asyncio.Lock(), False
      app.on_shutdown.append(carrot_session.on_shutdown if carrot else webrtcd.on_shutdown)
      if carrot:
        app.cleanup_ctx.append(carrot_session.stream_session_cleanup_context)
      app.router.add_post('/stream', carrot_session.get_stream if carrot else webrtcd.get_stream)
      runner = web.AppRunner(app)
      await runner.setup()
      site = web.TCPSite(runner, '127.0.0.1', 0)
      await site.start()
      port = site._server.sockets[0].getsockname()[1]
    else:
      process = await start_service(binary, profile, folder)
      line = await asyncio.wait_for(process.stdout.readline(), 3)
      port = int(line.decode().rsplit(':', 1)[1])
      row['pid'] = process.pid
    compound = {'key': ["b'c", True, None, '\u200b', '\ud800']}
    values = [compound, str(compound)] if case == 'compound' else ['\ud800', '\ud801', '\ud800']
    async with ClientSession() as http:
      for index, value in enumerate(values):
        client = Client()
        clients.append(client)
        body = {'sdp': await client.offer(), 'cameras': ['road'], 'client_id': value}
        if carrot and case == 'compound':
          body.update(client_id='fallback-client', device_id=value)
        save(folder / f'request-{index}.json', body)
        async with http.post(f'http://127.0.0.1:{port}/stream', json=body) as response:
          text = await response.text()
          observed = {'status': response.status, 'cors': response.headers.get('Access-Control-Allow-Origin')}
          row['requests'].append(observed)
          if response.status != 200:
            observed['body'] = text
            break
          answer = json.loads(text)
        await client.answer(answer)
        if index > 0 and (case == 'compound' or carrot or index == 2):
          replaced = clients[index - 1] if carrot else clients[0]
          await until(lambda replaced=replaced: replaced.peer.connectionState == 'closed')
        observed['peer_states_before_caller_cleanup'] = [peer.peer.connectionState for peer in clients]
        observed['channel_states_before_caller_cleanup'] = [peer.channel.readyState for peer in clients]
        if app is not None:
          observed['source_keys'] = [session.client_key for session in app['streams'].values()]
    row['complete_sequence'] = len(row['requests']) == len(values) and all(item['status'] == 200 for item in row['requests'])
  finally:
    try:
      if runner is not None:
        await runner.cleanup()
      if process is not None:
        if process.returncode is None:
          process.send_signal(signal.SIGTERM)
        stdout, stderr = await asyncio.wait_for(process.communicate(), 3)
        row.update(returncode=process.returncode, stdout=stdout.decode(), stderr=stderr.decode())
    finally:
      webrtcd.StreamSession.get_answer = original
      webrtcd._carrot_vision_mode, webrtcd._carrot_vision_params = previous
      await asyncio.gather(*(client.close() for client in clients))
      for session in retained:
        for track in session._video_tracks:
          track._sock = None
      save(folder / 'result.json', row)
  return row


async def main(binary, output, reference=None):
  output.mkdir()
  constructor, addresses = aiortc.RTCPeerConnection, aioice.ice.get_host_addresses
  previous = {key: os.environ.get(key) for key in ('PARAMS_ROOT', 'OPENPILOT_PREFIX', 'WEBRTC_OWNED_ROOT')}
  os.environ['WEBRTC_OWNED_ROOT'] = str(output.resolve())
  aiortc.RTCPeerConnection = lambda configuration=None: constructor(configuration or aiortc.RTCConfiguration(iceServers=[]))
  aioice.ice.get_host_addresses = lambda use_ipv4, use_ipv6: ['127.0.0.1']
  sources, natives = [], []
  try:
    if reference is not None:
      sources = json.loads(await asyncio.to_thread(reference.read_text))['source']
    with tempfile.TemporaryDirectory(prefix='msgq_rtc240_ids_', dir='/dev/shm') as namespace:
      os.environ['OPENPILOT_PREFIX'] = Path(namespace).name.removeprefix('msgq_')
      for profile in ('standard', 'carrot'):
        for case in ('compound', 'surrogates'):
          if reference is None:
            sources.append(await scenario(f'source-{profile}', binary, output / f'source-{profile}-{case}', case))
          natives.append(await scenario(f'native-{profile}', binary, output / f'native-{profile}-{case}', case))

    def comparable(row):
      return [{key: value for key, value in item.items() if key != 'source_keys'} for item in row['requests']]

    mismatches = [
      {'case': source['kind'] + '/' + source['case'], 'source': comparable(source), 'native': comparable(native)}
      for source, native in zip(sources, natives, strict=True)
      if comparable(source) != comparable(native)
    ]
    save(output / 'result.json', {'source': sources, 'native': natives, 'mismatches': mismatches})
    print(json.dumps({'sequences': len(sources), 'mismatches': mismatches}, indent=2))
    assert not mismatches, mismatches
  finally:
    aiortc.RTCPeerConnection, aioice.ice.get_host_addresses = constructor, addresses
    for key, value in previous.items():
      if value is None:
        os.environ.pop(key, None)
      else:
        os.environ[key] = value


if __name__ == '__main__':
  asyncio.run(main(*(Path(value) for value in sys.argv[1:])))
