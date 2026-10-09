# /// script
# requires-python = ">=3.12"
# dependencies = ["aiortc==1.14.0", "aiohttp==3.13.3"]
# ///
"""Exercise original/native ICE, DTLS and SCTP through an owned loopback NAT."""

import asyncio
import hashlib
import json
import os
from pathlib import Path
import signal
import socket
import sys
import tempfile

import aioice.ice
from aioice import stun
import aiortc
from aiohttp import ClientSession, web

from webrtc_test_peer import Client, save, start_service, until


class Mapping(asyncio.DatagramProtocol):
  """Forward the two owned sides of one server-reflexive UDP mapping."""

  def __init__(self, base):
    self.base, self.client, self.transport = base, None, None
    self.to_base = self.to_client = 0

  def connection_made(self, transport):
    self.transport = transport

  def datagram_received(self, data, address):
    if address == self.base:
      if self.client is not None:
        self.to_client += 1
        self.transport.sendto(data, self.client)
    else:
      self.client = address
      self.to_base += 1
      self.transport.sendto(data, self.base)


class Nat:
  """Own the STUN recipient, dynamic mappings and observed forwarding counters."""

  def __init__(self, recipient):
    self.recipient, self.mappings, self.blocked_direct = recipient, {}, 0
    self.requests = []

  def aliases(self):
    return {transport.get_extra_info('sockname') for transport, _ in self.mappings.values()}

  async def run(self):
    loop = asyncio.get_running_loop()
    while True:
      raw, base = await loop.sock_recvfrom(self.recipient, 65536)
      assert base[0] == '127.0.0.1'
      request = stun.parse_message(raw)
      assert request.message_method == stun.Method.BINDING and request.message_class == stun.Class.REQUEST
      self.requests.append({'base': base, 'wire': raw.hex()})
      if base not in self.mappings:
        mapping = Mapping(base)
        transport, _ = await loop.create_datagram_endpoint(lambda mapping=mapping: mapping, local_addr=('127.0.0.1', 0))
        self.mappings[base] = transport, mapping
      transport, _ = self.mappings[base]
      reply = stun.Message(stun.Method.BINDING, stun.Class.RESPONSE, transaction_id=request.transaction_id)
      reply.attributes['XOR-MAPPED-ADDRESS'] = transport.get_extra_info('sockname')
      await loop.sock_sendto(self.recipient, bytes(reply), base)

  def filter_client(self, client):
    connections = {transceiver.receiver.transport.transport._connection for transceiver in client.peer.getTransceivers()}
    connections.add(client.peer.sctp.transport.transport._connection)
    for connection in connections:
      for protocol in connection._protocols:
        received = protocol.datagram_received

        def filtered(data, address, received=received):
          if address in self.aliases():
            received(data, address)
          else:
            self.blocked_direct += 1

        protocol.datagram_received = filtered

  def close(self):
    for transport, _ in self.mappings.values():
      transport.close()


async def scenario(mode, binary, folder, nat):
  from openpilot.system.webrtc import webrtcd

  await asyncio.to_thread(folder.mkdir, exist_ok=False)
  os.environ['PARAMS_ROOT'] = str(folder / 'params')
  assert Path(os.environ['PARAMS_ROOT']).parent == folder and os.environ['OPENPILOT_PREFIX']
  client = Client()
  process = runner = None
  retained = []
  original = webrtcd.StreamSession.get_answer
  row = {'mode': mode, 'binary_sha256': hashlib.sha256(await asyncio.to_thread(binary.read_bytes)).hexdigest()}

  async def retained_answer(session):
    retained.append(session)
    return await original(session)

  try:
    if mode == 'source':
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
      host, port = nat.recipient.getsockname()
      process = await start_service(binary, 'standard', folder, (f'{host}:{port}',))
      line = await asyncio.wait_for(process.stdout.readline(), 3)
      port = int(line.decode().rsplit(':', 1)[1])
      row['pid'] = process.pid
    request = {'sdp': await client.offer(), 'cameras': ['road']}
    nat.filter_client(client)
    save(folder / 'request.json', request)
    async with ClientSession() as http:
      async with http.post(f'http://127.0.0.1:{port}/stream', json=request) as result:
        answer = await result.json()
        save(folder / 'answer.json', {'status': result.status, **answer})
        assert result.status == 200 and ' typ srflx ' in answer['sdp']
      answer['sdp'] = '\r\n'.join(line for line in answer['sdp'].splitlines() if not line.startswith('a=candidate:') or ' typ srflx ' in line) + '\r\n'
      save(folder / 'client-answer.json', answer)
      await client.answer(answer)
      connection = client.peer.getTransceivers()[0].receiver.transport.transport._connection
      remote = connection._nominated[1].remote_candidate
      row.update({'selected_remote_type': remote.type, 'selected_remote': [remote.host, remote.port]})
      assert remote.type == 'srflx' and (remote.host, remote.port) in nat.aliases()
      payload = {'owned_srflx': 240}
      async with http.post(f'http://127.0.0.1:{port}/notify', json=payload) as result:
        assert result.status == 200
      await until(lambda: json.dumps(payload) in client.messages)
      row.update({'connected': client.peer.connectionState, 'channel': client.channel.readyState, 'notify': json.dumps(payload)})
    if runner is not None:
      await runner.cleanup()
      runner = None
    else:
      process.send_signal(signal.SIGTERM)
      await asyncio.wait_for(process.wait(), 3)
    await until(lambda: client.peer.connectionState == 'closed')
    row.update(
      {
        'peer_before_caller_cleanup': client.peer.connectionState,
        'blocked_direct': nat.blocked_direct,
        'requests': nat.requests,
        'mappings': [
          {'base': base, 'alias': transport.get_extra_info('sockname'), 'to_base': mapping.to_base, 'to_client': mapping.to_client}
          for base, (transport, mapping) in nat.mappings.items()
        ],
      }
    )
    assert all(mapping['to_base'] and mapping['to_client'] for mapping in row['mappings'])
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
      await client.close()
      for session in retained:
        for track in session._video_tracks:
          track._sock = None
      webrtcd.StreamSession.get_answer = original


async def main(binary, output):
  await asyncio.to_thread(output.mkdir, parents=True, exist_ok=False)
  os.environ['WEBRTC_OWNED_ROOT'] = str(await asyncio.to_thread(output.resolve))
  constructor, addresses = aiortc.RTCPeerConnection, aioice.ice.get_host_addresses
  aioice.ice.get_host_addresses = lambda use_ipv4, use_ipv6: ['127.0.0.1']
  rows = []
  try:
    with tempfile.TemporaryDirectory(prefix='msgq_rtc240_nat_', dir='/dev/shm') as namespace:
      os.environ['OPENPILOT_PREFIX'] = Path(namespace).name.removeprefix('msgq_')
      for mode in ('source', 'native'):
        with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as recipient:
          recipient.bind(('127.0.0.1', 0))
          recipient.setblocking(False)
          host, port = recipient.getsockname()
          aiortc.RTCPeerConnection = lambda configuration=None, host=host, port=port: constructor(
            configuration or aiortc.RTCConfiguration(iceServers=[aiortc.RTCIceServer(f'stun:{host}:{port}')])
          )
          nat = Nat(recipient)
          try:
            async with asyncio.TaskGroup() as tasks:
              worker = tasks.create_task(nat.run())
              try:
                rows.append(await scenario(mode, binary, output / mode, nat))
              finally:
                worker.cancel()
          finally:
            nat.close()
      save(output / 'result.json', rows)
      print(json.dumps(rows, indent=2))
  finally:
    aiortc.RTCPeerConnection, aioice.ice.get_host_addresses = constructor, addresses


if __name__ == '__main__':
  asyncio.run(main(*(Path(value) for value in sys.argv[1:3])))
