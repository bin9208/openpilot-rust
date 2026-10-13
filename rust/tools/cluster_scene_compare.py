from __future__ import annotations

import argparse
import ast
from dataclasses import asdict, replace
import hashlib
import json
import math
from pathlib import Path
import random
import subprocess
import sys


def source(root: Path):
  sys.path.insert(0, str(root / 'openpilot/selfdrive/carrot/cluster'))
  import cluster_config as config
  import cluster_models as models
  import cluster_scene as scene

  path = root / 'openpilot/selfdrive/carrot/cluster/cluster_live.py'
  standby = next(node for node in ast.parse(path.read_text()).body if isinstance(node, ast.FunctionDef) and node.name == 'standby_state')
  scope = {**vars(models), **vars(config)}
  exec(compile(ast.Module(body=[standby], type_ignores=[]), str(path), 'exec'), scope)
  return config, models, scene, scope['standby_state']()


def cases(models, base):
  yield 'standby', base
  for mode in range(5):
    for side in ('left', 'right'):
      yield (
        f'lane-{mode}-{side}',
        replace(
          base,
          speed_kph=86.0,
          steering=0.37,
          camera_view_mode=mode,
          lane_change=side,
          lane_change_phase='changing',
          highlight_lane=side,
          highlight_lane_offset=-1.0 if side == 'left' else 1.0,
          ego_lane_offset=0.42,
          extra_left_lane_visible=True,
          extra_right_lane_visible=True,
          left_blindspot=True,
          right_blindspot=True,
          cruise_gap=3,
          cruise_display_state='engaged',
        ),
      )
  for yaw, pitch in ((0.0, 0.0), (-80.0, 45.0), (80.0, -45.0), (13.0, 9.0)):
    yield f'orbit-{yaw}-{pitch}', replace(base, surround_view_active=True, surround_yaw_deg=yaw, surround_pitch_deg=pitch)
  points = tuple(models.ModelPathPoint(i * 4.0, math.sin(i * 0.13) * 2.1, 0.02 * i, 15.0 + i * 0.4, (i % 7 - 3) * 0.8) for i in range(33))
  for style in ('solid', 'dashed', 'unknown'):
    lanes = tuple(
      models.LaneMarking(
        offset,
        width=5 + i,
        style=style,
        model_points=tuple(replace(point, lateral_m=point.lateral_m + offset * 3.6) for point in points),
        model_lateral_shift_m=0.163,
      )
      for i, offset in enumerate((-1.5, -0.5, 0.5, 1.5))
    )
    for mode in range(5):
      yield (
        f'model-{style}-{mode}',
        replace(
          base,
          lanes=lanes,
          model_path=points,
          camera_view_mode=mode,
          speed_kph=60.0,
          cruise_gap=2,
          cruise_display_state='paused',
          longitudinal_t_follow_s=1.7,
          left_road_edge_points=tuple(replace(point, lateral_m=point.lateral_m - 7.1) for point in points),
          right_road_edge_points=tuple(replace(point, lateral_m=point.lateral_m + 7.0) for point in points),
          left_road_edge_lateral_shift_m=-0.2,
          right_road_edge_lateral_shift_m=0.3,
          left_road_edge_distance_m=0.7,
          right_road_edge_distance_m=1.1,
          left_road_edge_confidence=0.4,
          right_road_edge_confidence=0.8,
        ),
      )
  yield 'short-model-fallback', replace(base, model_path=points[:1], lanes=(replace(base.lanes[0], model_points=points[:1]),))
  yield (
    'signed-zero-merge-order',
    replace(
      base,
      radar_points=(
        models.RadarPoint('A', 0.0, 3.0, source='liveTracks'),
        models.RadarPoint('B', -0.0, 3.0, source='liveTracks'),
      ),
    ),
  )
  for mode in range(2):
    for source_name in ('modelV2', 'radarState', 'carState', 'CAN 0x162'):
      radar = (
        models.RadarPoint('zero', 0.0, 0.0, source='liveTracks'),
        models.RadarPoint('A', 30.0, 0.5, source='liveTracks', relative_speed_mps=-3.0, probability=0.8, valid_count=40, motion_consistent=True),
        models.RadarPoint('B', 30.6, 0.6, source='radarPoint', relative_speed_mps=-2.7, probability=0.7, valid_count=45, motion_consistent=True),
        models.RadarPoint('C', 30.7, 0.6, source='liveTracks', relative_speed_mps=4.0, probability=0.9, valid_count=50, motion_consistent=True),
        models.RadarPoint('D', 50.0, 4.0, source='liveTracks', relative_speed_mps=-2.0, probability=0.8, valid_count=50, motion_consistent=True),
      )
      vehicles = (
        models.DetectedVehicle('TARGET' if source_name == 'radarState' else 'MODEL', 30.0, 0.5, source=source_name, probability=0.95, primary=True),
        models.DetectedVehicle('MODEL2', 31.0, 0.6, source='modelV2', probability=0.7, relative_speed_mps=-3.0),
        models.DetectedVehicle('RR', 50.0, 4.0, source='carState', probability=0.8),
      )
      yield f'merge-{mode}-{source_name}', replace(base, radar_points=radar, detected_vehicles=vehicles, radar_display_mode=mode, speed_kph=60.0)
  for lane_width in (0.0, 1.0, 8.0):
    yield f'width-{lane_width}', replace(base, lane_width_m=lane_width)
  rng = random.Random(253)
  for index in range(48):
    radar = tuple(
      models.RadarPoint(
        label=f'R{i}',
        longitudinal_m=rng.uniform(-20.0, 155.0),
        lateral_m=rng.uniform(-12.0, 12.0),
        source=rng.choice(('liveTracks', 'cornerRadar', 'radarPoint', 'CAN 0x162', 'unknown')),
        relative_speed_mps=rng.choice((None, -22.0, -2.0, 0.0, 13.0)),
        absolute_speed_kph=rng.choice((None, -12.0, 0.0, 65.0)),
        lateral_speed_mps=rng.choice((None, -3.0, 0.2, 2.0)),
        relative_accel_mps2=rng.choice((None, -6.0, -2.5, 0.0, 6.0)),
        probability=rng.choice((None, 0.19, 0.35, 0.85)),
        valid=rng.choice((None, 0, 1)),
        valid_count=rng.choice((None, 19, 20, 24, 35, 120)),
        in_my_lane=rng.choice((None, -1, 0, 1)),
        motion_consistent=rng.choice((None, False, True)),
        promotion_held=rng.choice((False, True)),
      )
      for i in range(72 if index == 0 else 18)
    )
    vehicles = tuple(
      models.DetectedVehicle(
        label=rng.choice(('TARGET', 'TARGET2', 'L1', 'L2', 'LF', 'RR', 'CUT', 'MODEL')),
        longitudinal_m=rng.choice((0.0, 10.0, 20.0, 22.0, 30.0, 50.0)),
        lateral_m=rng.uniform(-4.0, 4.0),
        source=rng.choice(('radarState', 'carState', 'modelV2', 'CAN 0x162', 'cameraLead')),
        probability=rng.choice((0.2, 0.35, 0.9)),
        cut_in=i == 0,
        primary=i == 1,
        radar_track_id=i % 3,
        relative_speed_mps=rng.choice((None, -4.0, 2.0)),
        absolute_speed_kph=rng.choice((None, -15.0, 50.0)),
        lateral_speed_mps=rng.choice((None, -2.0, 2.0)),
        ttc_s=rng.choice((None, 9.8, 10.0)),
      )
      for i in range(8)
    )
    yield (
      f'objects-{index}',
      replace(
        base,
        radar_points=radar,
        detected_vehicles=vehicles,
        radar_display_mode=index % 2,
        radar_source_color_mode=index % 2,
        camera_view_mode=index % 5,
        speed_kph=72.0,
        steering=0.21,
        model_path=points if index % 3 == 0 else (),
        left_road_edge_offset=-1.0,
        right_road_edge_offset=1.0,
        left_road_edge_distance_m=1.35,
        surround_view_active=index % 4 == 0,
        surround_yaw_deg=12.0,
        ego_lane_offset=0.15,
      ),
    )
  # Ordered near-grid pairs exercise warm-cache reuse of original unquantized vertices.
  for shift in (0.0, 0.001, 0.024, 0.026, 0.0):
    warm = tuple(replace(point, lateral_m=point.lateral_m + shift) for point in points)
    yield f'warm-{shift}', replace(base, model_path=warm, lanes=(replace(base.lanes[0], model_points=warm),))


def differences(expected, actual, path='', found=None):
  found = [] if found is None else found
  if len(found) >= 12:
    return found
  if isinstance(expected, dict) and isinstance(actual, dict):
    if expected.keys() != actual.keys():
      found.append((path, sorted(expected), sorted(actual)))
    for key in expected.keys() & actual.keys():
      differences(expected[key], actual[key], f'{path}.{key}', found)
  elif isinstance(expected, (list, tuple)) and isinstance(actual, list):
    if len(expected) != len(actual):
      found.append((path + '.length', len(expected), len(actual)))
    for index, (left, right) in enumerate(zip(expected, actual, strict=False)):
      differences(left, right, f'{path}[{index}]', found)
  elif isinstance(expected, (int, float)) and not isinstance(expected, bool) and isinstance(actual, (int, float)):
    if not math.isclose(expected, actual, rel_tol=1e-11, abs_tol=1e-10):
      found.append((path, expected, actual))
  elif expected != actual:
    found.append((path, expected, actual))
  return found


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  root = Path(__file__).resolve().parents[2]
  config, models, scene, base = source(root)
  args.output.mkdir(parents=True, exist_ok=True)
  results = []
  with subprocess.Popen([str(args.binary)], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True) as process:
    for name, state in cases(models, base):
      for dark, lit in ((False, True), (True, False)):
        request = {'state': asdict(state), 'dark': dark, 'highlight_lit': lit}
        process.stdin.write(json.dumps(request) + '\n')
        process.stdin.flush()
        profile_names = []
        expected_scene = scene.build_cluster_scene(
          state,
          profile_add=lambda name, _, names=profile_names: names.append(name),
          highlight_lane_lit=lit,
          theme=config.DARK_CLUSTER_THEME if dark else config.LIGHT_CLUSTER_THEME,
        )
        expected = {'scene': asdict(expected_scene), 'profile_names': profile_names}
        line = process.stdout.readline()
        if not line:
          raise RuntimeError(process.stderr.read())
        actual = json.loads(line)
        diff = differences(expected, actual)
        results.append({'name': f'{name}-{"dark" if dark else "light"}', 'pass': not diff, 'differences': diff})
        if diff:
          (args.output / 'first-request.json').write_text(json.dumps(request))
          (args.output / 'first-source.json').write_text(json.dumps(expected))
          (args.output / 'first-native.json').write_text(line)
          break
      if results[-1]['differences']:
        break
    process.stdin.close()
    process.wait(timeout=5)
    if process.returncode:
      raise RuntimeError(process.stderr.read())
  report = {
    'cases': results,
    'passed': sum(case['pass'] for case in results),
    'total': len(results),
    'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest(),
    'source_sha256': hashlib.sha256((root / 'openpilot/selfdrive/carrot/cluster/cluster_scene.py').read_bytes()).hexdigest(),
  }
  (args.output / 'result.json').write_text(json.dumps(report, indent=2) + '\n')
  print(json.dumps({key: value for key, value in report.items() if key != 'cases'}, indent=2))
  if results and results[-1]['differences']:
    print(json.dumps(results[-1], indent=2))
  return 0 if all(case['pass'] for case in results) else 1


if __name__ == '__main__':
  raise SystemExit(main())
