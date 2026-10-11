#!/usr/bin/env python3
"""Build the pinned webcam capture provider from an unchanged owned source tree."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil

from build_xiaoge_opencv import BuildDirectory, PATCHED_FILES, PATCH_SHA256, REVISION, SourceHardeningMismatch, guard, run

LIBRARIES = ('core', 'imgproc', 'imgcodecs', 'videoio')


def source_hashes(source: Path) -> dict[str, str]:
  return {str(path.relative_to(source)): hashlib.sha256(path.read_bytes()).hexdigest() for path in sorted(source.rglob('*')) if path.is_file()}


def verify_source(source: Path) -> None:
  for name, (_, expected) in PATCHED_FILES.items():
    actual = hashlib.sha256((source / name).read_bytes()).hexdigest()
    if actual != expected:
      raise SourceHardeningMismatch(source / name, actual)
  original = json.loads((source.parent / 'source.json').read_text())
  if original['revision'] != REVISION or original['external_hardening_patch_sha256'] != PATCH_SHA256:
    raise ValueError('source does not identify the pinned, hardened OpenCV provider')


def configure(source: Path, output: Path) -> list[str]:
  return [
    'cmake',
    '-S',
    str(source),
    '-B',
    str(output / 'build'),
    '-DCMAKE_BUILD_TYPE=Release',
    '-DBUILD_LIST=core,imgproc,imgcodecs,videoio',
    '-DBUILD_SHARED_LIBS=ON',
    '-DCMAKE_POSITION_INDEPENDENT_CODE=ON',
    '-DBUILD_TESTS=OFF',
    '-DBUILD_PERF_TESTS=OFF',
    '-DBUILD_EXAMPLES=OFF',
    '-DBUILD_opencv_apps=OFF',
    '-DBUILD_opencv_python3=OFF',
    '-DBUILD_opencv_python2=OFF',
    '-DBUILD_JAVA=OFF',
    '-DWITH_IPP=OFF',
    '-DWITH_OPENCL=OFF',
    '-DWITH_LAPACK=OFF',
    '-DWITH_TBB=OFF',
    '-DWITH_ITT=OFF',
    '-DWITH_PROTOBUF=OFF',
    '-DWITH_ADE=OFF',
    '-DWITH_GSTREAMER=OFF',
    '-DWITH_GTK=OFF',
    '-DWITH_QT=OFF',
    '-DWITH_1394=OFF',
    '-DWITH_OBSENSOR=OFF',
    '-DWITH_V4L=ON',
    '-DWITH_FFMPEG=ON',
    '-DWITH_JPEG=ON',
    '-DBUILD_JPEG=ON',
    '-DWITH_PNG=OFF',
    '-DWITH_TIFF=OFF',
    '-DWITH_WEBP=OFF',
    '-DWITH_OPENJPEG=OFF',
    '-DWITH_OPENEXR=OFF',
    '-DWITH_AVIF=OFF',
    '-DWITH_JPEGXL=OFF',
    '-DVIDEOIO_ENABLE_PLUGINS=OFF',
    '-DCMAKE_INSTALL_LIBDIR=lib',
    '-DOPENCV_DOWNLOAD_PATH=' + str(output / 'downloads'),
    '-DCMAKE_INSTALL_PREFIX=' + str(output / 'install'),
  ]


def build(source: Path, directory: BuildDirectory) -> None:
  verify_source(source)
  before = source_hashes(source)
  output = directory.path
  output.mkdir(parents=True, exist_ok=True)
  (output / 'source-files.json').write_text(json.dumps(before, indent=2) + '\n')
  run(configure(source, output), directory, 'configure')
  log = (output / 'configure.log').read_text()
  if re.search(r'FFMPEG:\s+YES', log) is None or re.search(r'v4l/v4l2:\s+YES', log) is None:
    raise RuntimeError('OpenCV configuration did not enable the required FFmpeg and V4L2 capture backends')
  run(['cmake', '--build', str(output / 'build'), '--parallel', '2'], directory, 'build')
  run(['cmake', '--install', str(output / 'build')], directory, 'install')
  after = source_hashes(source)
  if before != after:
    raise RuntimeError('external provider build changed its read-only source tree')
  libraries = sorted((output / 'install/lib').glob('libopencv_*.so.4.13.0'))
  if {path.name for path in libraries} != {f'libopencv_{name}.so.4.13.0' for name in LIBRARIES}:
    raise RuntimeError(f'unexpected webcam provider libraries: {libraries}')
  licenses = output / 'licenses'
  licenses.mkdir(exist_ok=True)
  for name, path in (
    ('OpenCV-Apache-2.0.txt', source / 'LICENSE'),
    ('libjpeg-turbo-licenses.md', source / '3rdparty/libjpeg-turbo/LICENSE.md'),
    ('zlib-License.txt', source / '3rdparty/zlib/LICENSE'),
  ):
    shutil.copyfile(path, licenses / name)
  retained = [
    *sorted((output / 'install/lib').glob('libopencv_*.so*')),
    *sorted((output / 'install/include/opencv4').rglob('*.h*')),
    *sorted(licenses.iterdir()),
    output / 'source-files.json',
  ]
  hashes = {str(path.relative_to(output)): hashlib.sha256(path.read_bytes()).hexdigest() for path in retained}
  (output / 'native-libraries.sha256').write_text(''.join(f'{digest}  {path}\n' for path, digest in hashes.items()))
  (output / 'receipt.json').write_text(
    json.dumps(
      {
        'status': 'PASS',
        'source': str(source),
        'source_revision': REVISION,
        'external_hardening_patch_sha256': PATCH_SHA256,
        'source_unchanged': True,
        'libraries': {str(path.relative_to(output)): hashes[str(path.relative_to(output))] for path in libraries},
        'sealed': hashes,
      },
      indent=2,
    )
    + '\n'
  )


def main() -> None:
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument('--source', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--growth-mib', type=int, default=2048)
  args = parser.parse_args()
  source, output = args.source.resolve(strict=True), args.output.resolve()
  growth = args.growth_mib * 1024**2
  guard(output, growth)
  build(source, BuildDirectory(output, growth))


if __name__ == '__main__':
  main()
