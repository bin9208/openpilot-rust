from __future__ import annotations

import argparse
from dataclasses import asdict, replace
import hashlib
import itertools
import json
from pathlib import Path
import sys

from cluster_scene_compare import source as load_scene_source
from cluster_world_render_source import asset_root
from cluster_world_source import vehicle


def cases(root: Path, output: Path):
  config, models, scene, base = load_scene_source(root)
  base = replace(
    base,
    onroad=True,
    speed_kph=40.0,
    model_path=(),
    left_road_edge_offset=None,
    right_road_edge_offset=None,
    highlight_lane='left',
    highlight_lane_offset=-1.0,
    extra_left_lane_visible=True,
    extra_right_lane_visible=True,
  )
  assets = asset_root(root, output, False, False)
  profiles = [('ar', 'tici', 'ar0231', False), ('os', 'mici', 'os04c10', False), ('wide', 'mici', 'os04c10', True)]
  for theme, profile, info, swap in itertools.product(['light', 'dark'], profiles, range(5), [False, True]):
    name, device, sensor, wide = profile
    state = replace(
      base,
      camera_device_type=device,
      camera_sensor=sensor,
      radar_info_mode=info,
      camera_calibration_euler=(0.013, -0.019, 0.006),
      wide_camera_from_device_euler=(0.002, 0.014, -0.008),
      road_transform_trans=(0.0, 0.0, 1.37),
      route_overlay=models.RouteOverlay(video_width=640, video_height=360),
    )
    world = scene.build_cluster_scene(state, theme=config.DARK_CLUSTER_THEME if theme == 'dark' else config.LIGHT_CLUSTER_THEME)
    items = []
    specifications = [
      ('L1', True, False, 0.0, 34.0, 50.0, 'carState'),
      ('L2', True, False, -1.8, 45.0, 25.0, 'radarState'),
      ('CUT-IN', False, True, 2.2, 24.0, 35.0, 'cameraLead'),
      ('slow', False, False, -3.2, 55.0, 4.0, 'cornerRadar'),
      ('stopped', False, False, 3.5, 75.0, 3.0, 'cornerRadar'),
      ('TARGET', True, False, 2.0, 130.0, -10.0, 'liveTracks'),
      ('overlap', False, False, 0.0, 34.5, 60.0, 'liveTracks'),
      ('behind', False, False, 0.0, -30.0, 60.0, 'radarPoint'),
    ]
    for label, primary, cutin, x, y, speed, origin in specifications:
      items.append(replace(vehicle(label, origin, primary, cutin, speed), center=scene.Vec3(x, y, 0.8), longitudinal_m=y - config.EGO_FORWARD_M))
    points = tuple(
      scene.RadarPointMarker(scene.Vec3(x, y, 0.12), 0.16, (160, 100, 40, 210), label, y - config.EGO_FORWARD_M, x, -2.0, speed, None, None, 0.8, 1, 1)
      for x, y, label, speed in [(0.6, 80.0, 'raw', 50.0), (-4.0, 44.0, 'static', 0.0), (1.0, 25.0, 'reverse', -15.0), (0.0, -25.0, 'behind', 25.0)]
    )
    strip = world.planned_path[0]
    left, right = list(strip.left), list(strip.right)
    for index in [4, 7, 8]:
      if index < len(left):
        left[index] = scene.Vec3(200.0, left[index].y, left[index].z)
    split = replace(strip, left=tuple(left), right=tuple(right), color=(40, 180, 220, 150), x_offset_m=0.163)
    short = replace(strip, left=strip.left[:1])
    world = replace(
      world,
      vehicles=tuple(items),
      radar_points=points,
      scene_shift_x_m=0.4,
      planned_path=world.planned_path + (split,),
      highlight_lanes=world.highlight_lanes + (short,),
    )
    yield {
      'name': f'{theme}-{name}-info{info}-swap{int(swap)}',
      'assets_root': str(assets),
      'state': state,
      'scene': world,
      'options': {
        'width': 1920,
        'height': 480,
        'theme_mode': theme,
        'language': 'ko',
        'is_metric': not swap,
        'screen_mode': 4 if swap else 0,
        'panel_layout': int(swap),
      },
      'wide': wide,
      'pitch_deg': 0.0,
      'z_offset': 0.0,
      'render': True,
    }
  template = next(cases_base(root, output))
  for device, sensor, wide in itertools.product(['neo', 'tizi', 'pc', 'missing', None], ['unknown', ' OS04C10 ', 'ox03c10', 'missing', None], [False, True]):
    yield {
      **template,
      'name': f'lookup-{device}-{sensor}-{wide}',
      'state': replace(template['state'], camera_device_type=device, camera_sensor=sensor),
      'wide': wide,
      'render': False,
    }
  for index, (pitch, height, speed, width) in enumerate(itertools.product([-3.0, 3.0], [-0.8, 0.4], [0.0, 65.0], [1280, 1920])):
    yield {
      **template,
      'name': f'tune-{index}',
      'pitch_deg': pitch,
      'z_offset': height,
      'wide': True,
      'state': replace(template['state'], speed_kph=speed),
      'options': {**template['options'], 'width': width},
    }


def cases_base(root: Path, output: Path):
  config, _, scene, base = load_scene_source(root)
  state = replace(base, onroad=True, speed_kph=40.0)
  yield {
    'name': 'base',
    'assets_root': str(asset_root(root, output, False, False)),
    'state': state,
    'scene': scene.build_cluster_scene(state, theme=config.DARK_CLUSTER_THEME),
    'options': {'width': 1920, 'height': 480, 'theme_mode': 'dark', 'language': 'ko', 'is_metric': True, 'screen_mode': 0, 'panel_layout': 0},
    'wide': False,
    'pitch_deg': 0.0,
    'z_offset': 0.0,
    'render': True,
  }


def rectangle(value):
  return {'x': float(value.x), 'y': float(value.y), 'width': float(value.width), 'height': float(value.height)}


def projection_record(view):
  return {
    **{
      key: getattr(view, key)
      for key in ['camera_width', 'camera_height', 'focal_length', 'zoom', 'video_tx', 'video_ty', 'view_from_road', 'camera_height_m', 'wide_camera']
    },
    **{key: rectangle(getattr(view, key)) for key in ['dest', 'source', 'video_dest']},
  }


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('root', type=Path)
  parser.add_argument('output', type=Path)
  parser.add_argument('--strip-fallback', action='store_true')
  args = parser.parse_args()
  root, output = args.root.resolve(), args.output.resolve()
  output.mkdir(parents=True, exist_ok=True)
  sys.path[:0] = [str(root), str(root / 'openpilot/selfdrive/carrot/cluster')]
  import cluster_renderer as reference
  import cluster_scene as scene_source
  import pyray as rl

  records = list(cases(root, output))
  if args.strip_fallback:
    records = [item for item in records if item['render'] and ('info4' in item['name'] or item['name'].startswith('tune-'))]
  points = [
    scene_source.Vec3(x, y, z) for x, y, z in itertools.product([-30.0, -4.0, 0.0, 4.0, 30.0], [4.18, 4.68, 5.0, 10.0, 34.18, 120.0, 1000.0], [-1.0, 0.0, 1.5])
  ]
  (output / 'input.json').write_text(
    json.dumps(
      {'cases': [{**item, 'state': asdict(item['state']), 'scene': asdict(item['scene'])} for item in records], 'points': [asdict(point) for point in points]},
      ensure_ascii=False,
    )
  )
  captures, helpers, buffers = [], [], []
  renderer, target, dimensions = None, None, None
  try:
    for index, item in enumerate(records):
      size = item['options']['width'], item['options']['height']
      if size != dimensions:
        if target is not None:
          rl.unload_render_texture(target)
        if renderer is not None:
          renderer.close()
        reference.VEHICLE_MODEL_PATH = Path(item['assets_root']) / 'missing.obj'
        reference.TPMS_CAR_ICON_PATH = Path(item['assets_root']) / 'missing.png'
        renderer = reference.ClusterUiRenderer(**item['options'])
        renderer.open(hidden=True)
        if args.strip_fallback:
          renderer._raw_draw_triangle_strip_2d = None
        target = rl.load_render_texture(*size)
        dimensions = size
      renderer.set_theme_mode(item['options']['theme_mode'])
      renderer.set_screen_mode(item['options']['screen_mode'])
      renderer.set_panel_layout(item['options']['panel_layout'])
      renderer.set_display_preferences('ko', item['options']['is_metric'])
      renderer._camera_overlay_wide = item['wide']
      renderer.camera_overlay_pitch_offset_deg = item['pitch_deg']
      renderer.camera_overlay_z_offset_m = item['z_offset']
      view = renderer._camera_overlay_projection(item['state'])
      camera = renderer._camera_overlay_camera(item['state'], item['wide'])
      helpers.append(
        {
          'name': item['name'],
          'projection': projection_record(view),
          'camera': {'width': float(camera.width), 'height': float(camera.height), 'focal_length': float(camera.focal_length)},
          'points': [renderer._camera_overlay_screen_xy(point, view, item['scene'].scene_shift_x_m) for point in points],
          'vehicles': [
            {'key': renderer._camera_overlay_vehicle_draw_key(vehicle), 'label': renderer._vehicle_overlay_label(vehicle, item['state'].radar_info_mode)}
            for vehicle in item['scene'].vehicles
          ],
        }
      )
      if item['render']:
        for warm in range(2):
          rl.begin_texture_mode(target)
          rl.clear_background(reference.rl_color(renderer._current_theme().bg))
          try:
            renderer._draw_camera_projected_overlay(item['scene'], item['state'], view if warm else None)
            rl.draw_rectangle_rec(rl.Rectangle(size[0] - 20, 5, 10, 10), rl.Color(255, 0, 255, 255))
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
            {
              'name': item['name'],
              'warm': warm,
              'width': size[0],
              'height': size[1],
              'path': str(path),
              'sha256': hashlib.sha256(path.read_bytes()).hexdigest(),
            }
          )
        buffers.append(
          {'name': item['name'], 'points': renderer._camera_overlay_strip_point_capacity, 'visible': len(renderer._camera_overlay_strip_pair_visibility)}
        )
      if index % 20 == 0:
        print(f'camera overlay source {index + 1}/{len(records)}', flush=True)
  finally:
    if target is not None:
      rl.unload_render_texture(target)
    if renderer is not None:
      renderer.close()
  result = {
    'cases': len(records),
    'frames': len(captures),
    'captures': captures,
    'helpers': helpers,
    'buffers': buffers,
    'strip_available': not args.strip_fallback,
    'source_sha256': hashlib.sha256((root / 'openpilot/selfdrive/carrot/cluster/cluster_renderer.py').read_bytes()).hexdigest(),
    'closed_points': renderer._camera_overlay_strip_point_capacity,
    'closed_visible': len(renderer._camera_overlay_strip_pair_visibility),
  }
  (output / 'source-result.json').write_text(json.dumps(result, indent=2) + '\n')
  print(json.dumps({'cases': len(records), 'frames': len(captures)}))


if __name__ == '__main__':
  main()
