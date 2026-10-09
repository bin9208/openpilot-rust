"""Observe actual original VisionIPC handshakes and frame metadata."""

from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import select
import sys
import time

from msgq.visionipc import VisionIpcClient


def main(output: Path, before_server: bool = False, cereal: bool = False) -> None:
  owned = Path(os.environ['WEBCAM_OWNED_ROOT']).resolve(strict=True)
  assert output.parent.resolve(strict=True) == owned and os.environ['OPENPILOT_PREFIX']
  from msgq.visionipc import VisionStreamType

  precreated = (
    {
      stream: VisionIpcClient('camerad', stream, False)
      for stream in (VisionStreamType.VISION_STREAM_ROAD, VisionStreamType.VISION_STREAM_WIDE_ROAD, VisionStreamType.VISION_STREAM_DRIVER)
    }
    if before_server
    else {}
  )
  sockets = {}
  if cereal:
    from openpilot.cereal import messaging

    sockets = {name: messaging.sub_sock(name, conflate=False) for name in ('roadCameraState', 'wideRoadCameraState', 'driverCameraState')}
  if before_server:
    print('PEER_READY', flush=True)
  streams = VisionIpcClient.available_streams('camerad', False)
  deadline = time.monotonic() + 3
  while not streams and time.monotonic() < deadline:
    time.sleep(0.005)
    streams = VisionIpcClient.available_streams('camerad', False)
  assert streams, 'owned server did not advertise any streams'
  clients, layouts, frames, connected = [], [], [], []
  for stream in streams:
    client = precreated.get(stream) or VisionIpcClient('camerad', stream, False)
    assert client.connect(False), stream
    connected.append({'stream': int(stream), 'monotonic': time.monotonic()})
    clients.append(client)
    layouts.append(
      {
        'stream': int(stream),
        'width': client.width,
        'height': client.height,
        'stride': client.stride,
        'uv_offset': client.uv_offset,
        'length': client.buffer_len,
        'count': client.num_buffers,
      }
    )
  print('READY ' + json.dumps(layouts), flush=True)
  stop, messages = False, []
  deadline = time.monotonic() + 5
  while time.monotonic() < deadline:
    if not stop and select.select([sys.stdin], [], [], 0)[0]:
      assert sys.stdin.readline().strip() == 'STOP'
      stop, deadline = True, time.monotonic() + 0.05
    for stream, client in zip(streams, clients, strict=True):
      value = client.recv(5)
      if value is not None:
        raw = value.data.tobytes()
        target = owned / f'vision-{int(stream)}-{client.frame_id}.nv12'
        target.write_bytes(raw)
        frames.append(
          {
            'stream': int(stream),
            'frame_id': client.frame_id,
            'sof': client.timestamp_sof,
            'eof': client.timestamp_eof,
            'valid': client.valid,
            'sha256': hashlib.sha256(raw).hexdigest(),
            'monotonic': time.monotonic(),
            'raw': str(target),
          }
        )
    if cereal:
      from openpilot.cereal import log

      for name, socket in sockets.items():
        for raw in messaging.drain_sock_raw(socket):
          with log.Event.from_bytes(raw) as message:
            frame_id = message.__getattr__(name).frameId
          target = owned / f'wire-{name}-{frame_id}.bin'
          target.write_bytes(raw)
          messages.append({'service': name, 'frame_id': frame_id, 'monotonic': time.monotonic(), 'raw': str(target)})
  output.write_text(
    json.dumps(
      {'layouts': layouts, 'frames': frames, 'messages': messages, 'connected': connected, 'source_provider': 'unchanged original Cython/C++ client'}, indent=2
    )
    + '\n'
  )


if __name__ == '__main__':
  main(Path(sys.argv[1]), '--before-server' in sys.argv, '--cereal' in sys.argv)
