"""Compare built pinned Cython optimizations to exact final Python fallbacks."""

import argparse
import importlib.util
import random
import struct
from pathlib import Path
from types import SimpleNamespace as NS
from radar_final_source import load


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--extension', type=Path, required=True)
  args = parser.parse_args()
  load()
  spec = importlib.util.spec_from_file_location('radar_final_native', args.extension.resolve())
  native = importlib.util.module_from_spec(spec)
  spec.loader.exec_module(native)
  from openpilot.selfdrive.carrot.radar_motion import predictor, trajectory_cutin

  rng = random.Random(338)

  def equal(a, b):
    if isinstance(a, tuple):
      return len(a) == len(b) and all(equal(x, y) for x, y in zip(a, b, strict=False))
    if isinstance(a, float):
      return struct.pack('<d', a) == struct.pack('<d', b)
    return a == b

  count = 0
  for n in range(1, 40):
    for case in range(30):
      obs = [NS(time_s=i * 0.05, d_path=rng.uniform(-6, 6)) for i in range(n)]
      for metrics in (False, True):
        actual = native.motion(obs, 1.0, 'd_path', metrics)
        expected = trajectory_cutin._motion_metrics_python(obs, 1.0, 'd_path') if metrics else trajectory_cutin._median_slope_python(obs, 'd_path', 1.0)
        assert equal(actual, expected), (n, case, metrics, actual, expected)
        count += 1
  for _case in range(1000):
    path = tuple((float(i), rng.uniform(-3, 3)) for i in range(33))
    _, segments = predictor._path_geometry(path)
    x, y = rng.uniform(-5, 50), rng.uniform(-6, 6)
    assert equal(native.nearest_segment(segments, x, y), predictor._nearest_segment_python(segments, x, y))
    count += 1
  print(f'exact Cython/Python kernel comparisons passed: {count}')


if __name__ == '__main__':
  main()
