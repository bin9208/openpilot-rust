# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# Run: retained startup-ui Python rust/tools/cluster_world_source.py --evidence DIR
"""Capture unchanged source world helpers and the source CPU OBJ parser."""

from __future__ import annotations

import argparse
from dataclasses import asdict, replace
import hashlib
import json
import itertools
from pathlib import Path
from typing import TypeAlias

from openpilot.selfdrive.carrot.cluster import cluster_renderer as source
from openpilot.selfdrive.carrot.cluster.cluster_scene import Vec3, VehicleBox, RadarPointMarker, CameraSpec
from cluster_scene_compare import source as load_scene_source

Json: TypeAlias = None | bool | int | float | str | list['Json'] | dict[str, 'Json']


def vehicle(label: str, origin: str, primary: bool, cut_in: bool, speed: float | None) -> VehicleBox:
  return VehicleBox(
    center=Vec3(0.0, 20.0, 0.8),
    right_x=1.0,
    right_y=0.0,
    forward_x=0.0,
    forward_y=1.0,
    width_m=1.8,
    length_m=4.2,
    height_m=1.6,
    body_color=(100, 120, 140, 255),
    side_color=(80, 90, 100, 255),
    rear_color=(90, 100, 110, 255),
    top_highlight=(160, 180, 200, 255),
    outline_color=(220, 230, 240, 255),
    confidence=0.9,
    label=label,
    source=origin,
    longitudinal_m=None,
    relative_speed_mps=-1.0,
    absolute_speed_kph=speed,
    acceleration_mps2=0.5,
    ttc_s=5.0,
    cut_in=cut_in,
    primary=primary,
    annotate=True,
  )


def obj(path: Path) -> dict[str, Json]:
  renderer = object.__new__(source.ClusterUiRenderer)
  mesh = renderer._load_obj_mesh(path)
  try:
    return {
      'vertices': [float(mesh.vertices[i]) for i in range(mesh.vertexCount * 3)],
      'normals': [float(mesh.normals[i]) for i in range(mesh.vertexCount * 3)],
      'colors': [int(mesh.colors[i]) for i in range(mesh.vertexCount * 4)],
    }
  finally:
    source.rl.mem_free(mesh.vertices)
    source.rl.mem_free(mesh.normals)
    source.rl.mem_free(mesh.colors)


def main() -> None:
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument('--evidence', type=Path, required=True)
  args = parser.parse_args()
  args.evidence.mkdir(parents=True, exist_ok=True)
  rows = []
  for label, origin, primary, cut_in in [
    ('L1', 'radarState', True, False),
    ('L2', 'radarState', True, False),
    ('front', 'carState', False, False),
    ('CUT-IN', 'camera:left', False, True),
    ('model', 'modelV2', False, False),
    ('lead', 'CAN 0x162', True, False),
    ('corner', 'liveTracks', False, False),
    ('other', '', False, False),
  ]:
    for speed in [None, -20.0, -3.0, 0.0, 2.0, 60.0]:
      item = vehicle(label, origin, primary, cut_in, speed)
      for metric in [True, False]:
        for theme in ['dark', 'light']:
          colors = source.current_cluster_theme(theme)
          for color_mode in [0, 1]:
            rows.append(
              {
                'op': 'vehicle',
                'vehicle': asdict(item),
                'metric': metric,
                'theme': theme,
                'color_mode': color_mode,
                'expected': {
                  'distance': source.vehicle_distance_label(item, metric),
                  'speed': source.vehicle_speed_label(item, metric),
                  'distance_m': source.vehicle_distance_m(item),
                  'lead_one': source.vehicle_has_lead_one_metrics(item),
                  'color': list(source.vehicle_metric_color(item, colors, color_mode)),
                  'coins': [
                    [list(c) for c in source.camera_overlay_vehicle_coin_colors(item, l1, l2)]
                    for l1, l2 in [(False, False), (True, False), (False, True), (True, True)]
                  ],
                  'stopped': source.camera_overlay_vehicle_is_stopped(item),
                },
              }
            )
  for distance in [-200.0, -18.0, 0.0, 18.0, 30.0, 90.0, 180.0, 200.0]:
    rows.append({'op': 'scale', 'distance': distance, 'expected': source.world_label_scale(distance)})
  for mode in range(-1, 7):
    rows.append(
      {
        'op': 'mode',
        'mode': mode,
        'expected': [
          source.radar_info_shows_vehicle(mode),
          source.radar_info_shows_radar_points(mode),
          source.radar_info_shows_speed(mode),
          source.radar_info_shows_distance(mode),
        ],
      }
    )
  for camera in [
    CameraSpec(Vec3(0.0, -3.0, 2.0), Vec3(0.0, 20.0, 0.0), 55.0),
    CameraSpec(Vec3(1.0, 2.0, 3.0), Vec3(1.0, 2.0, 3.0), 55.0),
    CameraSpec(Vec3(-2.0, 5.0, 7.0), Vec3(3.0, 4.0, 1.0), 70.0),
  ]:
    for point in [Vec3(0.0, 0.0, 0.0), Vec3(4.0, 18.0, 2.0), Vec3(-20.0, -30.0, 7.0)]:
      rows.append(
        {
          'op': 'camera',
          'camera': asdict(camera),
          'point': asdict(point),
          'expected': {'forward': source.camera_forward(camera), 'depth': source.camera_depth_m(point, camera)},
        }
      )
  for left, right, bounds in [
    ((0.0, 0.0, 2.0, 2.0), (2.0, 0.0, 1.0, 1.0), (0.0, 0.0, 3.0, 3.0)),
    ((0.0, 0.0, 2.0, 2.0), (1.0, 1.0, 2.0, 2.0), (1.0, 1.0, 3.0, 3.0)),
    ((-1.0, -2.0, 5.0, 6.0), (20.0, 10.0, 1.0, 1.0), (-2.0, -3.0, 4.0, 4.0)),
  ]:

    def rect(value: tuple[float, float, float, float]) -> dict[str, float]:
      return dict(zip(['x', 'y', 'width', 'height'], value, strict=True))

    rows.append(
      {
        'op': 'rectangles',
        'left': rect(left),
        'right': rect(right),
        'bounds': dict(zip(['left', 'top', 'right', 'bottom'], bounds, strict=True)),
        'expected': {'overlap': source.rectangles_overlap(left, right), 'inside': source.label_rect_inside_bounds(left, bounds)},
      }
    )
  for speed, metric in itertools.product([None, -20.0, -8.0, 0.0, 8.0, 60.0], [False, True]):
    point = RadarPointMarker(Vec3(0.0, 30.0, 0.0), 0.2, (255, 255, 255, 255), 'raw', 30.0, 0.0, absolute_speed_kph=speed)
    rows.append(
      {
        'op': 'radar',
        'point': asdict(point),
        'metric': metric,
        'expected': {'distance': source.radar_point_distance_label(point, metric), 'speed': source.radar_point_speed_label(point, metric)},
      }
    )
  _, _, _, base = load_scene_source(Path(__file__).resolve().parents[2])
  for mode, camera, panel, width in itertools.product(range(-1, 7), range(5), range(2), [0, 1280, 1920]):
    state = replace(base, onroad=True, gear_text='D', external_nav_active=True, camera_view_mode=camera)
    options = {'screen_mode': mode, 'panel_layout': panel, 'width': width, 'height': 480}
    renderer = source.ClusterUiRenderer(**options)
    try:
      bounds = renderer._world_label_bounds(10.0, 20.0, 30.0, 40.0)
      rows.append(
        {
          'op': 'policy',
          'state': asdict(state),
          'options': options,
          'expected': {
            'shift': renderer._world_view_shift_x(state),
            'design_shift': renderer._world_view_shift_design_x(state),
            'left': renderer._turn_signal_center_x_offset(state, 'left'),
            'right': renderer._turn_signal_center_x_offset(state, 'right'),
            'bounds': dict(zip(['left', 'top', 'right', 'bottom'], bounds, strict=True)),
          },
        }
      )
    finally:
      renderer.close()
  for name, data in [
    ('negative', b'v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nvn 0 0 -2\nusemtl body\nf -4//1 -3//1 -2//1 -1//1\n'),
    ('splitlines', b'v 0 0 0\x1cv 1 0 0\x1dv 0 1 0\x1ef 1 2 3\n'),
    ('degenerate', b'v 0 \xff0 0\nv 0 0 0\nv 0 0 0\nusemtl unknown\nf 1 2 3\n'),
  ]:
    path = args.evidence / f'{name}.obj'
    path.write_bytes(data)
    rows.append({'op': 'obj', 'path': str(path.resolve()), 'expected': obj(path)})
  rows.append({'op': 'obj', 'path': str(source.VEHICLE_MODEL_PATH), 'expected': obj(source.VEHICLE_MODEL_PATH)})
  output = {
    'source': str(Path(source.__file__).resolve()),
    'source_sha256': hashlib.sha256(Path(source.__file__).read_bytes()).hexdigest(),
    'model': str(source.VEHICLE_MODEL_PATH),
    'model_sha256': hashlib.sha256(source.VEHICLE_MODEL_PATH.read_bytes()).hexdigest(),
    'rows': rows,
  }
  (args.evidence / 'source.json').write_text(json.dumps(output, indent=2) + '\n')
  print(json.dumps({'helper_rows': len(rows), 'model_vertices': len(rows[-1]['expected']['vertices']) // 3, 'source_sha256': output['source_sha256']}))


if __name__ == '__main__':
  main()
