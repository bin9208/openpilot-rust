#!/usr/bin/env python3
"""Exercise the admitted original FFmpeg tools, with owned input and child only."""
import argparse
import hashlib
import json
from pathlib import Path
import platform
import subprocess
import time


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--ffmpeg', type=Path, required=True)
  parser.add_argument('--ffprobe', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  assert platform.machine() == 'aarch64'
  args.output.mkdir()
  commands = []

  def run(argv, name):
    commands.append(list(map(str, argv)))
    with (args.output/name).open('wb') as output, (args.output/(name+'.stderr')).open('wb') as stderr:
      subprocess.run(list(map(str, argv)), stdout=output, stderr=stderr, check=True, timeout=120)
    return (args.output/name).read_bytes()

  versions = {}
  for name, binary in (('ffmpeg', args.ffmpeg), ('ffprobe', args.ffprobe)):
    version = run([binary, '-version'], name+'-version.txt').decode().splitlines()[0]
    assert version.startswith(name+' version 6.1.1 '), version
    versions[name] = version
  width, height, frames = 16, 16, 3
  data = bytes(index % 251 for index in range(width*height*3//2*frames))
  (args.output/'input.yuv').write_bytes(data)
  cases = []
  # Both outputs are lossless. These check real CLI container/codec/readback
  # behavior without borrowing a previously packaged project executable.
  for codec, extra, expected in (('ffv1', [], 'ffv1'), ('libx264', ['-preset', 'ultrafast', '-crf', '0'], 'h264')):
    movie = args.output/(codec+'.mkv')
    command = [args.ffmpeg, '-v', 'error', '-nostdin', '-y', '-f', 'rawvideo', '-pixel_format', 'yuv420p',
               '-video_size', f'{width}x{height}', '-framerate', '20', '-i', 'pipe:0', '-an',
               '-frames:v', str(frames), '-c:v', codec, '-threads', '1', *extra, movie]
    commands.append(list(map(str, command)))
    with (args.output/(codec+'.stdout')).open('wb') as stdout, (args.output/(codec+'.stderr')).open('wb') as stderr:
      child = subprocess.Popen(list(map(str, command)), stdin=subprocess.PIPE, stdout=stdout, stderr=stderr)
      try:
        deadline = time.monotonic()+60
        while True:
          if child.poll() is not None:
            raise RuntimeError('owned FFmpeg child exited before input/maps verification')
          with Path(f'/proc/{child.pid}/maps').open('rb') as stream:
            maps = stream.read(8*1024**2+1)
          assert len(maps) <= 8*1024**2
          if all(name in maps for name in (b'libavcodec.so', b'libavformat.so', b'libavutil.so', b'libavdevice.so',
                                           b'libavfilter.so', b'libpostproc.so', b'libswresample.so', b'libswscale.so')):
            break
          if time.monotonic() >= deadline:
            raise RuntimeError('owned FFmpeg child did not map its original library closure')
          time.sleep(0.02)
        (args.output/(codec+'.maps.txt')).write_bytes(maps)
        child.communicate(input=data, timeout=120)
        assert child.returncode == 0
      finally:
        if child.poll() is None:
          child.terminate()
          try:
            child.communicate(timeout=10)
          except subprocess.TimeoutExpired:
            child.kill()
            child.communicate(timeout=10)
    probe = json.loads(run([args.ffprobe, '-v', 'error', '-select_streams', 'v:0', '-count_frames',
        '-show_entries', 'stream=codec_name,width,height,pix_fmt,nb_read_frames', '-of', 'json', movie], codec+'.probe.json'))
    assert len(probe['streams']) == 1
    stream = probe['streams'][0]
    assert all(stream.get(key) == value for key, value in dict(codec_name=expected, width=width, height=height,
                                                             pix_fmt='yuv420p', nb_read_frames=str(frames)).items())
    decoded = args.output/(codec+'.decoded.yuv')
    run([args.ffmpeg, '-v', 'error', '-nostdin', '-y', '-i', movie,
         '-f', 'rawvideo', '-pix_fmt', 'yuv420p', '-threads', '1', decoded], codec+'.decode.stdout')
    assert decoded.read_bytes() == data
    cases.append(dict(encoder=codec, codec=expected, width=width, height=height, frames=frames,
                      exact_decoded_bytes=len(data), container_sha256=hashlib.sha256(movie.read_bytes()).hexdigest()))
  (args.output/'commands.json').write_text(json.dumps(commands, indent=2)+'\n')
  print(json.dumps(dict(versions=versions, cases=cases, exact_lossless_cli_roundtrips=2,
                       original_source_daemon_regression=False, physical_device_execution=False)))


if __name__ == '__main__':
  main()
