# /// script
# requires-python = ">=3.12"
# dependencies = ["aiortc==1.14.0", "av==16.1.0", "aiohttp==3.13.3"]
# ///
"""Observe original FIR media-SSRC routing through actual encrypted feedback."""

import asyncio
import json
import os
from pathlib import Path
import signal
import struct
import sys
import tempfile

import aioice.ice
import aiortc
from aiohttp import ClientSession, web
from aiortc.codecs.vpx import VpxPayloadDescriptor
from aiortc.rtp import RtcpPsfbPacket, RtcpRrPacket

from webrtc_test_peer import Client, save, start_service, until


def keyframe(packet):
  descriptor, payload = VpxPayloadDescriptor.parse(bytes(packet['payload']))
  return descriptor.partition_start and bool(payload) and payload[0] & 1 == 0


async def scenario(mode, binary, folder, names):
  from openpilot.system.webrtc import webrtcd

  folder.mkdir()
  os.environ['PARAMS_ROOT'] = str(folder / 'params')
  client, runner, process = Client(), None, None
  row = {'mode': mode, 'cases': []}
  try:
    if mode == 'source':
      app = web.Application(middlewares=[webrtcd.cors_middleware])
      app['streams'], app['stream_lock'], app['debug'] = {}, asyncio.Lock(), True
      app.on_shutdown.append(webrtcd.on_shutdown)
      app.router.add_post('/stream', webrtcd.get_stream)
      runner = web.AppRunner(app)
      await runner.setup()
      site = web.TCPSite(runner, '127.0.0.1', 0)
      await site.start()
      port = site._server.sockets[0].getsockname()[1]
    else:
      process = await start_service(binary, 'standard-debug', folder)
      port = int((await asyncio.wait_for(process.stdout.readline(), 3)).decode().rsplit(':', 1)[1])
      row['pid'] = process.pid
    request = {'sdp': await client.offer(), 'cameras': ['road']}
    save(folder / 'request.json', request)
    async with ClientSession() as http:
      async with http.post(f'http://127.0.0.1:{port}/stream', json=request) as response:
        answer = await response.json()
        assert response.status == 200
    save(folder / 'answer.json', answer)
    await client.answer(answer)
    await until(lambda: len(client.decoded) >= 5)
    ssrc = client.packets[0]['ssrc']
    receiver = client.peer.getTransceivers()[0].receiver
    cases = [
      ('matching-media-and-entry', ssrc, ssrc),
      ('zero-media-matching-entry', 0, ssrc),
      ('matching-media-wrong-entry', ssrc, ssrc ^ 1),
      ('unroutable-rr-before-fir', ssrc, ssrc),
      ('fir-before-unroutable-rr', ssrc, ssrc),
    ]
    for sequence, (name, media, entry) in enumerate(cases, 1):
      if names and name not in names:
        continue
      before = len(client.packets)
      markers = sum(packet['marker'] for packet in client.packets)
      feedback = RtcpPsfbPacket(fmt=4, ssrc=240, media_ssrc=media, fci=struct.pack('!IB3x', entry, sequence))
      raw = bytes(feedback)
      if name == 'unroutable-rr-before-fir':
        raw = bytes(RtcpRrPacket(ssrc=240, reports=[])) + raw
      elif name == 'fir-before-unroutable-rr':
        raw += bytes(RtcpRrPacket(ssrc=240, reports=[]))
      await receiver.transport._send_rtp(raw)
      await until(lambda markers=markers: sum(packet['marker'] for packet in client.packets) >= markers + 8)
      packets = client.packets[before:]
      row['cases'].append(
        {
          'name': name,
          'feedback': raw.hex(),
          'media_matches': media == ssrc,
          'entry_matches': entry == ssrc,
          'keyframe_observed': any(keyframe(packet) for packet in packets),
          'packets_after_feedback': packets,
        }
      )
    if runner is not None:
      await runner.cleanup()
      runner = None
    else:
      process.send_signal(signal.SIGTERM)
      await asyncio.wait_for(process.wait(), 3)
    await until(lambda: client.peer.connectionState == 'closed')
    row['peer_before_caller_cleanup'] = client.peer.connectionState
  finally:
    try:
      if runner is not None:
        await runner.cleanup()
      if process is not None:
        if process.returncode is None:
          process.send_signal(signal.SIGTERM)
        stdout, stderr = await asyncio.wait_for(process.communicate(), 3)
        row.update(returncode=process.returncode, stdout=stdout.decode(), stderr=stderr.decode())
      save(folder / 'result.json', row)
    finally:
      await client.close()
  if process is not None:
    assert row['returncode'] == 0, row
  return row


async def main(binary, output):
  output.mkdir()
  constructor, addresses = aiortc.RTCPeerConnection, aioice.ice.get_host_addresses
  previous = {key: os.environ.get(key) for key in ('PARAMS_ROOT', 'OPENPILOT_PREFIX', 'WEBRTC_OWNED_ROOT')}
  os.environ['WEBRTC_OWNED_ROOT'] = str(output.resolve())
  aiortc.RTCPeerConnection = lambda configuration=None: constructor(configuration or aiortc.RTCConfiguration(iceServers=[]))
  aioice.ice.get_host_addresses = lambda use_ipv4, use_ipv6: ['127.0.0.1']
  arguments = sys.argv[3:]
  reference = Path(arguments[1]) if arguments and arguments[0] == '--native' else None
  names = arguments[2:] if reference else arguments
  try:
    with tempfile.TemporaryDirectory(prefix='msgq_rtc240_fir_', dir='/dev/shm') as namespace:
      os.environ['OPENPILOT_PREFIX'] = Path(namespace).name.removeprefix('msgq_')
      source = json.loads(await asyncio.to_thread(reference.read_text)) if reference else await scenario('source', binary, output / 'source', names)
      rows = [source, await scenario('native', binary, output / 'native', names)]
    comparison = [[{key: value for key, value in item.items() if key not in ('feedback', 'packets_after_feedback')} for item in row['cases']] for row in rows]
    save(output / 'comparison.json', comparison)
    print(json.dumps(comparison, indent=2))
    assert comparison[0] == comparison[1], comparison
  finally:
    aiortc.RTCPeerConnection, aioice.ice.get_host_addresses = constructor, addresses
    for key, value in previous.items():
      if value is None:
        os.environ.pop(key, None)
      else:
        os.environ[key] = value


if __name__ == '__main__':
  asyncio.run(main(*(Path(value) for value in sys.argv[1:3])))
