# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Capture actual unchanged source world/TPMS drawing with owned asset providers."""

from __future__ import annotations

import argparse
from dataclasses import asdict, replace
import hashlib
import itertools
import json
import math
from pathlib import Path
import sys

from cluster_scene_compare import source as load_scene_source
from cluster_world_source import vehicle


def asset_root(root: Path, output: Path, model: bool, icon: bool) -> Path:
  target = output / 'assets' / f'model-{int(model)}-icon-{int(icon)}'
  files = ['openpilot/selfdrive/assets/fonts/KaiGenGothicKR-Bold.ttf', 'openpilot/selfdrive/assets/fonts/JetBrainsMono-Medium.ttf']
  if model:
    files.append('openpilot/selfdrive/carrot/cluster/assets/models/cybertruck/cybertruck_cluster.obj')
  if icon:
    files.append('openpilot/selfdrive/carrot/cluster/assets/images/tpms_toy_car.png')
  for relative in files:
    origin, destination = root / relative, target / relative
    if origin.is_file():
      destination.parent.mkdir(parents=True, exist_ok=True)
      if not destination.exists():
        destination.symlink_to(origin)
  return target


def cases(root: Path, output: Path):
  config, models, scene, base = load_scene_source(root)
  path = tuple(models.ModelPathPoint(i * 4.0, math.sin(i * 0.13) * 0.6, 0.02 * i) for i in range(33))
  base = replace(
    base,
    onroad=True,
    gear_text='D',
    external_nav_active=True,
    speed_kph=65.0,
    model_path=path,
    highlight_lane='left',
    highlight_lane_offset=-1.0,
    extra_left_lane_visible=True,
    extra_right_lane_visible=True,
    left_road_edge_offset=-1.0,
    right_road_edge_offset=1.0,
    tpms=models.TpmsInfo(fl=30.5, fr=31.5, rl=None, rr=36.0),
  )
  for theme, camera, model, icon, info, swapped in itertools.product(
    ['light', 'dark'], ['driving', 'surround'], [False, True], [False, True], range(5), [False, True]
  ):
    state = replace(
      base, surround_view_active=camera == 'surround', surround_yaw_deg=13.0, surround_pitch_deg=9.0, radar_info_mode=info, radar_source_color_mode=int(swapped)
    )
    palette = config.DARK_CLUSTER_THEME if theme == 'dark' else config.LIGHT_CLUSTER_THEME
    world = scene.build_cluster_scene(state, theme=palette)
    items = []
    for label, origin, primary, cutin, position, speed, confidence in [
      ('L1', 'carState', True, False, (0.0, 30.0), 60.0, 0.91),
      ('L2', 'radarState', True, False, (-3.4, 48.0), 42.0, 0.63),
      ('CUT-IN', 'cameraLead', False, True, (2.5, 22.0), 50.0, 0.78),
      ('model', 'modelV2', False, False, (-4.2, 62.0), 15.0, 0.42),
      ('box', '', False, False, (3.6, 72.0), 0.0, 0.99),
      ('overlap', 'liveTracks', False, False, (0.0, 30.5), 60.0, 0.86),
      ('behind', 'radarPoint', False, False, (0.0, -30.0), -20.0, 0.31),
      ('far', 'CAN 0x162', False, False, (7.2, 170.0), 65.0, 0.2),
    ]:
      item = vehicle(label, origin, primary, cutin, speed)
      yaw = 0.2 if cutin else 0.0
      items.append(
        replace(
          item,
          center=scene.Vec3(*position, 0.8),
          longitudinal_m=position[1] - config.EGO_FORWARD_M,
          right_x=math.cos(yaw),
          right_y=-math.sin(yaw),
          forward_x=math.sin(yaw),
          forward_y=math.cos(yaw),
          confidence=confidence,
        )
      )
    points = tuple(
      scene.RadarPointMarker(scene.Vec3(x, y, 0.12), 0.16, (160, 100, 40, 210), label, y - config.EGO_FORWARD_M, x, -2.0, speed, None, None, 0.8, 1, 1)
      for x, y, label, speed in [(1.0, 80.0, 'raw', 50.0), (-6.0, 44.0, 'static', 0.0), (0.0, -25.0, 'behind', 25.0)]
    )
    world = replace(world, vehicles=tuple(items), radar_points=points, scene_shift_x_m=0.4)
    yield {
      'name': f'{theme}-{camera}-model{int(model)}-icon{int(icon)}-info{info}-swap{int(swapped)}',
      'assets_root': str(asset_root(root, output, model, icon)),
      'state': state,
      'scene': world,
      'options': {
        'width': 1920,
        'height': 480,
        'theme_mode': theme,
        'language': 'ko',
        'is_metric': not swapped,
        'screen_mode': 0 if not swapped else 4,
        'panel_layout': int(swapped),
      },
    }


def main() -> None:
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument('root', type=Path)
  parser.add_argument('output', type=Path)
  parser.add_argument('--prepare-only', action='store_true')
  parser.add_argument('--strip-fallback', action='store_true')
  parser.add_argument('--highlight-only', action='store_true')
  args = parser.parse_args()
  root = args.root.resolve()
  output = args.output.resolve()
  output.mkdir(parents=True, exist_ok=True)
  sys.path.insert(0, str(root))
  sys.path.insert(0, str(root / 'openpilot/selfdrive/carrot/cluster'))
  import cluster_renderer as reference
  import pyray as rl

  records = list(cases(root, output))
  extra = []
  for original in records:
    if 'model1-icon0-info4-swap0' not in original['name']:
      continue
    extra.append({**original, 'name': original['name'] + '-tpms-empty', 'state': replace(original['state'], tpms=type(original['state'].tpms)())})
    original_scene = original['scene']
    shifted = tuple(replace(strip, x_offset_m=0.163) for strip in original_scene.planned_path)
    short = replace(original_scene.planned_path[0], left=original_scene.planned_path[0].left[:1])
    extra.append(
      {
        **original,
        'name': original['name'] + '-strip-offset',
        'scene': replace(original_scene, planned_path=shifted, highlight_lanes=original_scene.highlight_lanes + (short,)),
      }
    )
    lead = replace(original_scene.vehicles[0], center=type(original_scene.vehicles[0].center)(0.0, 100000.0, 0.8), longitudinal_m=100000.0)
    competitor = replace(original_scene.vehicles[5], center=lead.center, longitudinal_m=100000.0)
    extra.append({**original, 'name': original['name'] + '-L1-horizon', 'scene': replace(original_scene, vehicles=(lead, competitor))})
  if args.highlight_only:
    import cluster_scene as scene_source
    import cluster_config as scene_config

    highlight_records = []
    for item in records:
      if 'model1-icon0-info4-swap0' not in item['name']:
        continue
      state = replace(item['state'], model_path=(), left_road_edge_offset=None, right_road_edge_offset=None)
      palette = scene_config.DARK_CLUSTER_THEME if item['options']['theme_mode'] == 'dark' else scene_config.LIGHT_CLUSTER_THEME
      actual_scene = scene_source.build_cluster_scene(state, theme=palette)
      assert actual_scene.highlight_lanes, 'source highlight not produced'
      original_scene = item['scene']
      highlight_records.append(
        {
          **item,
          'name': item['name'] + '-highlight',
          'state': state,
          'scene': replace(actual_scene, vehicles=original_scene.vehicles, radar_points=original_scene.radar_points, scene_shift_x_m=0.4),
        }
      )
    records = highlight_records
  else:
    records.extend(extra)
  if args.strip_fallback:
    records = [item for item in records if '-info4-swap0' in item['name']]
    delattr(rl, 'draw_triangle_strip_3d')
  (output / 'input.json').write_text(
    json.dumps({'cases': [{**item, 'state': asdict(item['state']), 'scene': asdict(item['scene'])} for item in records]}, ensure_ascii=False)
  )
  if args.prepare_only:
    print(json.dumps({'cases': len(records)}))
    return
  renderer = None
  target = None
  provider = None
  captures = []
  resources = []
  try:
    for index, item in enumerate(records):
      assets = Path(item['assets_root'])
      if provider != assets:
        if target is not None:
          rl.unload_render_texture(target)
        if renderer is not None:
          renderer.close()
        reference.VEHICLE_MODEL_PATH = assets / 'openpilot/selfdrive/carrot/cluster/assets/models/cybertruck/cybertruck_cluster.obj'
        reference.TPMS_CAR_ICON_PATH = assets / 'openpilot/selfdrive/carrot/cluster/assets/images/tpms_toy_car.png'
        renderer = reference.ClusterUiRenderer(**item['options'])
        renderer.open(hidden=True)
        target = rl.load_render_texture(1920, 480)
        provider = assets
      renderer.set_theme_mode(item['options']['theme_mode'])
      renderer.set_screen_mode(item['options']['screen_mode'])
      renderer.set_panel_layout(item['options']['panel_layout'])
      renderer.set_display_preferences('ko', item['options']['is_metric'])
      for warm in range(2):
        rl.begin_texture_mode(target)
        rl.clear_background(reference.rl_color(renderer._current_theme().bg))
        try:
          renderer._draw_scene(item['scene'], item['state'])
          renderer._draw_tpms_status(item['state'])
        finally:
          rl.end_texture_mode()
        image = rl.load_image_from_texture(target.texture)
        path = output / f"{item['name']}-{warm}.png"
        try:
          rl.image_flip_vertical(image)
          assert rl.export_image(image, str(path)), path
        finally:
          rl.unload_image(image)
        captures.append(
          {'name': item['name'], 'warm': warm, 'width': 1920, 'height': 480, 'path': str(path), 'sha256': hashlib.sha256(path.read_bytes()).hexdigest()}
        )
      resources.append(
        {
          'name': item['name'],
          'model': renderer._vehicle_model is not None,
          'icon': renderer._tpms_car_texture is not None,
          'strip_cache': len(renderer._triangle_strip_point_cache),
        }
      )
      if index % 20 == 0:
        print(f'world source {index + 1}/{len(records)}', flush=True)
  finally:
    if target is not None:
      rl.unload_render_texture(target)
    if renderer is not None:
      renderer.close()
  source_path = root / 'openpilot/selfdrive/carrot/cluster/cluster_renderer.py'
  result = {
    'cases': len(records),
    'frames': len(captures),
    'captures': captures,
    'resources': resources,
    'source_sha256': hashlib.sha256(source_path.read_bytes()).hexdigest(),
    'strip_primitive_available': not args.strip_fallback,
    'scope': 'Unchanged world/TPMS methods; actual raylib, original model/icon/fonts, source-generated CameraSpecs and strips.',
  }
  (output / 'source-result.json').write_text(json.dumps(result, indent=2) + '\n')
  print(json.dumps({'cases': len(records), 'frames': len(captures)}))


if __name__ == '__main__':
  main()
