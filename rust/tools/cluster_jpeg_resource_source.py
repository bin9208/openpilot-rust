# Run: source interpreter cluster_jpeg_resource_source.py ROOT TURBO_PROVIDER OUTPUT.
"""Golden bytes for each unchanged source codec and the native resource-cycle corpus."""
from __future__ import annotations

import hashlib
import json
from pathlib import Path
import sys


def main() -> None:
  root, provider, output = (Path(value).resolve() for value in sys.argv[1:])
  output.mkdir(parents=True, exist_ok=True)
  sys.path[:0] = [str(root), str(root / 'openpilot/selfdrive/carrot/cluster')]
  import cluster_usb_display as source
  import turbojpeg

  original = turbojpeg.TurboJPEG
  turbojpeg.TurboJPEG = lambda: original(str(provider))
  pixels = bytes(i % 251 for i in range(63 * 35 * 4))
  records = []
  for mode in ('turbojpeg', 'pillow'):
    jpeg = source.TuringUsbDisplay(jpeg_quality=82, jpeg_encoder=mode).encode_jpeg(pixels, 63, 35)
    path = output / f'resources-source-{mode}.jpg'
    path.write_bytes(jpeg)
    records.append({'mode': mode, 'path': str(path), 'size': len(jpeg), 'sha256': hashlib.sha256(jpeg).hexdigest()})
  result = {
    'source_codecs_equal': records[0]['sha256'] == records[1]['sha256'],
    'records': records,
    'provider_sha256': hashlib.sha256(provider.read_bytes()).hexdigest(),
    'source_sha256': hashlib.sha256(Path(source.__file__).read_bytes()).hexdigest(),
  }
  (output / 'resources-source-goldens.json').write_text(json.dumps(result, indent=2) + '\n')
  print(json.dumps(result))


if __name__ == '__main__':
  main()
