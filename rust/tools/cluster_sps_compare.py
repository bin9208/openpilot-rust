import argparse
import ast
import hashlib
import json
from pathlib import Path
import subprocess


def original(root):
  path = root / 'openpilot/selfdrive/carrot/cluster/cluster_h264_pipeline.py'
  tree = ast.parse(path.read_text())
  nodes = [ast.ImportFrom(module='__future__', names=[ast.alias(name='annotations')], level=0)]
  for node in tree.body:
    if isinstance(node, ast.ClassDef) and node.name in ['_H264BitReader', '_H264BitWriter']:
      nodes.append(node)
    elif isinstance(node, ast.FunctionDef) and node.name.startswith(('_h264_', '_patch_h264_', '_write_h264_')):
      nodes.append(node)
    elif isinstance(node, ast.ClassDef) and node.name == 'H264UsbPipeline':
      node.body = [method for method in node.body if isinstance(method, ast.FunctionDef) and method.name == '_prepare_hardware_packet']
      nodes.append(node)
  scope = {}
  exec(compile(ast.fix_missing_locations(ast.Module(body=nodes, type_ignores=[])), str(path), 'exec'), scope)
  return scope


def synthetic(scope, profile, chroma, separate, scaling, poc, frame_only, cropped, vui):
  writer = scope['_H264BitWriter']()
  writer.write_bits(profile, 8)
  writer.write_bits(0, 8)
  writer.write_bits(31, 8)
  writer.write_ue(0)
  if profile != 66:
    writer.write_ue(chroma)
    if chroma == 3:
      writer.write_bit(separate)
    writer.write_ue(0)
    writer.write_ue(0)
    writer.write_bit(0)
    writer.write_bit(scaling)
    if scaling:
      for index in range(12 if chroma == 3 else 8):
        writer.write_bit(index == 0)
        if index == 0:
          for _ in range(16):
            writer.write_ue(0)
  writer.write_ue(0)
  writer.write_ue(poc)
  if poc == 0:
    writer.write_ue(0)
  elif poc == 1:
    writer.write_bit(0)
    writer.write_ue(0)
    writer.write_ue(0)
    writer.write_ue(2)
    writer.write_ue(1)
    writer.write_ue(2)
  writer.write_ue(2)
  writer.write_bit(0)
  writer.write_ue(31)
  writer.write_ue(119)
  writer.write_bit(frame_only)
  if not frame_only:
    writer.write_bit(0)
  writer.write_bit(1)
  writer.write_bit(cropped)
  if cropped:
    for value in [1, 2, 1, 3]:
      writer.write_ue(value)
  writer.write_bit(vui != 'none')
  if vui == 'timing':
    scope['_write_h264_vui_timing'](writer, 9)
  elif vui == 'untimed':
    for _ in range(9):
      writer.write_bit(0)
  writer.write_bit(1)
  return b'\x00\x00\x00\x01\x67' + scope['_h264_escape_rbsp'](writer.to_bytes())


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('root', type=Path)
  parser.add_argument('output', type=Path)
  parser.add_argument('--binary', type=Path)
  args = parser.parse_args()
  root, output = args.root.resolve(), args.output.resolve()
  output.mkdir(parents=True, exist_ok=False)
  scope = original(root)
  packets = []
  for profile in [66, 100, 110, 122, 244, 44, 83, 86, 118, 128, 138, 139, 134, 135]:
    for chroma, separate, scaling, poc, frame_only, cropped, vui in [
      (1, 0, 0, 0, 1, 0, 'none'),
      (1, 0, 1, 1, 0, 1, 'untimed'),
      (3, 1, 0, 2, 1, 1, 'timing'),
      (2, 0, 0, 0, 1, 0, 'malformed'),
    ]:
      packet = synthetic(scope, profile, chroma, separate, scaling, poc, frame_only, cropped, vui)
      packets.extend([packet, b'prefix\x00\x00\x01\x68\x80' + packet + packet + b'\x00\x00'])
  packets += [b'', b'junk', b'\x00\x00\x01', b'\x00\x00\x01\x67', b'\x00\x00\x01\x67\x42\x00\x1f\x00\x00\x00\x00\x00']
  packets += [path.read_bytes() for path in sorted((root / '.omo/evidence/253-cluster-resume/decoder-source-aligned').glob('*.h264'))]
  cases = [
    {'data': list(packet), 'width': width, 'height': height, 'fps': fps}
    for packet in packets
    for width, height, fps in [(480, 1920, 10), (479, 1919, 5), (512, 3840, 30), (96, 64, 0), (-3, -2, -1), (480, 1920, 9223372036854775807)]
  ]
  expected = []
  for case in cases:
    data, width, height, fps = bytes(case['data']), case['width'], case['height'], case['fps']
    pipeline = scope['H264UsbPipeline'].__new__(scope['H264UsbPipeline'])
    pipeline.width, pipeline.height, pipeline.fps, pipeline.debug, pipeline._sps_patch_cache = width, height, fps, False, None
    prepared = [pipeline._prepare_hardware_packet(data, may_have_sps=enabled) for enabled in [True, True, False]]
    left = {
      'constraints': scope['_patch_h264_sps_constraints'](data),
      'crop': scope['_patch_h264_sps_crop'](data, width, height),
      'vui': scope['_patch_h264_sps_vui_timing'](data, fps),
      'nals': scope['_h264_nals'](data),
      'starts': scope['_h264_start_codes'](data),
      'units': scope['_h264_byte_stream_units'](data),
      'first': scope['_h264_first_unit_of_type'](data, 7),
      'prepared': prepared,
    }
    expected.append(json.loads(json.dumps(left, default=list)))
  (output / 'input.json').write_text(json.dumps(cases))
  (output / 'expected.json').write_text(json.dumps(expected))
  if args.binary is None:
    print(json.dumps({'cases': len(cases)}))
    return
  command = [str(args.binary.resolve()), str(output / 'input.json')]
  result = subprocess.run(command, capture_output=True, timeout=30, check=False)
  (output / 'native.stdout').write_bytes(result.stdout)
  (output / 'native.stderr').write_bytes(result.stderr)
  assert result.returncode == 0
  actual = json.loads(result.stdout)
  mismatches = [
    {'case': index, 'fields': [key for key in left if left[key] != right[key]]}
    for index, (left, right) in enumerate(zip(expected, actual, strict=True))
    if left != right
  ]
  receipt = {
    'argv': command,
    'exit_code': result.returncode,
    'cases': len(cases),
    'matched': len(cases) - len(mismatches),
    'mismatches': mismatches,
    'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest(),
    'source_sha256': hashlib.sha256((root / 'openpilot/selfdrive/carrot/cluster/cluster_h264_pipeline.py').read_bytes()).hexdigest(),
  }
  (output / 'result.json').write_text(json.dumps(receipt, indent=2) + '\n')
  print(json.dumps({key: receipt[key] for key in ['cases', 'matched', 'mismatches']}))
  raise SystemExit(bool(mismatches))


if __name__ == '__main__':
  main()
