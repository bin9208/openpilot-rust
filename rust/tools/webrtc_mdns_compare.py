# /// script
# requires-python = ">=3.12"
# dependencies = ["aiortc==1.14.0", "aiohttp==3.13.3"]
# ///
"""Exercise an actual mDNS offer through original and native HTTP sessions."""

import asyncio
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import sys
import tempfile
import time

import aioice.ice
import aioice.mdns
import aiortc
from aiohttp import ClientSession, web

from webrtc_mdns_source import OwnedTransport, Result, response
from webrtc_test_peer import Client, save, start_service, until


class Recipient(asyncio.DatagramProtocol):
  """Answer only synthetic queries on the task's owned loopback UDP socket."""

  def __init__(self, answer=True):
    self.transport = None
    self.source_receiver = None
    self.queries = []
    self.answer = answer

  def connection_made(self, transport):
    self.transport = transport

  def datagram_received(self, data, address):
    self.queries.append(data.hex())
    if self.answer:
      self.transport.sendto(response(data, "127.0.0.1"), self.source_receiver or address)


async def scenario(mode, binary, output, kind):
  from openpilot.system.webrtc import webrtcd

  folder = output / f'{kind}-{mode}'
  await asyncio.to_thread(folder.mkdir, exist_ok=False)
  os.environ['PARAMS_ROOT'] = str(folder / 'params')
  assert Path(os.environ['PARAMS_ROOT']).parent == folder and os.environ['OPENPILOT_PREFIX']
  loop = asyncio.get_running_loop()
  recipient = Recipient(kind != 'unresolved')
  transport, _ = await loop.create_datagram_endpoint(lambda: recipient, local_addr=('127.0.0.1', 0))
  endpoint = transport.get_extra_info('sockname')
  client = Client()
  process = runner = None
  retained = []
  original_answer = webrtcd.StreamSession.get_answer
  original_mdns = aioice.mdns.create_mdns_protocol
  source_wire = Result('http', [], [], [], [])
  row = {'mode': mode, 'case': kind, 'owned_recipient': endpoint, 'binary_sha256': hashlib.sha256(await asyncio.to_thread(binary.read_bytes)).hexdigest()}

  async def owned_protocol():
    tx, _ = await loop.create_datagram_endpoint(asyncio.DatagramProtocol, local_addr=('127.0.0.1', 0))
    protocol = aioice.mdns.MDnsProtocol(OwnedTransport(tx, endpoint, source_wire))
    rx, _ = await loop.create_datagram_endpoint(lambda: protocol, local_addr=('127.0.0.1', 0))
    recipient.source_receiver = rx.get_extra_info('sockname')
    return protocol

  async def retained_answer(session):
    retained.append(session)
    return await original_answer(session)

  try:
    if mode == 'source':
      aioice.mdns.create_mdns_protocol = owned_protocol
      webrtcd.StreamSession.get_answer = retained_answer
      webrtcd._carrot_vision_mode = False
      app = web.Application(middlewares=[webrtcd.cors_middleware])
      app['streams'], app['stream_lock'], app['debug'] = {}, asyncio.Lock(), False
      app.on_shutdown.append(webrtcd.on_shutdown)
      app.router.add_post('/stream', webrtcd.get_stream)
      app.router.add_post('/notify', webrtcd.post_notify)
      runner = web.AppRunner(app)
      await runner.setup()
      site = web.TCPSite(runner, '127.0.0.1', 0)
      await site.start()
      port = site._server.sockets[0].getsockname()[1]
    else:
      process = await start_service(binary, 'standard', folder, ('--mdns', f'{endpoint[0]}:{endpoint[1]}'))
      line = await asyncio.wait_for(process.stdout.readline(), 3)
      port = int(line.decode().rsplit(':', 1)[1])
      row['pid'] = process.pid
    offered = await client.offer()
    sdp, replacements = re.subn(r'(?m)^(a=candidate:\S+ \d+ \S+ \d+ )127\.0\.0\.1( )', r'\g<1>OwNeD.local\2', offered)
    assert replacements > 0
    if kind in ('duplicate', 'unresolved'):
      first = next(line for line in sdp.splitlines() if line.startswith('a=candidate:'))
      duplicate = first.replace(first.split()[0], first.split()[0] + 'ownedcopy', 1)
      if kind == 'unresolved':
        duplicated = first.replace('OwNeD.local', 'Missing1.local') + '\r\n' + duplicate.replace('OwNeD.local', 'Missing2.local')
      else:
        duplicated = first + '\r\n' + duplicate
      sdp = sdp.replace(first, duplicated, 1)
    request = {'sdp': sdp, 'cameras': ['road']}
    save(folder / 'request.json', request)
    async with ClientSession() as http:
      started = time.monotonic()
      async with http.post(f'http://127.0.0.1:{port}/stream', json=request) as result:
        answer = await result.json()
        row['answer_seconds'] = time.monotonic() - started
        save(folder / 'answer.json', {'status': result.status, **answer})
        assert result.status == 200
      payload = {'owned_mdns': 240}
      if kind != 'unresolved':
        await client.answer(answer)
        assert client.peer.connectionState == 'connected' and client.channel.readyState == 'open'
        async with http.post(f'http://127.0.0.1:{port}/notify', json=payload) as result:
          assert result.status == 200
        await until(lambda: json.dumps(payload) in client.messages)
    row.update(
      {
        'connected': client.peer.connectionState,
        'channel': client.channel.readyState,
        'notify': json.dumps(payload) if kind != 'unresolved' else None,
        'queries': recipient.queries,
        'source_query_destinations': source_wire.destinations,
        'replacements': replacements,
      }
    )
    if runner is not None:
      await runner.cleanup()
      runner = None
    else:
      process.send_signal(signal.SIGTERM)
      await asyncio.wait_for(process.wait(), 3)
    if kind != 'unresolved':
      await until(lambda: client.peer.connectionState == 'closed')
    row['peer_before_caller_cleanup'] = client.peer.connectionState
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
      save(folder / 'result.json', row)
    finally:
      transport.close()
      await client.close()
      for session in retained:
        for track in session._video_tracks:
          track._sock = None
      webrtcd.StreamSession.get_answer = original_answer
      aioice.mdns.create_mdns_protocol = original_mdns


async def main(binary, output, kinds):
  await asyncio.to_thread(output.mkdir, parents=True, exist_ok=False)
  constructor, addresses = aiortc.RTCPeerConnection, aioice.ice.get_host_addresses
  aiortc.RTCPeerConnection = lambda configuration=None: constructor(configuration or aiortc.RTCConfiguration(iceServers=[]))
  aioice.ice.get_host_addresses = lambda use_ipv4, use_ipv6: ['127.0.0.1']
  os.environ['WEBRTC_OWNED_ROOT'] = str(await asyncio.to_thread(output.resolve))
  try:
    with tempfile.TemporaryDirectory(prefix='msgq_rtc240_mdns_', dir='/dev/shm') as namespace:
      os.environ['OPENPILOT_PREFIX'] = Path(namespace).name.removeprefix('msgq_')
      rows = [await scenario(mode, binary, output, kind) for kind in kinds for mode in ('source', 'native')]
      save(output / 'result.json', rows)
      print(json.dumps(rows, indent=2))
      for source, native in zip(rows[::2], rows[1::2], strict=True):
        assert sorted(source['queries']) == sorted(native['queries'])
        assert native['answer_seconds'] < source['answer_seconds'] + 0.5, (source['answer_seconds'], native['answer_seconds'])
        assert len(native['queries']) == (2 if source['case'] == 'unresolved' else 1)
  finally:
    aiortc.RTCPeerConnection, aioice.ice.get_host_addresses = constructor, addresses


if __name__ == '__main__':
  asyncio.run(main(*(Path(value) for value in sys.argv[1:3]), sys.argv[3:] or ['a']))
