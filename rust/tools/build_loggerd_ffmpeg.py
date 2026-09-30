"""Build pinned external FFmpeg libraries for a generic cross-build, not deployment."""
from __future__ import annotations

import argparse
from hashlib import sha256
import json
from pathlib import Path
import subprocess
import tarfile
import urllib.request

VERSION = '6.1.1'
SHA256 = '8684f4b00f94b85461884c3719382f1261f0d9eb3d59640a1f4ac0873616f968'
URL = f'https://ffmpeg.org/releases/ffmpeg-{VERSION}.tar.xz'


def build(root: Path, cc: str, ar: str, ranlib: str, jobs: int) -> Path:
  root.mkdir(parents=True, exist_ok=True)
  archive = root / f'ffmpeg-{VERSION}.tar.xz'
  if not archive.exists():
    with urllib.request.urlopen(URL, timeout=120) as response, archive.open('wb') as output:
      while chunk := response.read(1024 * 1024):
        output.write(chunk)
  if sha256(archive.read_bytes()).hexdigest() != SHA256:
    raise ValueError('FFmpeg source checksum mismatch')
  source = root / f'ffmpeg-{VERSION}'
  if not source.exists():
    with tarfile.open(archive) as archive_file:
      archive_file.extractall(root, filter='data')
  prefix = root / 'install'
  arguments = [
    str(source / 'configure'), f'--prefix={prefix}', '--arch=aarch64', '--target-os=linux', '--enable-cross-compile',
    f'--cc={cc}', f'--ar={ar}', f'--ranlib={ranlib}', '--enable-static', '--disable-shared', '--enable-pic',
    '--disable-autodetect', '--disable-everything', '--disable-debug', '--disable-doc', '--disable-programs',
    '--disable-avdevice', '--disable-avfilter', '--disable-swresample', '--disable-swscale', '--disable-postproc',
    '--enable-avcodec', '--enable-avformat', '--enable-avutil', '--enable-encoder=aac,ffvhuff',
    '--enable-muxer=mpegts,matroska', '--enable-protocol=file', '--enable-parser=h264',
    '--enable-bsf=h264_mp4toannexb,hevc_mp4toannexb,aac_adtstoasc',
  ]
  (root / 'invocation.json').write_text(json.dumps({'url': URL, 'source_sha256': SHA256, 'configure': arguments}, indent=2) + '\n')
  work = root / 'build'
  work.mkdir(exist_ok=True)
  with (root / 'build.log').open('wb') as log:
    for command in (arguments, ['make', f'-j{jobs}'], ['make', 'install']):
      subprocess.run(command, cwd=work, stdout=log, stderr=subprocess.STDOUT, check=True)
  return prefix


if __name__ == '__main__':
  parser = argparse.ArgumentParser()
  parser.add_argument('--root', type=Path, required=True)
  parser.add_argument('--cc', required=True)
  parser.add_argument('--ar', required=True)
  parser.add_argument('--ranlib', required=True)
  parser.add_argument('--jobs', type=int, default=2)
  args = parser.parse_args()
  print(build(args.root.resolve(), args.cc, args.ar, args.ranlib, args.jobs))
