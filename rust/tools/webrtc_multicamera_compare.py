# /// script
# requires-python = ">=3.12"
# dependencies = ["aiortc==1.14.0", "av==16.1.0", "aiohttp==3.13.3"]
# ///
"""Drive three independent encoded queues through the production WebRTC service."""

import asyncio
from fractions import Fraction
import hashlib
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
import av

from webrtc_test_peer import Client, save, start_service, until

CAMERAS = ('driver', 'road', 'wideRoad')
SERVICES = {camera: f'livestream{camera[0].upper()}{camera[1:]}EncodeData' for camera in CAMERAS}


class MultiClient(Client):
  def __init__(self):
    self.peer = aiortc.RTCPeerConnection(aiortc.RTCConfiguration(iceServers=[]))
    self.channel = self.peer.createDataChannel('data', ordered=True)
    self.messages, self.tracks, self.tasks = [], [], []
    self.packets, self.decoded = {camera: [] for camera in CAMERAS}, {camera: [] for camera in CAMERAS}
    self.channel.on('message', self.messages.append)
    for camera in CAMERAS:
      transceiver = self.peer.addTransceiver('video', direction='recvonly')
      receive = transceiver.receiver._handle_rtp_packet

      async def observed(packet, arrival_time_ms, receive=receive, camera=camera):
        self.packets[camera].append(
          {
            'marker': packet.marker,
            'timestamp': packet.timestamp,
            'mid': packet.extensions.mid,
            'payload_sha256': hashlib.sha256(packet.payload).hexdigest(),
            'payload_bytes': len(packet.payload),
          }
        )
        await receive(packet, arrival_time_ms)

      transceiver.receiver._handle_rtp_packet = observed
    self.peer.on('track', self.track)

  def track(self, track):
    self.tracks.append(track.id)
    camera = track.id.split(':', 1)[0]
    assert camera in CAMERAS

    async def consume():
      while True:
        frame = await track.recv()
        self.decoded[camera].append(
          {'width': frame.width, 'height': frame.height, 'pts': frame.pts, 'rgba_sha256': hashlib.sha256(frame.to_ndarray(format='rgba').tobytes()).hexdigest()}
        )

    self.tasks.append(asyncio.create_task(consume()))


def frames():
  encoded = {}
  for camera_index, camera in enumerate(CAMERAS):
    encoder = av.CodecContext.create('libx264', 'w')
    encoder.width, encoder.height, encoder.pix_fmt = 128, 96, 'yuv420p'
    encoder.time_base, encoder.framerate = Fraction(1, 20), Fraction(20, 1)
    encoder.options = {'preset': 'ultrafast', 'tune': 'zerolatency', 'profile': 'baseline'}
    packets = []
    for index in range(4):
      frame = av.VideoFrame(128, 96, 'yuv420p')
      for plane_index, plane in enumerate(frame.planes):
        plane.update(bytes([48 + camera_index * 48 + index * 8 if plane_index == 0 else 128]) * plane.buffer_size)
      frame.pts = index
      frame.time_base = encoder.time_base
      packets.extend(bytes(packet) for packet in encoder.encode(frame))
    packets.extend(bytes(packet) for packet in encoder.encode(None))
    assert len(packets) == 4
    ids = [100, 101, 104, 110] if camera == 'road' else [7, 49, 13, 99]
    encoded[camera] = [{'frame_id': frame_id, 'header': [], 'data': list(packet)} for frame_id, packet in zip(ids, packets, strict=True)]
  return encoded


def comparison(row):
  streams = {}
  for camera, packets in row['packets'].items():
    first = packets[0]['timestamp']
    streams[camera] = [{**packet, 'timestamp': (packet['timestamp'] - first) & 0xFFFFFFFF} for packet in packets]
  road_markers = [packet['timestamp'] for packet in row['packets']['road'] if packet['marker']]
  return {
    'tracks': [name.split(':', 1)[0] for name in row['tracks']],
    'packets': streams,
    'decoded': row['decoded'],
    'cvf1_ids': [item[0] for item in row['cvf1']],
    'cvf1_matches_road_rtp': row['cvf1'] == [list(item) for item in zip([100, 101, 104, 110], road_markers, strict=True)] if row['cvf1'] else None,
    'peer_before_caller_cleanup': row['peer_before_caller_cleanup'],
    'channel_before_caller_cleanup': row['channel_before_caller_cleanup'],
  }


async def scenario(kind, binary, folder, input_frames):
  from openpilot.cereal import messaging
  from openpilot.system.webrtc import carrot_session, webrtcd

  folder.mkdir()
  mode, profile = kind.split('-')
  carrot = profile == 'carrot'
  os.environ['PARAMS_ROOT'] = str(folder / 'params')
  row, retained = {'kind': kind}, []
  client, runner, process = MultiClient(), None, None
  publisher = messaging.PubMaster(list(SERVICES.values()))
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
      port = int((await asyncio.wait_for(process.stdout.readline(), 3)).decode().rsplit(':', 1)[1])
      row['pid'] = process.pid
    request = {'sdp': await client.offer(), 'cameras': list(CAMERAS), 'client_id': 'owned-multicamera', 'carrot_state': carrot}
    save(folder / 'request.json', request)
    async with ClientSession() as http:
      async with http.post(f'http://127.0.0.1:{port}/stream', json=request) as response:
        answer = await response.json()
        save(folder / 'answer.json', answer)
        assert response.status == 200
      await client.answer(answer)
    assert [name.split(':', 1)[0] for name in client.tracks] == list(CAMERAS)
    for index in range(4):
      # Rotate publication order; every queue has its own bitstream and source IDs.
      for camera in CAMERAS[index % 3 :] + CAMERAS[: index % 3]:
        frame = input_frames[camera][index]
        message = messaging.new_message(SERVICES[camera])
        value = getattr(message, SERVICES[camera])
        value.idx.frameId, value.header, value.data = frame['frame_id'], bytes(frame['header']), bytes(frame['data'])
        publisher.send(SERVICES[camera], message)
      await until(lambda index=index: all(sum(packet['marker'] for packet in packets) == index + 1 for packets in client.packets.values()))
    await until(lambda: all(len(values) == 3 for values in client.decoded.values()))
    if carrot:
      await until(lambda: len([value for value in client.messages if isinstance(value, bytes) and value.startswith(b'CVF1')]) == 4)
    row.update(tracks=client.tracks, packets=client.packets, decoded=client.decoded)
    row['cvf1'] = [list(struct.unpack('>4sII', value)[1:]) for value in client.messages if isinstance(value, bytes) and value.startswith(b'CVF1')]
    if runner is not None:
      await runner.cleanup()
      runner = None
    else:
      process.send_signal(signal.SIGTERM)
      await asyncio.wait_for(process.wait(), 3)
    await until(lambda: client.peer.connectionState == 'closed')
    row.update(peer_before_caller_cleanup=client.peer.connectionState, channel_before_caller_cleanup=client.channel.readyState)
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
      await client.close()
      publisher.sock.clear()
      for session in retained:
        for track in session._video_tracks:
          track._sock = None
      save(folder / 'result.json', row)
  if process is not None:
    assert row['returncode'] == 0, row
  return row


async def main(binary, output):
  output.mkdir()
  previous = {key: os.environ.get(key) for key in ('PARAMS_ROOT', 'OPENPILOT_PREFIX', 'WEBRTC_OWNED_ROOT')}
  os.environ['WEBRTC_OWNED_ROOT'] = str(output.resolve())
  constructor, addresses = aiortc.RTCPeerConnection, aioice.ice.get_host_addresses
  aiortc.RTCPeerConnection = lambda configuration=None: constructor(configuration or aiortc.RTCConfiguration(iceServers=[]))
  aioice.ice.get_host_addresses = lambda use_ipv4, use_ipv6: ['127.0.0.1']
  input_frames = frames()
  save(output / 'input.json', input_frames)
  try:
    with tempfile.TemporaryDirectory(prefix='msgq_rtc240_multi_', dir='/dev/shm') as namespace:
      os.environ['OPENPILOT_PREFIX'] = Path(namespace).name.removeprefix('msgq_')
      rows = [await scenario(kind, binary, output / kind, input_frames) for kind in ('source-standard', 'native-standard', 'source-carrot', 'native-carrot')]
    comparisons = [comparison(row) for row in rows]
    save(output / 'comparison.json', comparisons)
    assert all(value['cvf1_matches_road_rtp'] is True for value in comparisons[2:])
    assert comparisons[0] == comparisons[1], 'standard multicamera mismatch'
    assert comparisons[2] == comparisons[3], 'Carrot multicamera mismatch'
    print(json.dumps({'profiles': 2, 'cameras': list(CAMERAS), 'decoded_per_camera': 3, 'comparison': comparisons}, indent=2))
  finally:
    aiortc.RTCPeerConnection, aioice.ice.get_host_addresses = constructor, addresses
    for key, value in previous.items():
      if value is None:
        os.environ.pop(key, None)
      else:
        os.environ[key] = value


if __name__ == '__main__':
  asyncio.run(main(*(Path(value) for value in sys.argv[1:])))
