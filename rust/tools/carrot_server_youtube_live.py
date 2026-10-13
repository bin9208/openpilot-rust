#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Normal complete service: actual Cereal/IPC, warmup, RTMPS/FLV/AAC and owned cleanup."""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import subprocess
import sys

import anyio
from anyio.streams.buffered import BufferedByteReceiveStream
from carrot_server_dashcam_upload import request, save
from carrot_server_youtube_decode import decoded
from carrot_server_youtube_fixture import Peer, Provider
from carrot_server_youtube_ingest import Ingest


class Publisher:
  def __init__(self, peer: Peer, frames: Path) -> None:
    self.peer = peer
    self.frames = frames
    self.process: anyio.abc.Process | None = None
    self.reader: BufferedByteReceiveStream | None = None
    self.log = None

  async def start(self) -> None:
    argv = [sys.executable, '-P', str(Path(__file__).with_name('carrot_server_youtube_publisher.py'))]
    environment = os.environ | {'PARAMS_ROOT': str(self.peer.params), 'OPENPILOT_PREFIX': self.peer.prefix, 'CARROT_DATA_DIR': str(self.peer.root)}
    self.log = await anyio.Path(self.peer.root / 'publisher.log').open('wb')
    self.process = await anyio.open_process(argv, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=self.log.wrapped, env=environment)
    assert self.process.stdin and self.process.stdout
    config = {'owned_root': str(self.peer.root), 'input': str(self.frames), 'seconds': 11}
    await self.process.stdin.send((json.dumps(config) + '\n').encode())
    self.reader = BufferedByteReceiveStream(self.process.stdout)
    with anyio.fail_after(4):
      ready = json.loads(await self.reader.receive_until(b'\n', 65536))
    await anyio.to_thread.run_sync(
      save, self.peer.root / 'publisher-invocation.json', {'argv': argv, 'input': config, 'ready': ready, 'namespace': self.peer.prefix}
    )

  async def play(self) -> None:
    assert self.process and self.process.stdin
    await self.process.stdin.send(b'play\n')

  async def finish(self) -> None:
    assert self.process and self.reader
    with anyio.fail_after(14):
      terminal = json.loads(await self.reader.receive_until(b'\n', 65536))
      await self.process.wait()
    await anyio.to_thread.run_sync(save, self.peer.root / 'publisher-terminal.json', terminal)
    assert self.process.returncode == 0 and 200 <= terminal['sent'] <= 220 and terminal['fresh_idr_reader_ack']

  async def close(self) -> None:
    if self.process:
      if self.process.returncode is None:
        self.process.kill()
      await self.process.wait()
      await self.process.aclose()
    if self.log:
      await self.log.aclose()


async def run(args: argparse.Namespace) -> None:
  output = Path(str(await anyio.Path(args.output).resolve()))
  await anyio.Path(output).mkdir(parents=True)
  certificate = Path(str(await anyio.Path(args.certificate).resolve()))
  key = Path(str(await anyio.Path(args.key).resolve()))
  binary = Path(str(await anyio.Path(args.binary).resolve()))
  frames = Path(str(await anyio.Path(args.input).resolve()))
  os.environ['ORIGINAL_PARAMS_BINDING'] = str(await anyio.Path(args.binding).resolve())
  peers = [Peer(Provider(name, binary, args.application), output / name, Ingest(output / name, (certificate, key))) for name in ['source', 'native']]
  publishers = [Publisher(peer, frames) for peer in peers]
  try:
    for peer, publisher in zip(peers, publishers, strict=True):
      await anyio.Path(peer.params / peer.prefix).mkdir(parents=True)
      await anyio.Path('/dev/shm', 'msgq_' + peer.prefix).mkdir()
      await anyio.Path(peer.root / 'state').mkdir()
      await anyio.Path(peer.root / 'state/youtube_live_secret.json').write_text('{"stream_key":"owned-stream-key"}')
      await anyio.Path(peer.params / peer.prefix / 'CarrotYouTubeLive').write_bytes(b'1')
      await publisher.start()
      await peer.start(certificate)
    for publisher in publishers:
      await publisher.play()
    for index, peer in enumerate(peers):
      with anyio.fail_after(10):
        while True:
          result = await request(peer.port, '/api/youtube_live/status')
          payload = result['payload']
          if payload['running'] and payload['rtmp_writer_frames_written'] > 0:
            break
          await anyio.sleep(0.1)
      await anyio.to_thread.run_sync(save, output / f'live-{index}.json', result)
      assert payload['state'] == 'live' and payload['quality'] == 'low'
      assert payload['frame_matches_target'] and payload['frame_width'] == 854 and payload['frame_height'] == 480
      assert payload['transport_connected'] and payload['mux_input_bytes'] > 0 and payload['restart_count'] == 1
    for publisher in publishers:
      await publisher.finish()
    for peer in peers:
      await anyio.to_thread.run_sync(save, peer.root / 'before-stop.json', await request(peer.port, '/api/youtube_live/status'))
    if args.application:
      native = peers[1]
      response = await request(native.port, '/api/heartbeat_status')
      await anyio.to_thread.run_sync(save, native.root / 'app-isolation.json', response)
      assert response['status'] == 200
      for label, method in [('app-diagnostics', 'GET'), ('app-status-head', 'HEAD'), ('app-key', 'GET')]:
        path = '/api/youtube_live/' + ('diagnostics' if label == 'app-diagnostics' else 'status' if method == 'HEAD' else 'stream_key')
        await anyio.to_thread.run_sync(save, native.root / (label + '.json'), await request(native.port, path, method))
      with anyio.fail_after(10):
        while True:
          statuses = [await request(peer.port, '/api/youtube_live/status') for peer in peers]
          if all(value['payload']['state'] == 'backoff' for value in statuses):
            break
          await anyio.sleep(0.1)
      for peer, status in zip(peers, statuses, strict=True):
        await anyio.to_thread.run_sync(save, peer.root / 'frame-gap.json', status)
        assert status['payload']['consecutive_failures'] == 1
        assert status['payload']['last_error'] == 'no youtubeRoadEncodeData frames'
  finally:
    errors = []
    with anyio.CancelScope(shield=True):
      for publisher in publishers:
        try:
          await publisher.close()
        except (OSError, TimeoutError, anyio.BrokenResourceError, anyio.ClosedResourceError) as error:
          errors.append(str(error))
      for peer in peers:
        try:
          errors.extend(await peer.close())
        except (OSError, TimeoutError, AssertionError, anyio.BrokenResourceError, anyio.ClosedResourceError) as error:
          errors.append(str(error))
        shared = anyio.Path('/dev/shm', 'msgq_' + peer.prefix)
        if await shared.exists():
          async for path in shared.iterdir():
            await path.unlink()
          await shared.rmdir()
    await anyio.to_thread.run_sync(save, output / 'cleanup.json', {'errors': errors})
    assert not errors, errors
  for peer in peers:
    recipient = peer.root / 'session-0/recipient.flv'
    media = await anyio.to_thread.run_sync(lambda path=recipient: decoded(path.read_bytes()))
    await anyio.to_thread.run_sync(save, peer.root / 'decoded.json', media)
    assert len(media['video']) >= 15 and len(media['audio']) > 0
    assert all(frame['width'] == 854 and frame['height'] == 480 for frame in media['video'])
    assert all(frame['rate'] == 44100 and frame['layout'] == 'stereo' for frame in media['audio'])
  await anyio.to_thread.run_sync(
    save,
    output / 'result.json',
    {
      'actual_cereal_services': ['youtubeRoadEncodeData'],
      'fresh_idr_reader_ack': True,
      'real_rtmps_publish_sessions': 2,
      'actual_native_application': args.application,
      'frame_gap_backoff_observed': args.application,
      'actual_flv_video_aac_decoded': True,
      'pass': True,
    },
  )


def main() -> None:
  parser = argparse.ArgumentParser()
  for field in ['binary', 'binding', 'input', 'certificate', 'key', 'output']:
    parser.add_argument('--' + field, type=Path, required=True)
  parser.add_argument('--application', action='store_true')
  anyio.run(run, parser.parse_args())


if __name__ == '__main__':
  main()
