import asyncio
from dataclasses import asdict
import json
import os
from pathlib import Path

import aiortc


def save(path, value):
  path.write_text(json.dumps(value, indent=2) + "\n")


async def start_service(binary, mode, folder, options=()):
  owned = await asyncio.to_thread(folder.resolve, strict=True)
  evidence = await asyncio.to_thread(Path(os.environ['WEBRTC_OWNED_ROOT']).resolve, strict=True)
  assert owned.is_relative_to(evidence) and owned != evidence
  environment = dict(os.environ, PARAMS_ROOT=str(owned / 'params'))
  assert Path(environment['PARAMS_ROOT']).parent == owned
  assert environment.get('OPENPILOT_PREFIX'), 'owned IPC namespace must be explicit'
  arguments = [str(binary), mode.removesuffix('-debug'), '0']
  if mode.endswith('-debug'):
    arguments.append('--debug')
  arguments.extend(options)
  return await asyncio.create_subprocess_exec(*arguments, env=environment, stdout=asyncio.subprocess.PIPE, stderr=asyncio.subprocess.PIPE)


async def until(predicate):
  loop = asyncio.get_running_loop()
  tick = asyncio.Event()
  async with asyncio.timeout(3):
    while not predicate():
      scheduled = loop.call_later(0.005, tick.set)
      try:
        await tick.wait()
      finally:
        scheduled.cancel()
        tick.clear()


class Client:
  def __init__(self):
    self.peer = aiortc.RTCPeerConnection(aiortc.RTCConfiguration(iceServers=[]))
    self.channel = self.peer.createDataChannel('data', ordered=True)
    self.messages, self.packets, self.decoded, self.tracks, self.tasks = [], [], [], [], []
    self.channel.on('message', self.messages.append)
    transceiver = self.peer.addTransceiver('video', direction='recvonly')
    receive = transceiver.receiver._handle_rtp_packet

    async def observed(packet, arrival_time_ms):
      self.packets.append(
        {
          'version': packet.version,
          'marker': packet.marker,
          'payload_type': packet.payload_type,
          'timestamp': packet.timestamp,
          'ssrc': packet.ssrc,
          'csrc': packet.csrc,
          'padding_size': packet.padding_size,
          'sequence': packet.sequence_number,
          'payload': list(packet.payload),
          'extensions': asdict(packet.extensions),
        }
      )
      await receive(packet, arrival_time_ms)

    transceiver.receiver._handle_rtp_packet = observed
    self.peer.on('track', self.track)

  def track(self, track):
    self.tracks.append(track.id)

    async def consume():
      while True:
        frame = await track.recv()
        self.decoded.append({'width': frame.width, 'height': frame.height, 'pts': frame.pts})

    self.tasks.append(asyncio.create_task(consume()))

  async def offer(self):
    await self.peer.setLocalDescription(await self.peer.createOffer())
    return self.peer.localDescription.sdp

  async def answer(self, answer):
    await self.peer.setRemoteDescription(aiortc.RTCSessionDescription(**answer))
    await until(lambda: self.channel.readyState == 'open')

  async def close(self):
    await self.peer.close()
    for task in self.tasks:
      task.cancel()
    await asyncio.gather(*self.tasks, return_exceptions=True)
