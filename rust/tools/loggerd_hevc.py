"""Real native HEVC bitstream with split codec header over original msgq."""
from __future__ import annotations

from hashlib import sha256
import json
from pathlib import Path
import re
import subprocess

import openpilot.cereal.messaging as messaging
from loggerd_peer import Peer, PeerSettings


def fixture(output: Path) -> tuple[bytes, list[bytes]]:
  output.mkdir(parents=True)
  path = output / 'native.hevc'
  command = ['ffmpeg', '-nostdin', '-v', 'error', '-f', 'lavfi', '-i', 'testsrc2=size=160x120:rate=20',
             '-frames:v', '8', '-c:v', 'libx265', '-x265-params', 'log-level=error:pools=1:frame-threads=1', '-f', 'hevc', str(path)]
  with (output / 'encoder.log').open('wb') as diagnostics:
    subprocess.run(command, stdout=diagnostics, stderr=subprocess.STDOUT, check=True)
  probe = subprocess.run(['ffprobe', '-v', 'error', '-show_packets', '-of', 'json', str(path)], stdout=subprocess.PIPE, check=True)
  (output / 'packets.json').write_bytes(probe.stdout)
  raw = path.read_bytes()
  records = []
  chunks = []
  for sequence, packet in enumerate(json.loads(probe.stdout)['packets']):
    start, size = int(packet['pos']), int(packet['size'])
    data = raw[start:start + size]
    chunks.append(data)
    header = b''
    if sequence == 0:
      for nal in re.finditer(b'\x00\x00(?:\x00)?\x01', data):
        if (data[nal.end()] >> 1) & 63 <= 31:
          header, data = data[:nal.start()], data[nal.start():]
          break
      assert header
    message = messaging.new_message('roadEncodeData')
    message.valid = True
    message.logMonoTime = 20_000_000_000 + sequence
    encoded = message.roadEncodeData
    # The raw writer does not consume dimensions; zero also tests that contract.
    encoded.width = encoded.height = 0
    encoded.header, encoded.data = header, data
    encoded.idx.type = 'fullHEVC'
    encoded.idx.segmentNum = 12
    encoded.idx.frameId = encoded.idx.encodeId = encoded.idx.segmentId = sequence
    encoded.idx.timestampEof = 1_000_000_000 + sequence * 50_000_000
    encoded.idx.flags = 8 if 'K' in packet['flags'] else 0
    encoded.idx.len = len(data)
    records.append(message.to_bytes())
  assert b''.join(chunks) == raw
  (output / 'invocation.json').write_text(json.dumps({'argv': command, 'bitstream_sha256': sha256(raw).hexdigest(), 'packets': len(records)}, indent=2) + '\n')
  return raw, records


def run(binary: Path, root: Path, sample: tuple[bytes, list[bytes]], runner: tuple[str, ...] = ()) -> tuple[Peer, list[dict]]:
  raw, records = sample
  peer = Peer(binary, root, PeerSettings(('roadEncodeData',), runner=runner))
  try:
    for record in records:
      peer.send('roadEncodeData', record)
    logs = peer.stop()
    assert (peer.segment(0) / 'fcamera.hevc').read_bytes() == raw
    return peer, logs
  except BaseException:
    peer.cleanup()
    raise
