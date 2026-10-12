from __future__ import annotations

import argparse
from dataclasses import asdict, dataclass
import hashlib
import json
from pathlib import Path, PurePosixPath
import shutil
import stat
import subprocess
import tarfile
import tomllib
from typing import Final, TypedDict
import zipfile

from card_qa.ci import ROOT, require_space


@dataclass(frozen=True)
class Package:
  name: str
  version: str
  commit: str
  sha256: str
  size: int

  @property
  def filename(self) -> str:
    return f'{self.name}-{self.version}-py3-none-linux_aarch64.whl'

  @property
  def url(self) -> str:
    return f'https://github.com/commaai/dependencies/releases/download/{self.name}/v{self.version}/{self.filename}'

  @property
  def prefix(self) -> str:
    return f'{self.name}-{self.version}.data/purelib/{self.name}/install/'


PACKAGES = (
  Package('ffmpeg', '7.1.0', 'b9732165bcf5a3fab83b05994187802a0d115b6e',
          'ce758b64c0343574e18ab97cbcff1e29868c66ccc5e0c866b5970266451203d4', 11743600),
  Package('libyuv', '1922.0', '28c3c2a2444232aeeaf989c33fd333ce74e6fc90',
          '1659e55a357f732836e5ed2a17fa65e715a11bf9bfa6d305e71fd7ac00a8e475', 267013),
)

SWSCALE_VERSION: Final = '7.1'
SWSCALE_SHA256: Final = '40973d44970dbc83ef302b0609f2e74982be2d85916dd2ee7472d30678a7abe6'
SWSCALE_URL: Final = f'https://ffmpeg.org/releases/ffmpeg-{SWSCALE_VERSION}.tar.xz'


class SwscaleReceipt(TypedDict):
  source_version: str
  source_sha256: str
  url: str
  license: str
  commands: list[list[str]]
  files: dict[str, str]


class SwscaleProviderError(ValueError):
  def __init__(self, path: Path, reason: str) -> None:
    self.path = path
    self.reason = reason
    super().__init__(f'{path}: {reason}')


def verify_swscale_headers(source: Path, install: Path) -> None:
  for name in ('version.h', 'pixfmt.h'):
    header = install / 'include/libavutil' / name
    if header.read_bytes() != (source / 'libavutil' / name).read_bytes():
      raise SwscaleProviderError(header, 'avutil header differs from the pinned scaler source')


def build_swscale(output: Path, install: Path, source_archive: Path | None,
                  cc: str, ar: str, ranlib: str) -> SwscaleReceipt:
  """Add only the missing scaler to the unchanged, verified FFmpeg 7.1 wheel."""
  require_space(output, 512 * 1024**2)
  output.mkdir(parents=True, exist_ok=False)
  archive = output / f'ffmpeg-{SWSCALE_VERSION}.tar.xz'
  if source_archive:
    shutil.copyfile(source_archive, archive)
  else:
    subprocess.run(['curl', '--retry', '3', '--connect-timeout', '20', '--max-time', '180', '-fsSL',
                    SWSCALE_URL, '-o', str(archive)], check=True)
  with archive.open('rb') as stream:
    if hashlib.file_digest(stream, 'sha256').hexdigest() != SWSCALE_SHA256:
      raise SwscaleProviderError(archive, 'source SHA256 differs from pinned FFmpeg 7.1')
  with tarfile.open(archive) as source_tar:
    source_tar.extractall(output, filter='data')
  source = output / f'ffmpeg-{SWSCALE_VERSION}'
  verify_swscale_headers(source, install)
  work = output / 'build'
  work.mkdir()
  configure = [str(source / 'configure'), f'--prefix={install}', '--arch=aarch64', '--target-os=linux',
               '--enable-cross-compile', f'--cc={cc}', f'--ar={ar}', f'--ranlib={ranlib}',
               '--enable-static', '--disable-shared', '--enable-pic', '--disable-autodetect',
               '--disable-everything', '--disable-debug', '--disable-doc', '--disable-programs',
               '--disable-avcodec', '--disable-avformat', '--disable-avdevice', '--disable-avfilter',
               '--disable-swresample', '--disable-postproc', '--enable-avutil', '--enable-swscale']
  receipt: SwscaleReceipt = {'source_version': SWSCALE_VERSION, 'source_sha256': SWSCALE_SHA256,
                            'url': SWSCALE_URL, 'license': 'LGPL-2.1-or-later',
                            'commands': [configure, ['make', '-j2', 'libswscale/libswscale.a']], 'files': {}}
  record = output / 'receipt.json'
  record.write_text(json.dumps(receipt, indent=2) + '\n')
  with (output / 'build.log').open('wb') as log:
    for command in receipt['commands']:
      require_space(output, 512 * 1024**2)
      subprocess.run(command, cwd=work, stdout=log, stderr=subprocess.STDOUT, check=True)
  require_space(output, 16 * 1024**2)
  files = [(work / 'libswscale/libswscale.a', 'lib/libswscale.a'),
           *((source / 'libswscale' / name, f'include/libswscale/{name}')
             for name in ('swscale.h', 'version.h', 'version_major.h')),
           *((source / name, f'share/licenses/swscale/{name}') for name in ('COPYING.LGPLv2.1', 'LICENSE.md'))]
  for origin, relative in files:
    destination = install / relative
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(origin, destination)
    receipt['files'][relative] = hashlib.sha256(destination.read_bytes()).hexdigest()
  record.write_text(json.dumps(receipt, indent=2) + '\n')
  return receipt


def verify_lock(lock: dict, package: Package) -> None:
  matches = [row for row in lock['package'] if row['name'] == package.name]
  if len(matches) != 1 or matches[0]['version'] != package.version or not matches[0]['source'].get('git', '').endswith('#' + package.commit):
    raise ValueError(f'{package.name}: locked source differs from the reviewed native payload')


def verify_archive(archive: Path, package: Package) -> None:
  with archive.open('rb') as stream:
    digest = hashlib.file_digest(stream, 'sha256').hexdigest()
  if digest != package.sha256 or archive.stat().st_size != package.size:
    raise ValueError(f'{package.name}: native wheel SHA256/size differs')


def extract_native(archive: Path, package: Package, output: Path) -> list[dict[str, str | int]]:
  with zipfile.ZipFile(archive) as wheel:
    members = [member for member in wheel.infolist() if member.filename.startswith(package.prefix) and not member.is_dir()]
    if not members or sum(member.file_size for member in members) > 256 * 1024**2:
      raise ValueError(f'{package.name}: missing or oversized native payload')
    seen = set()
    for member in members:
      relative = PurePosixPath(member.filename.removeprefix(package.prefix))
      if relative.is_absolute() or '..' in relative.parts or stat.S_ISLNK(member.external_attr >> 16) or relative in seen:
        raise ValueError(f'unsafe native archive member: {member.filename}')
      seen.add(relative)
    output.mkdir(parents=True, exist_ok=False)
    records = []
    for member in members:
      relative = member.filename.removeprefix(package.prefix)
      target = output / relative
      target.parent.mkdir(parents=True, exist_ok=True)
      data = wheel.read(member)
      target.write_bytes(data)
      if relative in ('bin/ffmpeg', 'bin/ffprobe'):
        target.chmod(0o755)
      records.append({'path': relative, 'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest()})
    return records


def main() -> None:
  parser = argparse.ArgumentParser(description='Stage pinned native ARM codec payloads without installing Python launchers')
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--archive-directory', type=Path, help='use already downloaded exact wheel filenames')
  parser.add_argument('--swscale-archive', type=Path, help='use the verified FFmpeg 7.1 scaler source archive')
  parser.add_argument('--cc', default='aarch64-linux-gnu-gcc')
  parser.add_argument('--ar', default='aarch64-linux-gnu-ar')
  parser.add_argument('--ranlib', default='aarch64-linux-gnu-ranlib')
  args = parser.parse_args()
  output = args.output.resolve()
  space = require_space(output, 512 * 1024**2)
  output.mkdir(parents=True, exist_ok=False)
  (output / 'space.json').write_text(json.dumps(space) + '\n')
  lock = tomllib.loads((ROOT / 'uv.lock').read_text())
  result = {}
  for package in PACKAGES:
    verify_lock(lock, package)
    archive = output / package.filename
    if args.archive_directory:
      shutil.copyfile(args.archive_directory / package.filename, archive)
    else:
      command = ['curl', '--retry', '3', '--connect-timeout', '20', '--max-time', '180', '-fsSL', package.url, '-o', str(archive)]
      (output / f'{package.name}-download.json').write_text(json.dumps(command) + '\n')
      subprocess.run(command, check=True)
    verify_archive(archive, package)
    install = output / package.name / 'install'
    records = extract_native(archive, package, install)
    result[package.name] = {**asdict(package), 'url': package.url, 'install': str(install), 'files': records}
  result['swscale'] = build_swscale(output / 'swscale', output / 'ffmpeg/install',
                                   args.swscale_archive, args.cc, args.ar, args.ranlib)
  required = [*(output / 'ffmpeg/install/lib' / f'lib{name}.a' for name in
               ('avcodec', 'avformat', 'avutil', 'swscale', 'swresample', 'x264', 'z', 'va', 'va-drm', 'drm')),
              output / 'ffmpeg/install/include/libswscale/swscale.h',
              output / 'ffmpeg/install/include/libavcodec/avcodec.h', output / 'libyuv/install/lib/libyuv.a',
              output / 'libyuv/install/include/libyuv.h']
  for path in required:
    if not path.is_file():
      raise FileNotFoundError(path)
  (output / 'receipt.json').write_text(json.dumps(result, indent=2) + '\n')
  print(json.dumps({'status': 'PASS', 'FFMPEG_DIR': result['ffmpeg']['install'],
                  'ENCODER_LIBYUV_LIB': str(Path(result['libyuv']['install']) / 'lib')}))


if __name__ == '__main__':
  main()
