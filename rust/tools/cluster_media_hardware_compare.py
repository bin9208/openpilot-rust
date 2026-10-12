import argparse
import ast
from dataclasses import dataclass
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import threading


def digest(path):
  return hashlib.sha256(path.read_bytes()).hexdigest()


def source(root, packet):
  sys.path.insert(0, str(root / 'openpilot/selfdrive/carrot/cluster'))
  from cluster_h264_decoder import HardwareH264DecoderError, create_tici_h264_decoder

  path = root / 'openpilot/selfdrive/carrot/cluster/cluster_navi_source.py'
  nodes = [node for node in ast.parse(path.read_text()).body if isinstance(node, ast.ClassDef) and node.name in ('H264Decoder', 'DecodedH264Frame')]
  scope = {
    'dataclass': dataclass,
    'threading': threading,
    'create_tici_h264_decoder': create_tici_h264_decoder,
    'HardwareH264DecoderError': HardwareH264DecoderError,
    'TiciH264DecodedBuffer': object,
  }
  exec(compile(ast.fix_missing_locations(ast.Module(body=nodes, type_ignores=[])), str(path), 'exec'), scope)
  decoder = scope['H264Decoder']()
  payload = packet.read_bytes()
  frame = decoder.decode(payload, sequence=1, width=96, height=64)
  retained = frame.hardware_buffer
  assert retained is not None
  retained.mark_egl_import_failed('controlled original-contract EGL import failure')
  software = decoder.decode(payload, sequence=2, width=96, height=64)
  decoder.close()
  value = {
    'hardware_layout': [retained.width, retained.height, retained.stride, retained.uv_offset],
    'hardware_token': list(retained.token),
    'hardware_sequence': retained.sequence,
    'lease_after_close': os.fstat(retained.fd).st_size > 0,
    'software': {
      'width': software.width,
      'height': software.height,
      'plane_strides': software.strides,
      'plane_data': [list(plane) for plane in software.planes],
    },
  }
  retained.release()
  print(json.dumps(value))


def visible(value):
  frame = value['software']
  output = []
  for index, plane in enumerate(frame['plane_data']):
    width = frame['width'] // (1 if index == 0 else 2)
    height = frame['height'] // (1 if index == 0 else 2)
    stride = frame['plane_strides'][index]
    output.append(bytes(byte for row in range(height) for byte in plane[row * stride : row * stride + width]))
  return output


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('root', type=Path)
  parser.add_argument('output', type=Path)
  parser.add_argument('--binary', type=Path)
  parser.add_argument('--packet', type=Path, required=True)
  parser.add_argument('--provider', type=Path)
  parser.add_argument('--pyav', type=Path)
  parser.add_argument('--source', action='store_true')
  args = parser.parse_args()
  root, output = args.root.resolve(), args.output.resolve()
  if args.source:
    source(root, args.packet.resolve())
    return
  output.mkdir(parents=True, exist_ok=False)
  prepared = (root / '.omo/evidence/253-cluster-resume/vidc-source-abi/source.cc').read_text()
  bridge = root / 'openpilot/system/loggerd/decoder/cluster_h264_decoder_bridge.cc'
  text = '#include <memory>\n' + prepared[: prepared.index('int main(')] + '\n' + '\n'.join(bridge.read_text().splitlines()[1:])
  (output / 'source.cc').write_text(text)
  available, growth = shutil.disk_usage(output).free, 128 * 1024**2
  assert available >= 25 * 1024**3 + growth
  build = [
    'c++',
    '-std=c++20',
    '-O1',
    '-g0',
    '-pthread',
    '-shared',
    '-fPIC',
    '-I' + str(root),
    '-I' + str(root / 'openpilot'),
    '-I' + str(root / 'third_party/linux/include'),
    str(output / 'source.cc'),
    '-o',
    str(output / 'source-bridge.so'),
  ]
  built = subprocess.run(build, capture_output=True, timeout=30, check=False)
  (output / 'source-build.log').write_bytes(built.stdout + built.stderr)
  assert built.returncode == 0
  env = {
    **os.environ,
    'LD_PRELOAD': str(args.provider.resolve()),
    'VIDC_FAKE_CASE': 'normal',
    'CLUSTER_H264_DECODER_LIBRARY': str(output / 'source-bridge.so'),
    'PYTHONPATH': str(args.pyav.resolve()),
  }
  commands = {
    'source': [sys.executable, str(Path(__file__).resolve()), str(root), str(output), '--packet', str(args.packet.resolve()), '--source'],
    'native': [str(args.binary.resolve()), str(args.packet.resolve())],
  }
  values, invocations = {}, {}
  for kind, command in commands.items():
    with tempfile.TemporaryDirectory(prefix='msgq_rust-probe-cluster-hardware-', dir='/dev/shm') as namespace:
      call_env = {**env, 'OPENPILOT_PREFIX': Path(namespace).name.removeprefix('msgq_'), 'VIDC_FAKE_TRACE': str(output / f'{kind}.trace')}
      result = subprocess.run(command, env=call_env, capture_output=True, timeout=8, check=False)
    (output / f'{kind}.stdout').write_bytes(result.stdout)
    (output / f'{kind}.stderr').write_bytes(result.stderr)
    invocations[kind] = {'argv': command, 'exit_code': result.returncode}
    assert result.returncode == 0, (kind, result.stderr.decode())
    values[kind] = json.loads(result.stdout.splitlines()[-1])
  fields = ('hardware_layout', 'hardware_token', 'hardware_sequence', 'lease_after_close')
  compared = {field: values['source'][field] == values['native'][field] for field in fields}
  compared['visible_planes'] = visible(values['source']) == visible(values['native'])
  compared['strides'] = values['source']['software']['plane_strides'] == values['native']['software']['plane_strides']
  compared['native_worker_closed'] = values['native']['worker_closed']
  receipt = {
    'matched': all(compared.values()),
    'checks': compared,
    'invocations': invocations,
    'build': {'argv': build, 'available_bytes': available, 'estimated_growth_bytes': growth},
    'binary_sha256': digest(args.binary),
    'provider_sha256': digest(args.provider),
    'packet_sha256': digest(args.packet),
    'source_bridge_sha256': digest(bridge),
    'scope': 'original hardware-first decoder wrapper versus native owned Cereal IPC worker; controlled kernel ABI and actual FFmpeg decoded visible pixels',
  }
  (output / 'result.json').write_text(json.dumps(receipt, indent=2) + '\n')
  print(json.dumps({'matched': receipt['matched'], 'checks': compared}))
  raise SystemExit(not receipt['matched'])


if __name__ == '__main__':
  main()
