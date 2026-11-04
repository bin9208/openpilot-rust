#!/usr/bin/env python3
"""Actions-only independent Cargo tree comparison, never a build acceptance."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--cargo', type=Path, required=True)
  parser.add_argument('--root', type=Path, required=True)
  parser.add_argument('--source-commit', required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  source = subprocess.check_output(['git', '-C', args.root, 'rev-parse', 'HEAD'], text=True).strip()
  assert source == args.source_commit
  catalog_path = args.root/'rust/native-runtime-catalog.json'
  original = catalog_path.read_bytes()
  catalog = json.loads(original)
  assert catalog['target'] == 'aarch64-unknown-linux-gnu'
  args.output.mkdir()
  results = []
  for name, group in catalog['groups'].items():
    command = [str(args.cargo), 'tree', '--manifest-path', str(args.root/'rust/Cargo.toml'),
        '--locked', '--offline', '--target', catalog['target'], '--edges', 'normal,build',
        '--prefix', 'none', '--format', '{p}|{f}', '--color', 'never']
    for package in group['packages']:
      command += ['--package', package]
    if group['features']:
      command += ['--features', ','.join(group['features'])]
    tree = args.output/f'{name}-cargo-tree.txt'
    with tree.open('xb') as output, (args.output/f'{name}-cargo-tree.stderr').open('xb') as error:
      subprocess.run(command, cwd=args.root, stdout=output, stderr=error, check=True, timeout=180)
    assert tree.stat().st_size <= 8*1024**2
    active = {}
    for line in tree.read_text().splitlines():
      if '|' not in line:
        assert not line.strip() or line.strip().startswith('[build-dependencies]'), line
        continue
      package, features = line.split('|', 1)
      name_in_tree = package.split()[0]
      active.setdefault(name_in_tree, set()).update(value.strip() for value in features.removesuffix(' (*)').split(',') if value.strip())
    assert set(group['packages']) <= active.keys()
    required = {'OPENPILOT_CARGO_VENDOR': 'vendor'}
    if 'openpilot-cereal' in active:
      required['OPENPILOT_CAPNP'] = 'usr/bin/capnp'
    if 'ffmpeg-next' in active:
      required['FFMPEG_DIR'] = ''
      required['LIBCLANG_PATH'] = 'usr/lib/llvm-18/lib'
    for package, feature, environment, suffix in (
        ('openpilot-encoderd', 'native', 'ENCODER_LIBYUV_LIB', 'lib'),
        ('openpilot-startup-ui', 'native', 'STARTUP_UI_RAYLIB_ROOT', ''),
        ('openpilot-webcam', 'native', 'OPENPILOT_WEBCAM_OPENCV_ROOT', ''),
        ('openpilot-opencv-runtime', 'native-skip-miri', 'OPENPILOT_OPENCV_ROOT', ''),
        ('openpilot-locationd', 'solver', 'LOCATIOND_EIGEN_INCLUDE', ''),
        ('openpilot-paramsd', 'solver', 'PARAMSD_EIGEN_INCLUDE', '')):
      if feature in active.get(package, set()):
        required[environment] = suffix
    assert required == group['compiler_inputs'], (name, required, group['compiler_inputs'])
    results.append(dict(group=name, argv=command, cargo_tree_sha256=hashlib.sha256(tree.read_bytes()).hexdigest(),
        compiler_inputs=required, matches=True))
  assert catalog_path.read_bytes() == original
  assert subprocess.check_output(['git', '-C', args.root, 'rev-parse', 'HEAD'], text=True).strip() == source
  assert not subprocess.check_output(['git', '-C', args.root, 'status', '--porcelain', '--untracked-files=all'], text=True).strip()
  result = dict(schema=1, source_commit=source, catalog_sha256=hashlib.sha256(original).hexdigest(),
      groups=results, status='PASS', compiled=False, compiler_inputs_prepared=False,
      complete_build_dependency_equivalence=False, full_runtime_build=False, physical_device_execution=False)
  (args.output/'result.json').write_text(json.dumps(result, indent=2)+'\n')
  print(json.dumps(dict(status='PASS', groups=len(results), compiled=False, compiler_inputs_prepared=False)))


if __name__ == '__main__':
  main()
