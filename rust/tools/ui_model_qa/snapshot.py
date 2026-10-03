"""Map source-owned representation to the native typed state contract."""

from capture import color


def points(values):
  return [{'x': float(x), 'y': float(y)} for x, y in values]


def model_points(value):
  return {'raw': value.raw_points.tolist(), 'projected': points(value.projected_points)}


def snapshot(widget, big):
  clip = widget._clip_region
  common = {
    'longitudinal': widget._longitudinal_control,
    'experimental': widget._experimental_mode,
    'path_height': float(widget._path_offset_z),
    'path': model_points(widget._path),
    'lanes': [model_points(v) for v in widget._lane_lines],
    'roads': [model_points(v) for v in widget._road_edges],
    'probabilities': widget._lane_line_probs.tolist(),
    'deviations': widget._road_edge_stds.tolist(),
    'acceleration': widget._acceleration_x.tolist(),
    'projection': {
      'transform': widget._car_space_transform.tolist(),
      'clip': {key: float(getattr(clip, key)) if clip is not None else 0.0 for key in ('x', 'y', 'width', 'height')},
    },
    'transform_dirty': widget._transform_dirty,
  }
  if big:
    names = {
      'mode': 'show_path_mode',
      'color': 'show_path_color',
      'width': 'show_path_width',
      'active_lane': 'active_lane_line',
      'long_active': 'long_active',
      'lane_speed': 'use_lane_line_speed_apply',
      'sequence': 'path_draw_seq',
      'position_t': 'pos_t',
      'path_x': 'path_x',
      'path_y': 'path_y',
      'path_width': 'path_width_px',
      'filtered_x': 'path_fx',
      'filtered_y': 'path_fy',
      'filtered_width': 'path_fwidth',
      'track_id': 'radar_track_id',
      'lead_status': 'lead_status',
      'radar_distance': 'radar_dist',
      'vision_distance': 'vision_dist',
      'x_state': 'x_state',
      'traffic_state': 'traffic_state',
      'speed': 'v_ego',
      'brake_hold': 'brake_hold_active',
      'soft_hold': 'soft_hold_active',
      'cruise': 'carrot_cruise',
      't_follow': 't_follow',
      'follow_distance': 'tf_distance',
      'follow_left': 'tf_left',
      'follow_right': 'tf_right',
      'lead_two_status': 'lead_two_status',
      'lead_two_left': 'lead_two_xl',
      'lead_two_right': 'lead_two_xr',
      'lead_two_y': 'lead_two_y',
    }
    carrot = {key: getattr(widget, '_carrot_' + value) for key, value in names.items()}
    carrot['sequence_second'] = getattr(widget, '_carrot_path_draw_seq2', None)
    carrot['barriers'] = [points(v) for v in widget._carrot_lane_barrier_vertices]
    settings = {
      'next_refresh': widget._carrot_params_next_refresh_time,
      'lane_info': widget._carrot_show_lane_info,
      'radar_info': widget._carrot_show_radar_info,
      'normal_mode': widget._carrot_show_path_mode_normal,
      'normal_color': widget._carrot_show_path_color_normal,
      'lane_mode': widget._carrot_show_path_mode_lane,
      'lane_color': widget._carrot_show_path_color_lane,
      'cruise_off_color': widget._carrot_show_path_color_cruise_off,
      'tire_trajectory': widget._carrot_tire_trajectory,
    }
    return {'common': common, 'carrot': carrot, 'settings': settings}
  lead = widget._lead_vehicles[0]
  return {
    'common': common,
    'filters': {
      'throttle': float(widget._blend_filter.x),
      'acceleration': float(widget._acceleration_x_filter.x),
      'acceleration_slow': float(widget._acceleration_x_filter2.x),
      'torque': float(widget._torque_filter.x),
    },
    'marking_codes': widget._lane_marking_codes,
    'marking_segments': [[points(p) for p in segments] for segments in widget._lane_marking_segments],
    'lead': {'corners': lead.rect, 'color': color(lead.color)} if lead.rect else None,
    'lead_filter': widget._lead_pt_filt[0],
    'radar_items': [
      {'x': item.x, 'y': item.y, 'width': item.w, 'height': item.h, 'text': item.text, 'color': color(item.color), 'star': item.is_star}
      for item in widget._radar_info_items
    ],
    'gradient': {'colors': [color(c) for c in widget._exp_gradient.colors], 'stops': [float(s) for s in widget._exp_gradient.stops]},
  }
