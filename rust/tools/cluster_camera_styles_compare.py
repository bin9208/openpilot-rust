"""Capture exact existing-style pixels after admitting the cluster shader slot."""

from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

import numpy as np
from PIL import Image


ROOT = Path(__file__).resolve().parents[2]
NAMES = ('large', 'compact-00', 'compact-10', 'compact-01', 'compact-11', 'large-after-compact')


def main() -> None:
  binary, output = map(Path, sys.argv[1:])
  output = output.resolve()
  results = []
  for width, height in ((384, 192), (800, 480)):
    paths = []
    for lane in ('source', 'native'):
      path = output / f'{width}x{height}' / lane
      path.mkdir(parents=True)
      match lane:
        case 'source':
          command = [sys.executable, str(ROOT / 'rust/tools/cluster_camera_styles_source.py'), str(ROOT), str(path), str(width), str(height)]
        case 'native':
          command = [str(binary.resolve()), str(ROOT), str(path), str(width), str(height)]
      (path / 'invocation.json').write_text(json.dumps({'command': command, 'display': os.environ['DISPLAY']}, indent=2))
      with (path / 'process.log').open('w') as log:
        subprocess.run(command, stdout=log, stderr=subprocess.STDOUT, check=True)
      paths.append(path)
    for name in NAMES:
      images = [np.asarray(Image.open(path / f'{name}.png')).astype(int) for path in paths]
      delta = abs(images[0] - images[1])
      result = {'viewport': [width, height], 'style': name, 'pixels': int(np.any(delta, axis=-1).sum()), 'max_channel': int(delta.max())}
      results.append(result)
      assert result['pixels'] == 0, result
  result = {'pass': True, 'results': results, 'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest()}
  (output / 'result.json').write_text(json.dumps(result, indent=2))
  print(json.dumps(result))


if __name__ == '__main__':
  main()
