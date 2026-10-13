#!/usr/bin/env python3
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Build the combined pinned ARM OpenCV provider; run with --help for paths."""
from __future__ import annotations

import argparse
from dataclasses import dataclass
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
from typing import Final

from build_webcam_opencv import configure, source_hashes, verify_source
from build_xiaoge_opencv import ARCHIVE_SHA256, BuildDirectory, PATCH_SHA256, REVISION, guard

LIBRARIES: Final = ('core', 'imgproc', 'imgcodecs', 'videoio', 'dnn')


@dataclass(frozen=True, slots=True)
class Providers:
  ffmpeg: Path
  sdk: Path
  cross: Path
  compiler_root: Path


def run(command: list[str], directory: BuildDirectory, phase: str, environment: dict[str, str]) -> None:
  free = guard(directory.path, directory.growth)
  (directory.path / f'{phase}.command.json').write_text(json.dumps(command, indent=2) + '\n')
  (directory.path / f'{phase}.space.json').write_text(json.dumps({'free_bytes': free, 'growth_bytes': directory.growth}) + '\n')
  with (directory.path / f'{phase}.log').open('w') as log:
    subprocess.run(command, check=True, env=environment, stdout=log, stderr=subprocess.STDOUT)


def toolchain(providers: Providers, output: Path) -> Path:
  path = output / 'aarch64.cmake'
  roots = ';'.join(str(p) for p in (providers.ffmpeg, providers.sdk, providers.compiler_root))
  link = ' '.join(f'-Wl,-rpath-link,{p}' for p in
                  (providers.ffmpeg / 'lib', providers.sdk / 'usr/lib/aarch64-linux-gnu', providers.cross / 'agnos/libraries'))
  path.write_text(f'''set(CMAKE_SYSTEM_NAME Linux)
set(CMAKE_SYSTEM_PROCESSOR aarch64)
set(CMAKE_C_COMPILER "{providers.cross / 'cc.sh'}")
set(CMAKE_CXX_COMPILER "{providers.cross / 'cxx.sh'}")
set(CMAKE_FIND_ROOT_PATH "{roots}")
set(CMAKE_FIND_ROOT_PATH_MODE_PROGRAM NEVER)
set(CMAKE_FIND_ROOT_PATH_MODE_LIBRARY ONLY)
set(CMAKE_FIND_ROOT_PATH_MODE_INCLUDE ONLY)
set(CMAKE_FIND_ROOT_PATH_MODE_PACKAGE ONLY)
set(CMAKE_SHARED_LINKER_FLAGS_INIT "{link}")
set(CMAKE_EXE_LINKER_FLAGS_INIT "{link}")
''')
  return path


def build(source: Path, directory: BuildDirectory, providers: Providers) -> None:
  verify_source(source)
  identity = json.loads((source.parent / 'source.json').read_text())
  if identity['sha256'] != ARCHIVE_SHA256:
    raise RuntimeError('source archive identity differs from the pinned provider')
  patch = Path(__file__).resolve().parents[1] / 'crates/opencv-runtime/native/opencv-defined-access.patch'
  if hashlib.sha256(patch.read_bytes()).hexdigest() != PATCH_SHA256:
    raise RuntimeError('source hardening patch identity differs')
  output = directory.path
  output.mkdir(parents=True, exist_ok=True)
  sdk_include = output / 'target-include'
  if sdk_include.is_symlink():
    if sdk_include.resolve(strict=True) != providers.sdk / 'usr/include':
      raise RuntimeError('target header alias conflicts with the configured SDK')
  else:
    sdk_include.symlink_to(providers.sdk / 'usr/include', target_is_directory=True)
  before = source_hashes(source)
  (output / 'source-files.json').write_text(json.dumps(before, indent=2) + '\n')
  environment = os.environ.copy()
  environment.update({'PKG_CONFIG_LIBDIR': str(providers.ffmpeg / 'lib/pkgconfig'),
                      'PKG_CONFIG_PATH': '', 'PKG_CONFIG_SYSROOT_DIR': ''})
  (output / 'pkg-config-environment.json').write_text(json.dumps({k: environment[k] for k in
    ('PKG_CONFIG_LIBDIR', 'PKG_CONFIG_PATH', 'PKG_CONFIG_SYSROOT_DIR')}, indent=2) + '\n')
  run(['pkg-config', '--modversion', 'libavcodec', 'libavformat', 'libavutil', 'libswscale'], directory, 'ffmpeg-versions', environment)
  command = [arg for arg in configure(source, output) if arg not in
             ('-DBUILD_LIST=core,imgproc,imgcodecs,videoio', '-DWITH_PROTOBUF=OFF')]
  command += ['-DBUILD_LIST=' + ','.join(LIBRARIES), '-DBUILD_PROTOBUF=ON', '-DWITH_PROTOBUF=ON',
              '-DOPENCV_DNN_CUDA=OFF', '-DCMAKE_TOOLCHAIN_FILE=' + str(toolchain(providers, output)),
              '-DCMAKE_SKIP_RPATH=ON', '-DOPENCV_FFMPEG_ENABLE_LIBAVDEVICE=OFF',
              '-DZLIB_INCLUDE_DIR:PATH=' + str(sdk_include)]
  run(command, directory, 'configure', environment)
  log = (output / 'configure.log').read_text()
  if re.search(r'FFMPEG:\s+YES', log) is None or re.search(r'v4l/v4l2:\s+YES', log) is None:
    raise RuntimeError('combined provider requires target FFmpeg and V4L2 capture backends')
  if re.search(r'To be built:\s+core dnn imgcodecs imgproc videoio', log) is None:
    raise RuntimeError('combined provider did not enable precisely the required five libraries')
  flags = (output / 'build/modules/core/CMakeFiles/opencv_core.dir/flags.make').read_text()
  if str(sdk_include) not in flags:
    raise RuntimeError('OpenCV omitted the relocated target zlib header include directory')
  run(['cmake', '--build', str(output / 'build'), '--parallel', '2'], directory, 'build', environment)
  run(['cmake', '--install', str(output / 'build')], directory, 'install', environment)
  if before != source_hashes(source):
    raise RuntimeError('external build changed its read-only source tree')
  libraries = sorted((output / 'install/lib').glob('libopencv_*.so.4.13.0'))
  if {p.name for p in libraries} != {f'libopencv_{name}.so.4.13.0' for name in LIBRARIES}:
    raise RuntimeError('unexpected combined runtime libraries')
  licenses = output / 'licenses'
  licenses.mkdir(exist_ok=True)
  for name, relative in (
    ('OpenCV-Apache-2.0.txt', 'LICENSE'), ('OpenCV-COPYRIGHT.txt', 'COPYRIGHT'),
    ('protobuf-BSD-3-Clause.txt', '3rdparty/protobuf/LICENSE'),
    ('flatbuffers-Apache-2.0.txt', '3rdparty/flatbuffers/LICENSE.txt'), ('zlib-License.txt', '3rdparty/zlib/LICENSE'),
    ('libjpeg-turbo-licenses.md', '3rdparty/libjpeg-turbo/LICENSE.md'), ('libjpeg-turbo-README.ijg', '3rdparty/libjpeg-turbo/README.ijg'),
  ):
    shutil.copyfile(source / relative, licenses / name)
  carotene = (source / 'hal/carotene/src/common.hpp').read_text()
  (licenses / 'carotene-BSD-3-Clause.txt').write_text(carotene[:carotene.index('*/') + 2] + '\n')
  kleidicv = output / 'build/3rdparty/kleidicv/kleidicv-0.7.0/LICENSES/Apache-2.0.txt'
  shutil.copyfile(kleidicv, licenses / 'KleidiCV-Apache-2.0.txt')
  downloaded = output / 'downloads/kleidicv/e8f94e427bd78a745afa5c8cd073b416-kleidicv-0.7.0.tar.gz'
  (licenses / 'external-hal-provenance.json').write_text(json.dumps({
    'carotene_notice_source': 'hal/carotene/src/common.hpp',
    'carotene_notice_source_sha256': before['hal/carotene/src/common.hpp'],
    'kleidicv_version': '0.7.0', 'kleidicv_archive_sha256': hashlib.sha256(downloaded.read_bytes()).hexdigest(),
    'kleidicv_license_sha256': hashlib.sha256(kleidicv.read_bytes()).hexdigest(),
    'discovery': 'Pinned OpenCV hal/kleidicv dependency recipe; recorded external ARM acceleration providers.',
  }, indent=2) + '\n')
  shutil.copyfile(patch, output / patch.name)
  shutil.copyfile(source.parent / 'source.json', output / 'source.json')
  retained = [*sorted((output / 'install/lib').glob('libopencv_*.so*')),
              *sorted((output / 'install/include/opencv4').rglob('*.h*')), *sorted(licenses.iterdir()),
              output / 'source.json', output / patch.name, output / 'source-files.json', output / 'aarch64.cmake']
  hashes = {str(p.relative_to(output)): hashlib.sha256(p.read_bytes()).hexdigest() for p in retained}
  (output / 'native-libraries.sha256').write_text(''.join(f'{value}  {path}\n' for path, value in hashes.items()))
  (output / 'receipt.json').write_text(json.dumps({'status': 'BUILT', 'architecture': 'aarch64',
    'source_revision': REVISION, 'source_archive_sha256': ARCHIVE_SHA256, 'hardening_patch_sha256': PATCH_SHA256,
    'source_unchanged': True, 'libraries': {str(p.relative_to(output)): hashes[str(p.relative_to(output))] for p in libraries},
    'sealed': hashes, 'limits': 'External provider build only; ABI and real consumer validation are separate.'}, indent=2) + '\n')


def main() -> None:
  parser = argparse.ArgumentParser(description=__doc__)
  for name in ('source', 'output', 'ffmpeg', 'sdk', 'cross', 'compiler-root'):
    parser.add_argument('--' + name, type=Path, required=True)
  args = parser.parse_args()
  source, output = args.source.resolve(strict=True), args.output.resolve()
  directory = BuildDirectory(output, 3 * 1024**3)
  guard(output, directory.growth)
  providers = Providers(*(p.resolve(strict=True) for p in (args.ffmpeg, args.sdk, args.cross, args.compiler_root)))
  build(source, directory, providers)


if __name__ == '__main__':
  main()
