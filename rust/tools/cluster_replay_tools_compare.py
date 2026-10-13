# /// script
# requires-python = ">=3.12"
# dependencies = ["pillow", "numpy"]
# ///
# Run after source.py and the native render/events fixture against the same inputs.
from __future__ import annotations
import json
from pathlib import Path
import sys
import numpy as np
from PIL import Image


def compare(directory: Path) -> None:
  cases = json.loads((directory / 'cases.json').read_text())
  results = []
  for case in cases:
    name = case['name']
    source = np.frombuffer((directory / 'original' / f'{name}.rgba').read_bytes(), dtype=np.uint8).reshape(case['height'], case['width'], 4)
    native = np.frombuffer((directory / 'native-corrected' / f'{name}.rgba').read_bytes(), dtype=np.uint8).reshape(source.shape)
    difference = np.abs(source.astype(np.int16) - native.astype(np.int16))
    pixels = np.any(difference != 0, axis=2)
    results.append({'name': name, 'different_pixels': int(pixels.sum()), 'maximum_difference': int(difference.max())})
    Image.fromarray(native).save(directory / 'native-corrected' / f'{name}.png')
    if pixels.any():
      visual = np.zeros_like(source)
      visual[:, :, 3] = 255
      visual[pixels, :3] = 255
      Image.fromarray(visual).save(directory / f'{name}-diff.png')
  source_events = json.loads((directory / 'events-original.json').read_text())
  native_events = json.loads((directory / 'events-native.json').read_text())
  policy = [
    {'case': index, 'equal': left['commands'] == right['commands'] and left['options'] == right['state']['options'] and left['dragging'] == right['dragging']}
    for index, (left, right) in enumerate(zip(source_events, native_events, strict=True))
  ]
  report = {
    'frames': results,
    'event_cases': policy,
    'all_equal': all(item['different_pixels'] == 0 for item in results) and all(item['equal'] for item in policy),
  }
  (directory / 'report.json').write_text(json.dumps(report, indent=2))
  print(json.dumps(report, indent=2))
  if not report['all_equal']:
    raise SystemExit(1)


if __name__ == '__main__':
  compare(Path(sys.argv[1]))
