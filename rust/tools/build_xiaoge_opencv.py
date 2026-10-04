#!/usr/bin/env python3
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# Run with --archive reference/opencv.tar.gz --output a bounded native build directory.
from __future__ import annotations

import argparse
from dataclasses import dataclass
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import urllib.request

REVISION = 'fe38fc608f6acb8b68953438a62305d8318f4fcd'
ARCHIVE_SHA256 = '6a7508554941c1a698c243b2212b2985ce59a65a3e0348d53c2158607d801e61'
URL = f'https://codeload.github.com/opencv/opencv/tar.gz/{REVISION}'
PATCH_SHA256 = '7d17439697d9786b43b2a44dc9eecea34381adb6d2f14c6892266b6699a49aec'
PATCHED_FILES = {
  'modules/core/include/opencv2/core/hal/intrin_sse.hpp': (
    '9ea43239afc6dd6915c37715074d8d6351a055b3890aa71fb644864dcb9ececd',
    'dcd1c37f0c3817c176953e5c15037497954e1ff523467e3379fcb1e0d3d31ffb'),
  'modules/imgproc/src/drawing.cpp': (
    'feb12ebea7b08419289a46d043afeee665b77217d1faed89d86ad30d2d8af0e0',
    'bed85690b61fa5060708fa6860062c60f2bc4b5f5d5a738748aa3c0df033f99f'),
}


@dataclass(frozen=True, slots=True)
class BuildDirectory:
  path: Path
  growth: int


class SourceArchiveMismatch(ValueError):
  def __init__(self, actual: str) -> None:
    self.expected = ARCHIVE_SHA256
    self.actual = actual
    super().__init__(f'OpenCV source archive SHA256 mismatch: expected={self.expected}, actual={actual}')


class UnexpectedLibraries(RuntimeError):
  def __init__(self, paths: list[Path]) -> None:
    self.paths = tuple(paths)
    super().__init__(f'expected core/imgproc/dnn libraries, found {paths}')


class SourceHardeningMismatch(ValueError):
  def __init__(self, path: Path, actual: str) -> None:
    self.path = path
    self.actual = actual
    super().__init__(f'pinned OpenCV hardening input/output mismatch: {path}, sha256={actual}')


def guard(output: Path, growth: int) -> int:
  parent = output
  while not parent.exists():
    parent = parent.parent
  free = shutil.disk_usage(parent).free
  if free < 25 * 1024**3 + growth:
    raise OSError(f'require 25 GiB plus {growth} bytes; free={free}; recover 35 GiB before resuming')
  return free


def run(command: list[str], output: BuildDirectory, phase: str) -> None:
  free = guard(output.path, output.growth)
  (output.path / f'{phase}.command.json').write_text(json.dumps(command, indent=2) + '\n')
  (output.path / f'{phase}.space.json').write_text(json.dumps({'free_bytes': free, 'growth_bytes': output.growth}) + '\n')
  with (output.path / f'{phase}.log').open('w') as log:
    subprocess.run(command, check=True, stdout=log, stderr=subprocess.STDOUT)


def harden(source: Path, directory: BuildDirectory) -> None:
  patch = Path(__file__).resolve().parents[1] / 'crates/opencv-runtime/native/opencv-defined-access.patch'
  digest = hashlib.sha256(patch.read_bytes()).hexdigest()
  if digest != PATCH_SHA256:
    raise SourceHardeningMismatch(patch, digest)
  actual = {name: hashlib.sha256((source / name).read_bytes()).hexdigest() for name in PATCHED_FILES}
  if all(actual[name] == pair[0] for name, pair in PATCHED_FILES.items()):
    run(['patch', '--batch', '--forward', '-p1', '-d', str(source), '-i', str(patch)], directory, 'hardening')
  for name, pair in PATCHED_FILES.items():
    digest = hashlib.sha256((source / name).read_bytes()).hexdigest()
    if digest != pair[1]:
      raise SourceHardeningMismatch(source / name, digest)
  shutil.copyfile(patch, directory.path / patch.name)
  (directory.path / 'hardening.json').write_text(json.dumps({'patch_sha256': PATCH_SHA256,
    'files': {name: {'original': pair[0], 'patched': pair[1]} for name, pair in PATCHED_FILES.items()}}, indent=2) + '\n')


def main() -> None:
  parser = argparse.ArgumentParser(description='Build pinned native OpenCV core, imgproc, and DNN libraries')
  parser.add_argument('--output', required=True, type=Path)
  parser.add_argument('--archive', type=Path)
  parser.add_argument('--prepare-only', action='store_true')
  parser.add_argument('--toolchain', type=Path)
  parser.add_argument('--sanitizers', choices=('address,undefined',))
  args = parser.parse_args()
  output = args.output.resolve()
  growth = (3 if args.sanitizers else 2) * 1024**3
  free = guard(output, growth)
  output.mkdir(parents=True, exist_ok=True)
  archive = args.archive or output / 'opencv.tar.gz'
  if not archive.exists():
    with urllib.request.urlopen(URL, timeout=90) as response, archive.open('xb') as target:
      shutil.copyfileobj(response, target)
  digest = hashlib.sha256(archive.read_bytes()).hexdigest()
  if digest != ARCHIVE_SHA256:
    raise SourceArchiveMismatch(digest)
  source = output / f'opencv-{REVISION}'
  if not source.exists():
    with tarfile.open(archive) as package:
      package.extractall(output, filter='data')
  directory = BuildDirectory(output, growth)
  harden(source, directory)
  (output / 'source.json').write_text(json.dumps({'revision': REVISION, 'sha256': digest, 'url': URL,
    'external_hardening_patch_sha256': PATCH_SHA256,
    'free_before': free, 'python_reference_revision': 'b4c5ec4042f097e2a5b386b9d413ec7333d0a184',
    'reference_difference': 'only modules/python/test/test_fitline.py and test_mser.py; native source identical'}, indent=2) + '\n')
  if args.prepare_only:
    return
  build = output / 'build'
  command = ['cmake', '-S', str(source), '-B', str(build), '-DCMAKE_BUILD_TYPE=' + ('RelWithDebInfo' if args.sanitizers else 'Release'),
    '-DBUILD_LIST=core,imgproc,dnn', '-DBUILD_SHARED_LIBS=ON', '-DCMAKE_POSITION_INDEPENDENT_CODE=ON',
    '-DBUILD_TESTS=OFF', '-DBUILD_PERF_TESTS=OFF', '-DBUILD_EXAMPLES=OFF', '-DBUILD_opencv_apps=OFF',
    '-DBUILD_opencv_python3=OFF', '-DBUILD_opencv_python2=OFF', '-DBUILD_JAVA=OFF',
    '-DWITH_IPP=OFF', '-DWITH_OPENCL=OFF', '-DWITH_LAPACK=OFF', '-DWITH_TBB=OFF', '-DWITH_ITT=OFF',
    '-DOPENCV_DNN_CUDA=OFF', '-DBUILD_PROTOBUF=ON', '-DWITH_PROTOBUF=ON', '-DCMAKE_INSTALL_LIBDIR=lib',
    '-DCMAKE_INSTALL_PREFIX=' + str(output / 'install')]
  if args.toolchain:
    command.append('-DCMAKE_TOOLCHAIN_FILE=' + str(args.toolchain.resolve()))
  if args.sanitizers:
    flags = '-fsanitize=' + args.sanitizers + ' -fno-omit-frame-pointer'
    linker = flags + (' -shared-libasan' if 'clang' in os.environ.get('CXX', '') else '')
    command += ['-DCMAKE_C_FLAGS=' + flags, '-DCMAKE_CXX_FLAGS=' + flags, '-DCMAKE_SHARED_LINKER_FLAGS=' + linker]
  run(command, directory, 'configure')
  run(['cmake', '--build', str(build), '--parallel', '2'], directory, 'build')
  run(['cmake', '--install', str(build)], directory, 'install')
  libraries = sorted((output / 'install/lib').glob('libopencv_*.so.4.13.0'))
  if len(libraries) != 3:
    raise UnexpectedLibraries(libraries)
  licenses = output / 'licenses'
  licenses.mkdir(exist_ok=True)
  for name, path in (('opencv-Apache-2.0.txt', source / 'LICENSE'),
                     ('protobuf-BSD-3-Clause.txt', source / '3rdparty/protobuf/LICENSE'),
                     ('flatbuffers-Apache-2.0.txt', source / '3rdparty/flatbuffers/LICENSE.txt'),
                     ('zlib-License.txt', source / '3rdparty/zlib/LICENSE')):
    shutil.copyfile(path, licenses / name)
  retained = [*sorted((output / 'install/lib').glob('libopencv_*.so*')),
    *sorted((output / 'install/include/opencv4').rglob('*.h*')), *sorted(licenses.iterdir()), output / 'source.json',
    output / 'hardening.json', output / 'opencv-defined-access.patch', *[source / name for name in PATCHED_FILES]]
  hashes = {str(path.relative_to(output)): hashlib.sha256(path.read_bytes()).hexdigest() for path in retained}
  (output / 'native-libraries.sha256').write_text(''.join(f'{digest}  {path}\n' for path, digest in hashes.items()))
  (output / 'receipt.json').write_text(json.dumps({'status': 'PASS', 'source_revision': REVISION,
    'source_archive_sha256': digest, 'external_hardening_patch_sha256': PATCH_SHA256,
    'sanitizers': args.sanitizers, 'toolchain': str(args.toolchain) if args.toolchain else None,
    'libraries': {str(path.relative_to(output)): hashes[str(path.relative_to(output))] for path in libraries},
    'sealed': hashes}, indent=2) + '\n')


if __name__ == '__main__':
  main()
