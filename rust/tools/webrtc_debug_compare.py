import asyncio
import json
import os
from pathlib import Path
import signal
import sys
import tempfile
import time

import aioice.ice
import aiortc
import av
from aiohttp import web, ClientSession
from aiortc.rtp import RtcpPsfbPacket, pack_remb_fci
from aiortc.sdp import SessionDescription
from webrtc_debug_peer import DebugClient, keyframe
from openpilot.system.webrtc import webrtcd, carrot_session
from webrtc_test_peer import start_service, until, save


async def scenario(kind, binary, output):
  implementation, profile, codec = kind.split('-')
  folder = output / kind
  folder.mkdir(parents=True, exist_ok=False)
  client = DebugClient()
  receiver = client.peer.getTransceivers()[0].receiver
  if codec == 'h264':
    codecs = [item for item in aiortc.RTCRtpSender.getCapabilities('video').codecs if item.mimeType in ('video/H264', 'video/rtx')]
    client.peer.getTransceivers()[0].setCodecPreferences(codecs)
  runner = process = app = None
  row = {'scenario': kind, 'source_av_library_versions': av.library_versions}
  try:
    if implementation == 'source':
      webrtcd._carrot_vision_mode = profile == 'carrot'
      webrtcd._carrot_vision_params = None
      app = web.Application(middlewares=[webrtcd.cors_middleware])
      app['streams'], app['stream_lock'], app['debug'] = {}, asyncio.Lock(), True
      app.on_shutdown.append(carrot_session.on_shutdown if profile == 'carrot' else webrtcd.on_shutdown)
      app.router.add_post('/stream', carrot_session.get_stream if profile == 'carrot' else webrtcd.get_stream)
      runner = web.AppRunner(app)
      await runner.setup()
      site = web.TCPSite(runner, '127.0.0.1', 0)
      await site.start()
      port = site._server.sockets[0].getsockname()[1]
    else:
      process = await start_service(binary, profile + '-debug', folder)
      line = await asyncio.wait_for(process.stdout.readline(), 3)
      port = int(line.decode().rsplit(':', 1)[1])
      row.update({'pid': process.pid, 'listener': line.decode().strip(), 'params_root': str(folder / 'params')})
      maps = await asyncio.to_thread(Path(f'/proc/{process.pid}/maps').read_text)
      row['native_codec_library_maps'] = [
        line for line in maps.splitlines() if any(name in line for name in ('libavcodec', 'libavformat', 'libavutil', 'libvpx', 'libx264'))
      ]
    request = {'sdp': await client.offer(), 'cameras': ['road'], 'carrot_state': profile == 'carrot'}
    save(folder / 'request.json', request)
    async with ClientSession() as http:
      async with http.post(f'http://127.0.0.1:{port}/stream', json=request) as response:
        answer = await response.json()
        save(folder / 'answer.json', {'status': response.status, **answer})
        assert response.status == 200
    media = next(item for item in SessionDescription.parse(answer['sdp']).media if item.kind == 'video')
    selected = media.rtp.codecs[0]
    assert selected.mimeType.lower() == 'video/' + codec
    assert all(item.mimeType in ('video/VP8', 'video/H264', 'video/rtx') for item in media.rtp.codecs)
    rtx_type = next(item.payloadType for item in media.rtp.codecs if item.mimeType == 'video/rtx' and item.parameters['apt'] == selected.payloadType)
    await client.answer(answer)
    await until(lambda: client.dropped)
    lost = client.dropped[0]
    await receiver._send_rtcp_nack(lost['ssrc'], [lost['sequence']])
    await until(lambda: any(item['pt'] == rtx_type and bytes.fromhex(item['payload'])[:2] == lost['sequence'].to_bytes(2, 'big') for item in client.wire))
    if codec == 'vp8':
      await until(lambda: len(client.decoded) >= 5)
    row['decoded_before_pli'] = len(client.decoded)
    pli_time = time.monotonic()
    await receiver._send_rtcp(RtcpPsfbPacket(fmt=1, ssrc=240, media_ssrc=lost['ssrc']))
    await until(lambda: any(item['elapsed'] >= pli_time and keyframe(item, selected.payloadType, rtx_type, codec) for item in client.wire))
    await until(
      lambda: any(item['elapsed'] >= pli_time and item['marker'] and not keyframe(item, selected.payloadType, rtx_type, codec) for item in client.wire)
    )
    remb_time = time.monotonic()
    await receiver._send_rtcp(RtcpPsfbPacket(fmt=15, ssrc=240, media_ssrc=0, fci=pack_remb_fci(1_500_000, [lost['ssrc']])))
    await until(lambda: any(item['elapsed'] >= remb_time and keyframe(item, selected.payloadType, rtx_type, codec) for item in client.wire))
    await until(lambda: len(client.decoded) >= 12)
    row.update(
      {
        'wire': client.wire,
        'dropped': client.dropped,
        'decoded': client.decoded,
        'codec': selected.mimeType,
        'primary_type': selected.payloadType,
        'rtx_type': rtx_type,
        'pli_time': pli_time,
        'remb_time': remb_time,
        'messages': [value.hex() if isinstance(value, bytes) else value for value in client.messages],
        'rtx_recovery': True,
        'pli_keyframe': True,
        'remb_codec_reset_keyframe': True,
      }
    )
    assert not any(isinstance(value, bytes) and value.startswith(b'CVF1') for value in client.messages)
    for frame in client.decoded:
      assert (frame['width'], frame['height'], frame['format']) == (640, 480, 'yuv420p')
    first = lost['timestamp']
    timestamps = sorted({(item['timestamp'] - first) % 2**32 for item in client.wire if item['marker']})
    expected = [0, 2999, 6000, 9000] if codec == 'vp8' else [0, 3000, 6000, 9000]
    assert timestamps[:4] == expected, timestamps[:4]
    row['initial_rtp_deltas'] = timestamps[:4]
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
          process.kill()
          await process.wait()
        stdout, stderr = await process.communicate()
        row.update({'returncode': process.returncode, 'stdout': stdout.decode(), 'stderr': stderr.decode()})
      row.setdefault('wire', client.wire)
      row.setdefault('dropped', client.dropped)
      row.setdefault('decoded', client.decoded)
      save(folder / 'result.json', row)
    finally:
      await client.close()
  return row


async def main(binary, output):
  output.mkdir(parents=True, exist_ok=False)
  constructor, addresses = aiortc.RTCPeerConnection, aioice.ice.get_host_addresses
  previous = {name: os.environ.get(name) for name in ('OPENPILOT_PREFIX', 'PARAMS_ROOT', 'WEBRTC_OWNED_ROOT')}
  os.environ['WEBRTC_OWNED_ROOT'] = str(output.resolve(strict=True))
  aiortc.RTCPeerConnection = lambda configuration=None: constructor(configuration or aiortc.RTCConfiguration(iceServers=[]))
  aioice.ice.get_host_addresses = lambda use_ipv4, use_ipv6: ['127.0.0.1']
  try:
    with tempfile.TemporaryDirectory(prefix='msgq_rtc240_debug_', dir='/dev/shm') as namespace:
      os.environ['OPENPILOT_PREFIX'] = Path(namespace).name.removeprefix('msgq_')
      kinds = sys.argv[3:] or [
        'source-standard-vp8',
        'native-standard-vp8',
        'source-carrot-vp8',
        'native-carrot-vp8',
        'source-standard-h264',
        'native-standard-h264',
      ]
      rows = [await scenario(kind, binary, output) for kind in kinds]
      save(output / 'summary.json', rows)
      for source in [row for row in rows if row['scenario'].startswith('source-')]:
        native = next((row for row in rows if row['scenario'] == source['scenario'].replace('source-', 'native-')), None)
        if native is not None:
          assert source['initial_rtp_deltas'] == native['initial_rtp_deltas']
          assert [frame['planes'] for frame in source['decoded'][:5]] == [frame['planes'] for frame in native['decoded'][:5]]
      print(json.dumps({'passed': [{'scenario': row['scenario'], 'decoded': len(row['decoded']), 'rtx': row['rtx_recovery']} for row in rows]}, indent=2))
  finally:
    aiortc.RTCPeerConnection, aioice.ice.get_host_addresses = constructor, addresses
    for name, value in previous.items():
      if value is None:
        os.environ.pop(name, None)
      else:
        os.environ[name] = value


if __name__ == '__main__':
  asyncio.run(main(*(Path(value) for value in sys.argv[1:3])))
