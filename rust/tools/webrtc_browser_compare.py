# /// script
# requires-python = ">=3.12"
# dependencies = ["aiortc==1.14.0", "av==16.1.0", "aiohttp==3.13.3", "dnspython==2.8.0"]
# ///
# Run with the pinned source-oracle environment and owned output/IPC roots.
import asyncio
import hashlib
import json
import os
from pathlib import Path
import signal
import sys
import tempfile

import aioice.ice
import aioice.mdns
import aiortc
from aiohttp import ClientSession, web
import dns.message

from webrtc_browser_ports import ports
from original_params_binding import load
from webrtc_mdns_source import OwnedTransport, Result, response
from webrtc_test_peer import save, start_service, until


class Resolver(asyncio.DatagramProtocol):
  def __init__(self):
    self.transport, self.source_receiver = None, None
    self.addresses, self.queries = {}, []

  def connection_made(self, transport):
    self.transport = transport

  def datagram_received(self, data, address):
    name = dns.message.from_wire(data).question[0].name.to_text(omit_final_dot=True).lower()
    self.queries.append({'name': name, 'query': data.hex(), 'result': self.addresses[name]})
    self.transport.sendto(response(data, self.addresses[name]), self.source_receiver or address)


async def scenario(mode, binary, output, carrot):
  from openpilot.system.webrtc import webrtcd, carrot_session
  from openpilot.cereal import messaging

  folder = output / mode
  folder.mkdir(exist_ok=False)
  os.environ['PARAMS_ROOT'] = str(folder / 'params')
  assert Path(os.environ['PARAMS_ROOT']).parent == folder and os.environ['OPENPILOT_PREFIX']
  loop = asyncio.get_running_loop()
  resolver = Resolver()
  dns_transport, _ = await loop.create_datagram_endpoint(lambda: resolver, local_addr=('127.0.0.1', 0))
  endpoint = dns_transport.get_extra_info('sockname')
  runner = process = control_runner = browser = None
  retained = []
  original_answer, original_mdns = webrtcd.StreamSession.get_answer, aioice.mdns.create_mdns_protocol
  wire = Result('browser', [], [], [], [])
  row = {'mode': mode, 'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest()}
  publisher = messaging.PubMaster(['livestreamRoadEncodeData'])
  key = None
  if carrot:
    from openpilot.common.params import Params

    params = Params(str(folder / 'params'))
    key = Path(params.get_param_path('CarrotVisionActive'))
    assert key.parent.resolve().is_relative_to((folder / 'params').resolve())
    row['params_path'] = str(key)

  async def owned_protocol():
    tx, _ = await loop.create_datagram_endpoint(asyncio.DatagramProtocol, local_addr=('127.0.0.1', 0))
    protocol = aioice.mdns.MDnsProtocol(OwnedTransport(tx, endpoint, wire))
    rx, _ = await loop.create_datagram_endpoint(lambda: protocol, local_addr=('127.0.0.1', 0))
    resolver.source_receiver = rx.get_extra_info('sockname')
    return protocol

  async def retained_answer(session):
    retained.append(session)
    return await original_answer(session)

  async def prepare(request):
    data = await request.json()
    resolver.addresses, row['socket_ownership'] = ports(data['sdp'], data['browserPid'], data['nodePid'])
    offered = {'sdp': data['sdp'], 'cameras': ['road']}
    if carrot:
      offered.update({'client_id': 'owned-browser', 'carrot_state': True})
    save(folder / 'request.json', offered)
    async with ClientSession() as http:
      async with http.post(f'http://127.0.0.1:{port}/stream', json=offered) as result:
        answer = await result.json()
        save(folder / 'answer.json', {'status': result.status, **answer})
        return web.json_response(answer, status=result.status)

  async def produce(request):
    if key is not None:
      await until(lambda: key.exists() and key.read_bytes() == b'1')
      row['active_params'] = key.read_bytes().hex()
    frames = json.loads(await asyncio.to_thread(Path(os.environ['WEBRTC_BROWSER_FRAMES']).read_text))['frames']
    for frame in frames:
      raw = messaging.new_message('livestreamRoadEncodeData')
      raw.livestreamRoadEncodeData.idx.frameId = frame['frame_id']
      raw.livestreamRoadEncodeData.header = bytes(frame['header'])
      raw.livestreamRoadEncodeData.data = bytes(frame['data'])
      publisher.send('livestreamRoadEncodeData', raw)
      await asyncio.sleep(0.15)
    async with ClientSession() as http:
      async with http.post(f'http://127.0.0.1:{port}/notify', json={'owned_browser': 240}) as result:
        assert result.status == 200
    return web.Response(text='OK')

  async def shutdown(request):
    nonlocal runner
    if runner is not None:
      await runner.cleanup()
      runner = None
    else:
      process.send_signal(signal.SIGTERM)
      await asyncio.wait_for(process.wait(), 3)
    if key is not None:
      await until(lambda: key.exists() and key.read_bytes() == b'0')
      row['stopped_params'] = key.read_bytes().hex()
    return web.Response(text='OK')

  try:
    if mode == 'source':
      aioice.mdns.create_mdns_protocol = owned_protocol
      webrtcd.StreamSession.get_answer = retained_answer
      webrtcd._carrot_vision_mode = carrot
      if carrot:
        webrtcd._carrot_vision_params = params
        webrtcd._carrot_vision_active = None
        webrtcd._set_carrot_vision_active(False)
      app = web.Application(middlewares=[webrtcd.cors_middleware])
      app['streams'], app['stream_lock'], app['debug'] = {}, asyncio.Lock(), False
      if carrot:
        app.cleanup_ctx.append(carrot_session.stream_session_cleanup_context)
      app.on_shutdown.append(carrot_session.on_shutdown if carrot else webrtcd.on_shutdown)
      app.router.add_post('/stream', carrot_session.get_stream if carrot else webrtcd.get_stream)
      app.router.add_post('/notify', webrtcd.post_notify)
      runner = web.AppRunner(app)
      await runner.setup()
      site = web.TCPSite(runner, '127.0.0.1', 0)
      await site.start()
      port = site._server.sockets[0].getsockname()[1]
    else:
      process = await start_service(binary, 'carrot' if carrot else 'standard', folder, ('--mdns', f'{endpoint[0]}:{endpoint[1]}'))
      line = await asyncio.wait_for(process.stdout.readline(), 3)
      port = int(line.decode().rsplit(':', 1)[1])
      row['pid'] = process.pid
    if key is not None:
      await until(lambda: key.exists() and key.read_bytes() == b'0')
      row['initial_params'] = key.read_bytes().hex()
    control = web.Application()
    control.router.add_get(
      '/',
      lambda request: web.Response(text='<!doctype html><title>Owned WebRTC media</title><video muted autoplay playsinline></video>', content_type='text/html'),
    )
    control.router.add_post('/prepare', prepare)
    control.router.add_post('/produce', produce)
    control.router.add_post('/shutdown', shutdown)
    control_runner = web.AppRunner(control)
    await control_runner.setup()
    site = web.TCPSite(control_runner, '127.0.0.1', 0)
    await site.start()
    control_port = site._server.sockets[0].getsockname()[1]
    browser = await asyncio.create_subprocess_exec(
      'node',
      str(Path(__file__).with_name('webrtc_browser_peer.mjs')),
      f'http://127.0.0.1:{control_port}',
      str(folder),
      str(int(carrot)),
      stdout=asyncio.subprocess.PIPE,
      stderr=asyncio.subprocess.PIPE,
    )
    stdout, stderr = await asyncio.wait_for(browser.communicate(), 15)
    row.update({'browser_exit': browser.returncode, 'browser_stdout': stdout.decode(), 'browser_stderr': stderr.decode(), 'queries': resolver.queries})
    assert browser.returncode == 0, row
    row['browser'] = json.loads(stdout)
    return row
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
      if browser is not None and browser.returncode is None:
        browser.kill()
        await browser.wait()
      if control_runner is not None:
        await control_runner.cleanup()
      save(folder / 'result.json', row)
    finally:
      dns_transport.close()
      publisher.sock.clear()
      for session in retained:
        for track in session._video_tracks:
          track._sock = None
      webrtcd.StreamSession.get_answer, aioice.mdns.create_mdns_protocol = original_answer, original_mdns
      webrtcd._carrot_vision_params = None


async def main(binary, output, carrot=False):
  output.mkdir(parents=True, exist_ok=False)
  os.environ['WEBRTC_OWNED_ROOT'] = str(output.resolve())
  constructor, addresses = aiortc.RTCPeerConnection, aioice.ice.get_host_addresses
  aiortc.RTCPeerConnection = lambda configuration=None: constructor(configuration or aiortc.RTCConfiguration(iceServers=[]))
  aioice.ice.get_host_addresses = lambda use_ipv4, use_ipv6: ['127.0.0.1']
  try:
    with tempfile.TemporaryDirectory(prefix='msgq_rtc240_browser_', dir='/dev/shm') as namespace:
      os.environ['OPENPILOT_PREFIX'] = Path(namespace).name.removeprefix('msgq_')
      os.environ['PARAMS_ROOT'] = str(output / 'bootstrap-params')
      if carrot:
        binding = Path(os.environ['WEBRTC_PARAMS_BINDING'])
        load(binding, f'ipc://{output}/logs.sock', output / 'logs')
        save(output / 'binding.json', {'path': str(binding), 'sha256': hashlib.sha256(await asyncio.to_thread(binding.read_bytes)).hexdigest()})
      source = json.loads(await asyncio.to_thread(Path(sys.argv[4]).read_text)) if len(sys.argv) > 4 else await scenario('source', binary, output, carrot)
      rows = [source, await scenario('native', binary, output, carrot)]
      save(output / 'result.json', rows)
      print(json.dumps(rows, indent=2))
  finally:
    aiortc.RTCPeerConnection, aioice.ice.get_host_addresses = constructor, addresses


if __name__ == '__main__':
  asyncio.run(main(*(Path(value) for value in sys.argv[1:3]), carrot=len(sys.argv) > 3 and sys.argv[3] == 'carrot'))
