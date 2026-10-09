from __future__ import annotations
import argparse
import hashlib
import json
from pathlib import Path
import shutil


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--descriptor', type=Path, required=True)
  parser.add_argument('--metadata', type=Path, required=True)
  parser.add_argument('--warp', type=Path, action='append', required=True)
  parser.add_argument('--probe', type=Path, action='append', required=True)
  parser.add_argument('--qcom-warp', type=Path, action='append', required=True)
  parser.add_argument('--firmware', type=Path, required=True)
  parser.add_argument('--license', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  descriptor = json.loads(args.descriptor.read_text())
  metadata = json.loads(args.metadata.read_text())
  digest = descriptor['model_sha256']
  if len(digest) != 64 or any(char not in '0123456789abcdef' for char in digest) or metadata['model_sha256'] != digest:
    raise ValueError('model descriptor and metadata hash mismatch')
  model = Path('models') / digest
  files = [(args.descriptor, model / 'model.hcq.json'), (args.metadata, model / 'model.hcq-meta.json'), (args.license, Path('tinygrad-LICENSE'))]
  for path in args.warp:
    value = json.loads(path.read_text())
    width, height = value['camera']
    if value['arch'] != 'gfx1200' or (width, height) not in ((1344, 760), (1928, 1208)):
      raise ValueError('unsupported packaged warp')
    files.append((path, Path(f'warp-gfx1200-{width}x{height}.json')))
  for path in args.probe:
    value = json.loads(path.read_text())
    if value['arch'] not in ('gfx1200', 'gfx1201', 'gfx1100', 'gfx1101', 'gfx1102', 'gfx1150', 'gfx942', 'gfx950'):
      raise ValueError('unsupported packaged probe')
    files.append((path, Path(f'probe-{value["arch"]}.json')))
  for directory in args.qcom_warp:
    provenance = json.loads((directory / 'provenance.json').read_text())
    width, height = provenance['camera']
    graph = json.loads((directory / 'graph.json').read_text())
    if graph['backend'] != 'qcom-cl' or graph['arch'] != 'a630' or (width, height) not in ((1344, 760), (1928, 1208)):
      raise ValueError('unsupported packaged QCOM warp')
    destination = Path(f'warp-qcom-a630-{width}x{height}')
    entries = [('weights.bin', graph['weights_sha256']), *[(f'kernel-{i}.bin', kernel['binary_sha256'])
               for i, kernel in enumerate(graph['kernels'])]]
    for name, expected in entries:
      path = directory / name
      if hashlib.sha256(path.read_bytes()).hexdigest() != expected:
        raise ValueError(f'QCOM warp asset checksum mismatch: {name}')
      files.append((path, destination / name))
    files.extend((directory / name, destination / name) for name in ['graph.json', 'provenance.json'])
  files.extend((path, Path('firmware') / path.name) for path in sorted(args.firmware.glob('*.bin')))
  if len({destination for _, destination in files}) != len(files) or len(args.warp) != 2 or len(args.qcom_warp) != 2 or not list(args.firmware.glob('*.bin')):
    raise ValueError('duplicate or incomplete native GPU assets')
  growth = sum(path.stat().st_size for path, _ in files)
  args.output.parent.mkdir(parents=True, exist_ok=True)
  if shutil.disk_usage(args.output.parent).free < (25 << 30) + growth:
    raise OSError('native GPU asset packaging requires 25 GiB free plus output growth')
  rows = []
  for source, target in files:
    destination = args.output / target
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, destination)
    rows.append({'path': str(target), 'bytes': source.stat().st_size, 'sha256': hashlib.sha256(source.read_bytes()).hexdigest()})
  manifest = {'version': 1, 'model_sha256': digest, 'files': rows,
              'dependency_boundary': 'Firmware and model weights remain external native assets; this package contains no Python runtime.'}
  (args.output / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
  print(json.dumps({'output': str(args.output), 'files': len(rows), 'bytes': growth}))


if __name__ == '__main__':
  main()
