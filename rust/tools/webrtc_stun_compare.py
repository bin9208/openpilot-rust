# /// script
# requires-python = ">=3.12"
# dependencies = ["aiortc==1.14.0", "av==16.1.0", "aiohttp==3.13.3", "dnspython==2.8.0"]
# ///
# Run with the pinned source-oracle environment and owned output/IPC roots.
"""Capture actual pending STUN transactions and pair failure on owned peers."""

import asyncio
import json
import os
from pathlib import Path
import signal
import sys
import tempfile
import time

import aioice.ice
import aioice.stun
import aiortc
from aiohttp import ClientSession, web

from webrtc_source_owners import observe_owners
from webrtc_test_peer import save, start_service


async def scenario(mode, binary, folder):
  from openpilot.system.webrtc import webrtcd

  folder.mkdir()
  client = aiortc.RTCPeerConnection(aiortc.RTCConfiguration(iceServers=[]))
  channel = client.createDataChannel('data', ordered=True)
  runner = process = None
  sessions, events, wire, transactions, pairs = [], [], [], [], []
  originals = (
    webrtcd.StreamSession.get_answer,
    aioice.ice.StunProtocol.datagram_received,
    aioice.ice.StunProtocol.send_stun,
    aioice.stun.Transaction.response_received,
    aioice.ice.Connection.check_state,
  )
  started = time.monotonic()
  row = {'mode': mode, 'wire': wire, 'transactions': transactions, 'owner_events': events, 'pairs': pairs}

  async def answer(session):
    sessions.append(session)
    observe_owners(session.stream.peer_connection, events, started)
    return await originals[0](session)

  def received(protocol, data, address):
    try:
      message = aioice.stun.parse_message(data)
    except ValueError:
      return originals[1](protocol, data, address)
    wire.append(
      {
        'phase': 'receive',
        'seconds': time.monotonic() - started,
        'sender': list(address),
        'local': list(protocol.transport.get_extra_info('sockname')),
        'class': message.message_class.name,
        'id': message.transaction_id.hex(),
        'username': message.attributes.get('USERNAME'),
        'hex': data.hex(),
      }
    )
    return originals[1](protocol, data, address)

  def sent(protocol, message, address):
    wire.append(
      {
        'phase': 'send',
        'seconds': time.monotonic() - started,
        'recipient': list(address),
        'local': list(protocol.transport.get_extra_info('sockname')),
        'class': message.message_class.name,
        'id': message.transaction_id.hex(),
        'error': message.attributes.get('ERROR-CODE'),
        'hex': bytes(message).hex(),
      }
    )
    return originals[2](protocol, message, address)

  def accepted(transaction, message, address):
    transactions.append(
      {
        'seconds': time.monotonic() - started,
        'sender': list(address),
        'class': message.message_class.name,
        'id': message.transaction_id.hex(),
        'error': message.attributes.get('ERROR-CODE'),
        'hex': bytes(message).hex(),
      }
    )
    return originals[3](transaction, message, address)

  def pair_state(connection, pair, state):
    pairs.append(
      {
        'seconds': time.monotonic() - started,
        'owner_ufrag': connection.local_username,
        'local': pair.local_addr,
        'remote': pair.remote_addr,
        'before': pair.state.name,
        'after': state.name,
      }
    )
    return originals[4](connection, pair, state)

  webrtcd.StreamSession.get_answer = answer
  aioice.ice.StunProtocol.datagram_received = received
  aioice.ice.StunProtocol.send_stun = sent
  aioice.stun.Transaction.response_received = accepted
  aioice.ice.Connection.check_state = pair_state
  try:
    await client.setLocalDescription(await client.createOffer())
    client.addTransceiver('video', direction='recvonly')
    await client.setLocalDescription(await client.createOffer())
    parsed = aiortc.sdp.SessionDescription.parse(client.localDescription.sdp)
    assert [m.kind for m in parsed.media] == ['application', 'video']
    assert parsed.media[0].ice.usernameFragment != parsed.media[1].ice.usernameFragment
    offered = {'sdp': client.localDescription.sdp, 'cameras': ['road'], 'client_id': 'owned-stun-error'}
    save(folder / 'request.json', offered)
    if mode == 'source':
      webrtcd._carrot_vision_mode = False
      app = web.Application(middlewares=[webrtcd.cors_middleware])
      app['streams'], app['stream_lock'], app['debug'] = {}, asyncio.Lock(), False
      app.on_shutdown.append(webrtcd.on_shutdown)
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
    async with ClientSession() as http:
      async with http.post(f'http://127.0.0.1:{port}/stream', json=offered) as response:
        remote = await response.json()
        save(folder / 'answer.json', {'status': response.status, **remote})
        assert response.status == 200
    await client.setRemoteDescription(aiortc.RTCSessionDescription(**remote))
    await asyncio.sleep(2.3)
    row.update(
      {'client': client.connectionState, 'ice': client.sctp.transport.transport.state, 'dtls': client.sctp.transport.state, 'channel': channel.readyState}
    )
    if sessions:
      connection = sessions[0].stream.peer_connection
      row['source_state'] = connection.connectionState
      row['checklists'] = [
        [{'state': pair.state.name, 'local': pair.local_addr, 'remote': pair.remote_addr} for pair in owner._connection._check_list]
        for owner in connection._RTCPeerConnection__iceTransports
      ]
  finally:
    try:
      if runner is not None:
        await runner.cleanup()
      if process is not None:
        if process.returncode is None:
          process.send_signal(signal.SIGTERM)
          await asyncio.wait_for(process.wait(), 3)
        stdout, stderr = await process.communicate()
        row.update({'returncode': process.returncode, 'stdout': stdout.decode(), 'stderr': stderr.decode()})
    finally:
      await client.close()
      for session in sessions:
        for track in session._video_tracks:
          track._sock = None
      (
        webrtcd.StreamSession.get_answer,
        aioice.ice.StunProtocol.datagram_received,
        aioice.ice.StunProtocol.send_stun,
        aioice.stun.Transaction.response_received,
        aioice.ice.Connection.check_state,
      ) = originals
      save(folder / 'result.json', row)
  return row


async def main(binary, output):
  output.mkdir()
  os.environ['WEBRTC_OWNED_ROOT'] = str(output.resolve())
  constructor, addresses = aiortc.RTCPeerConnection, aioice.ice.get_host_addresses
  aiortc.RTCPeerConnection = lambda configuration=None: constructor(configuration or aiortc.RTCConfiguration(iceServers=[]))
  aioice.ice.get_host_addresses = lambda use_ipv4, use_ipv6: ['127.0.0.1']
  try:
    with tempfile.TemporaryDirectory(prefix='msgq_rtc240_stun_', dir='/dev/shm') as namespace:
      os.environ['OPENPILOT_PREFIX'] = Path(namespace).name.removeprefix('msgq_')
      os.environ['PARAMS_ROOT'] = str(output / 'bootstrap-params')
      source = json.loads(await asyncio.to_thread(Path(sys.argv[3]).read_text)) if len(sys.argv) > 3 else await scenario('source', binary, output / 'source')
      rows = [source, await scenario('native', binary, output / 'native')]
      save(output / 'result.json', rows)
      print(json.dumps(rows, indent=2))
      if len(sys.argv) > 3:
        for field in ['client', 'ice', 'dtls', 'channel']:
          assert rows[0][field] == rows[1][field], (field, rows[0][field], rows[1][field])
  finally:
    aiortc.RTCPeerConnection, aioice.ice.get_host_addresses = constructor, addresses


if __name__ == '__main__':
  asyncio.run(main(*(Path(value) for value in sys.argv[1:3])))
