import asyncio
import hashlib
import time

from aiortc.codecs.h264 import H264PayloadDescriptor
from aiortc.codecs.vpx import VpxPayloadDescriptor
from aiortc.rtp import RtpPacket, is_rtcp
from webrtc_test_peer import Client


class DebugClient(Client):
  def __init__(self):
    super().__init__()
    self.wire, self.dropped = [], []
    receiver = self.peer.getTransceivers()[0].receiver
    transport = receiver.transport
    recv, rtp = transport.transport._recv, transport._handle_rtp_data

    async def lossy_recv():
      while True:
        data = await recv()
        if not self.dropped and data and 128 <= data[0] < 192 and not is_rtcp(data):
          self.dropped.append(
            {
              'sequence': int.from_bytes(data[2:4], 'big'),
              'ssrc': int.from_bytes(data[8:12], 'big'),
              'timestamp': int.from_bytes(data[4:8], 'big'),
              'datagram': data.hex(),
            }
          )
          continue
        return data

    async def observed(data, arrival_time_ms):
      packet = RtpPacket.parse(data)
      self.wire.append(
        {
          'elapsed': time.monotonic(),
          'pt': packet.payload_type,
          'sequence': packet.sequence_number,
          'timestamp': packet.timestamp,
          'ssrc': packet.ssrc,
          'marker': packet.marker,
          'payload': packet.payload.hex(),
          'raw': data.hex(),
        }
      )
      await rtp(data, arrival_time_ms)

    transport.transport._recv, transport._handle_rtp_data = lossy_recv, observed

  def track(self, track):
    self.tracks.append(track.id)

    async def consume():
      while True:
        frame = await track.recv()
        planes = []
        for plane in frame.planes:
          raw = bytes(plane)
          visible = b''.join(raw[row * plane.line_size : row * plane.line_size + plane.width] for row in range(plane.height))
          planes.append(
            {'width': plane.width, 'height': plane.height, 'sha256': hashlib.sha256(visible).hexdigest(), 'minimum': min(visible), 'maximum': max(visible)}
          )
        self.decoded.append(
          {'width': frame.width, 'height': frame.height, 'format': frame.format.name, 'pts': frame.pts, 'elapsed': time.monotonic(), 'planes': planes}
        )

    self.tasks.append(asyncio.create_task(consume()))


def primary(row, payload_type, rtx_type):
  if row['pt'] == payload_type:
    return bytes.fromhex(row['payload'])
  if row['pt'] == rtx_type:
    return bytes.fromhex(row['payload'])[2:]
  return None


def keyframe(row, payload_type, rtx_type, codec):
  payload = primary(row, payload_type, rtx_type)
  if payload is None:
    return False
  if codec == 'vp8':
    descriptor, data = VpxPayloadDescriptor.parse(payload)
    return descriptor.partition_start and bool(data) and data[0] & 1 == 0
  _, data = H264PayloadDescriptor.parse(payload)
  return any(nalu and nalu[0] & 31 == 5 for nalu in data.split(b'\x00\x00\x00\x01'))
