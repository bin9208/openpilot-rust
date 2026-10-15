"""Prepare frozen source day-housing pixels for the native acquisition fixture.

This temporary reference uses external OpenCV only to encode synthetic NV12.
Native worker/queue/planner execution is a separate command, never Python-owned.
"""
import argparse
import hashlib
import json
from pathlib import Path
import cv2
import numpy as np
from signal_source import source


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--process-input', type=Path, required=True)
  parser.add_argument('--owner-input', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--isolated-housing', action='store_true', help='Synthetic separated housing with source-detected evidence')
  parser.add_argument('--red-frames', type=int, default=12, help='Red dwell length for the isolated synthetic housing')
  args = parser.parse_args()
  assert 3 <= args.red_frames <= 20
  args.output.mkdir(parents=True, exist_ok=False)
  cv2.setNumThreads(1)
  cv2.ocl.setUseOpenCL(False)
  source('tools/signal_analysis/test_signal_tracker.py')  # verify final source pin
  frames = json.loads(args.process_input.read_text())[1]
  owner = json.loads(args.owner_input.read_text())
  images = []
  for index, frame in enumerate(frames):
    rgb = np.fromfile(frame['path'], np.uint8).reshape(frame['height'], frame['width'], 3)
    if args.isolated_housing:
      # Keep the final source test's image size/background, but separate the
      # housing from its deliberately merged background. This exercises the
      # original housing detector, rather than relabeling day-core evidence.
      rgb = np.full_like(rgb, 120)
      rgb[249:263, 620:672] = 20
      cv2.circle(rgb, (627 if index < args.red_frames else 665, 256), 4,
                 (255, 20, 20) if index < args.red_frames else (20, 210, 30), -1)
    height, width = rgb.shape[:2]
    yuv = cv2.cvtColor(rgb, cv2.COLOR_RGB2YUV_I420).reshape(-1)
    uv = np.stack((yuv[height * width:height * width * 5 // 4], yuv[height * width * 5 // 4:]), axis=-1).reshape(-1)
    nv12 = np.concatenate((yuv[:height * width], uv))
    restored = cv2.cvtColor(nv12.reshape(height * 3 // 2, width), cv2.COLOR_YUV2RGB_NV12)
    path = args.output / f'{index}.nv12'
    path.write_bytes(nv12.tobytes())
    pixels = args.output / f'{index}.rgb'
    pixels.write_bytes(restored.tobytes())
    images.append(dict(path=str(path.resolve()), rgb=str(pixels.resolve()), width=width, height=height, sha256=hashlib.sha256(nv12.tobytes()).hexdigest(), rgb_sha256=hashlib.sha256(restored.tobytes()).hexdigest()))
  result = dict(source_commit='e8e152dbcfd9574a1791ed12026d1880e87689b3', scene='synthetic_isolated_housing_radius4' if args.isolated_housing else 'source_merged_day_housing', artifact=owner['artifact'], car_params=owner['car_params'], parameters=owner['parameters'], packets=owner['frames'][0]['messages'], moving_packets=owner['frames'][120]['messages'], images=images)
  (args.output / 'input.json').write_text(json.dumps(result))
  print(json.dumps(dict(frames=len(images), nv12_bytes=sum(Path(v['path']).stat().st_size for v in images))))


if __name__ == '__main__':
  main()
