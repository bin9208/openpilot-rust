from __future__ import annotations

from dataclasses import dataclass
import hashlib
import json
import os
from pathlib import Path
import struct
import subprocess
import sys

from Crypto.Cipher import DES
from PIL import Image


@dataclass(frozen=True)
class Scenario:
  name: str
  size: str
  fps: int
  ack: bool
  soft: bool


def wire(path: Path, limit: int) -> tuple[bytes, list[str], int]:
  data = path.read_bytes()
  chunks = []
  commands = []
  count = 0
  while data:
    length = struct.unpack('<I', data[:4])[0]
    packet, data = data[4 : 4 + length], data[4 + length :]
    assert len(packet) == length and length >= 512
    plain = bytearray(DES.new(b'slv3tuzx', DES.MODE_CBC, b'slv3tuzx').decrypt(packet[:504]))
    plain[4:8] = bytes(4)
    if plain[0] == 121:
      size = struct.unpack('>I', plain[8:12])[0]
      assert size == length - 512 and 0 < size <= limit and plain[12] == 0
      chunks.append(packet[512:])
      count += 1
    else:
      commands.append(hashlib.sha256(plain + packet[504:]).hexdigest())
  return b''.join(chunks), commands, count


def run(root: Path, binding: Path, library: Path, native: Path, frames: Path, output: Path) -> None:
  output.mkdir(parents=True)
  results = []
  scenarios = [Scenario('ack', '384x192', 10, True, False), Scenario('soft-active', '800x480', 5, True, True), Scenario('no-ack', '384x192', 10, False, False)]
  for scenario in scenarios:
    directory = output / scenario.name
    receipts, streams, command_payloads = [], [], []
    width, height = map(int, scenario.size.split('x'))
    for implementation in ['source', 'native']:
      destination = directory / implementation
      params = destination / 'params/fixture'
      params.mkdir(parents=True)
      (params / 'UsbGpuActive').write_bytes(b'1' if scenario.fps == 5 else b'0')
      (params / 'UsbGpuLoading').write_bytes(b'0')
      rendered = frames / scenario.size / 'poll-stale' / implementation
      pixels = [Image.open(rendered / f'frame-{index:03}.png').convert('RGBA').tobytes() for index in [1, 3, 4, 7]]
      input_file = destination / 'input.rgba'
      input_file.write_bytes(b''.join(pixels * 20))
      case = {
        'input': str(input_file),
        'root': str(params.parent),
        'lock': str(destination / 'bus.lock'),
        'dump': str(destination / 'dump.h264'),
        'width': width,
        'height': height,
        'fps': scenario.fps,
        'gop': 10,
        'bitrate': '800k',
        'chunk_size': 4096,
        'wait_ack': scenario.ack,
        'soft_ack': scenario.soft,
        'encoder': 'libx264',
      }
      case_file = destination / 'case.json'
      case_file.write_text(json.dumps(case))
      env = {
        **os.environ,
        'PARAMS_ROOT': str(params.parent),
        'OPENPILOT_PREFIX': 'fixture',
        'TZ': 'UTC',
        'USB_FIXTURE_MODE': 'ok',
        'USB_FIXTURE_BYTES': str(destination / 'bytes.bin'),
        'USB_FIXTURE_WIRE': str(destination / 'wire.txt'),
        'USB_FIXTURE_LOG': str(destination / 'libusb.log'),
        'LD_LIBRARY_PATH': str(library.parent),
      }
      command = (
        [sys.executable, str(root / 'rust/tools/cluster_h264_source.py'), str(root), str(binding), str(library), str(case_file)]
        if implementation == 'source'
        else [str(native), str(case_file)]
      )
      completed = subprocess.run(command, env=env, capture_output=True, text=True, timeout=40)
      (destination / 'invocation.json').write_text(
        json.dumps(
          {
            'command': command,
            'input': case,
            'exit_code': completed.returncode,
            'environment': {key: env[key] for key in ['PARAMS_ROOT', 'OPENPILOT_PREFIX', 'TZ', 'LD_LIBRARY_PATH']},
            'stdout': completed.stdout,
            'stderr': completed.stderr,
          },
          indent=2,
        )
      )
      assert completed.returncode == 0, (command, completed.stdout, completed.stderr)
      receipt = json.loads(completed.stdout.splitlines()[-1])
      assert receipt['initial'] and not receipt['closed'] and receipt['reaped'] and receipt['error'] is None
      assert receipt['submitted'] == 80
      assert (params / 'ClusterHudConnected').read_bytes() == b'0'
      assert (destination / 'libusb.log').read_text().endswith('exit clean\n')
      stream, commands, count = wire(destination / 'bytes.bin', 4096)
      assert stream == (destination / 'dump.h264').read_bytes() and len(stream) == receipt['bytes_sent'] and count == receipt['chunks_sent']
      (destination / 'wire.h264').write_bytes(stream)
      probe_command = [
        '/usr/bin/ffprobe',
        '-v',
        'error',
        '-select_streams',
        'v:0',
        '-count_frames',
        '-show_entries',
        'stream=width,height,nb_read_frames',
        '-of',
        'json',
        str(destination / 'wire.h264'),
      ]
      probe = subprocess.run(probe_command, capture_output=True, text=True, check=True, timeout=10)
      (destination / 'ffprobe.json').write_text(json.dumps({'command': probe_command, 'output': json.loads(probe.stdout)}, indent=2))
      decoded = json.loads(probe.stdout)['streams'][0]
      assert (decoded['width'], decoded['height']) == (width, height) and int(decoded['nb_read_frames']) > 0
      receipts.append(receipt)
      streams.append(stream)
      command_payloads.append(commands)
    assert receipts[0]['command'] == receipts[1]['command']
    assert streams[0] == streams[1] and command_payloads[0] == command_payloads[1]
    row = {
      'scenario': scenario.name,
      'size': [width, height],
      'fps': scenario.fps,
      'pass': True,
      'bytes': len(streams[0]),
      'sha256': hashlib.sha256(streams[0]).hexdigest(),
      'chunks': [receipt['chunks_sent'] for receipt in receipts],
      'non_h264_commands': len(command_payloads[0]),
    }
    (directory / 'result.json').write_text(json.dumps(row, indent=2))
    results.append(row)
    print(json.dumps(row))
  result = {
    'pass': True,
    'scenarios': results,
    'binary_sha256': hashlib.sha256(native.read_bytes()).hexdigest(),
      'scope': 'rendered frames through real FFmpeg processes and owned libusb ABI; read segmentation may vary; no physical display',
  }
  (output / 'result.json').write_text(json.dumps(result, indent=2))


if __name__ == '__main__':
  run(*(Path(argument).resolve() for argument in sys.argv[1:]))
