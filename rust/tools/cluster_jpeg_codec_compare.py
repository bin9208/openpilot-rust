# /// script
# requires-python = ">=3.12"
# dependencies = ["pillow", "numpy", "PyTurboJPEG==1.8.3"]
# ///
# Run: source interpreter rust/tools/cluster_jpeg_codec_compare.py ROOT OUTPUT [NATIVE] [TURBO_PROVIDER].
from __future__ import annotations
import hashlib
import json
from pathlib import Path
import subprocess
import sys


def main() -> None:
  root, output = (Path(value).resolve() for value in sys.argv[1:3])
  output.mkdir(parents=True, exist_ok=True)
  sys.path[:0] = [str(root), str(root / 'openpilot/selfdrive/carrot/cluster')]
  import cluster_usb_display as source
  import turbojpeg

  provider = Path(sys.argv[4]).resolve() if len(sys.argv) > 4 else output / 'missing-turbojpeg.so'
  original = turbojpeg.TurboJPEG
  turbojpeg.TurboJPEG = lambda: original(str(provider))
  records = []
  for mode in ['pillow', 'auto', 'turbojpeg']:
    for width, height in [(1, 1), (63, 35), (1920, 480)]:
      for quality in [-1, 68, 82, 100]:
        rgba = bytes((i * 29 + (i // 4) % width * 19 + (i // (width * 4)) * 37) % 256 for i in range(width * height * 4))
        name = f'{mode}-{width}x{height}-{quality}'
        pixels = output / f'{name}.rgba'
        pixels.write_bytes(rgba)
        display = source.TuringUsbDisplay(jpeg_quality=quality, jpeg_encoder=mode)
        display.set_profile_enabled(True)
        encoded = None
        error = None
        try:
          encoded = display.encode_jpeg(rgba, width, height)
        except (RuntimeError, OSError, ValueError) as exc:
          error = str(exc)
        stages = [name for name, _ in display.profile_samples()]
        if encoded is not None:
          (output / f'{name}-source.jpg').write_bytes(encoded)
        record = {'case': name, 'source_stages': stages, 'source_error': error, 'source_bytes': None if encoded is None else len(encoded)}
        if len(sys.argv) > 3:
          native = Path(sys.argv[3]).resolve()
          request = {
            'mode': 'codec',
            'encoder': mode,
            'quality': quality,
            'width': width,
            'height': height,
            'pixels': str(pixels),
            'output': str(output / f'{name}-native.jpg'),
            'turbo_library': str(provider),
            'profile': True,
          }
          run = subprocess.run([str(native)], input=json.dumps(request), capture_output=True, text=True, timeout=8)
          run.check_returncode()
          actual = json.loads(run.stdout)
          assert actual['stages'] == stages, (name, actual, stages)
          if encoded is not None:
            data = (output / f'{name}-native.jpg').read_bytes()
            assert encoded == data, (name, len(encoded), len(data))
            record['sha256'] = hashlib.sha256(data).hexdigest()
          else:
            assert actual['error'] is not None, actual
          record['native'] = actual
        records.append(record)
  turbojpeg.TurboJPEG = original
  report = {
    'all_equal': True if len(sys.argv) > 3 else None,
    'cases': len(records),
    'records': records,
    'source_sha256': hashlib.sha256(Path(source.__file__).read_bytes()).hexdigest(),
    'py_turbojpeg_sha256': hashlib.sha256(Path(turbojpeg.__file__).read_bytes()).hexdigest(),
    'provider': str(provider),
  }
  (output / 'report.json').write_text(json.dumps(report, indent=2))
  print(json.dumps({'all_equal': report['all_equal'], 'cases': len(records)}))


if __name__ == '__main__':
  main()
