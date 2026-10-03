#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy", "pycapnp"]
# ///
# ─── How to run ───
# Imported by: PYTHONPATH=. python rust/tools/check_selfdrived_controller.py --help
# ──────────────────
"""Read all policy state from the unchanged source instance for differential evidence."""
from __future__ import annotations

import math

from openpilot.cereal import car, log
from car_specific_oracle import snapshot as car_snapshot
from generate_selfdrive_alerts import serialize_alert

RENAMES = {'last_steering_pressed_frame': 'last_steering_pressed_frame', 'last_functional_fan_frame': 'last_functional_fan_frame',
           'logged_comm_issue': 'logged_comm_issue', 'not_running_prev': 'not_running_previous',
           'big_model_ready_t': 'big_model_ready_time', 'atc_type_last': 'atc_type_previous',
           'gps_location_service': 'gps_service'}
SCALARS = ('initialized', 'enabled', 'active', 'mismatch_counter', 'cruise_mismatch_counter',
           'last_steering_pressed_frame', 'distance_traveled', 'last_functional_fan_frame',
           'logged_comm_issue', 'not_running_prev', 'experimental_mode', 'recalibrating_seen',
           'dm_lockout_set', 'dm_uncertain_alerted', 'update_reboot_alerted', 'big_model_loading',
           'big_model_active', 'big_model_ready_t', 'atc_type_last', 'gps_location_service',
           'camera_packets', 'is_metric', 'is_ldw_enabled', 'disable_dm', 'use_wide_camera', 'excessive_actuation')


def pose_snapshot(pose):
  if pose is None:
    return None
  return {name: {'xyz': getattr(pose, name).xyz.tolist(), 'xyz_std': getattr(pose, name).xyz_std.tolist()}
          for name in ('orientation', 'velocity', 'acceleration', 'angular_velocity')}


def source_snapshot(fixture):
  machine = fixture.machine
  if machine is None:
    return None
  policy = {RENAMES.get(name, name): getattr(machine, name) for name in SCALARS}
  policy['not_running_previous'] = sorted(policy['not_running_previous']) if policy['not_running_previous'] is not None else None
  policy['personality'] = str(machine.personality)
  policy['mode'] = fixture.mode
  cp = machine.CP
  cp_car = {field: getattr(cp, field) for field in ('brand', 'minSteerSpeed', 'minEnableSpeed', 'pcmCruise', 'openpilotLongitudinalControl')}
  cp_car['networkLocation'] = cp.networkLocation.raw
  policy['config'] = {'car': cp_car, 'flags': cp.flags, 'alpha_longitudinal_available': cp.alphaLongitudinalAvailable,
                      'not_car': cp.notCar, 'passive': cp.passive, 'sec_oc_required': cp.secOcRequired,
                      'sec_oc_key_available': cp.secOcKeyAvailable, 'alternative_experience': cp.alternativeExperience,
                      'safety': [{'model': entry.safetyModel.raw, 'parameter': entry.safetyParam} for entry in cp.safetyConfigs]}
  policy['car_events'] = car_snapshot(machine.car_events)
  policy['pose_calibrator'] = {'calibrated': machine.pose_calibrator.calib_valid,
                               'calib_from_device': machine.pose_calibrator.calib_from_device.tolist()}
  policy['calibrated_pose'] = pose_snapshot(machine.calibrated_pose)
  policy['excessive_actuation_check'] = {name.lstrip('_'): value for name, value in vars(machine.excessive_actuation_check).items()}
  policy['state_machine'] = {'state': next(name for name, value in log.SelfdriveState.OpenpilotState.schema.enumerants.items()
                                         if value == machine.state_machine.state),
                             'soft_disable_timer': machine.state_machine.soft_disable_timer,
                             'current_alert_types': machine.state_machine.current_alert_types.copy()}
  policy['ignored_processes'] = sorted(machine.ignored_processes)
  return {'policy': policy, 'events': machine.events.names.copy(), 'static_events': machine.events.static_events.copy(),
          'counters': {str(key): value for key, value in machine.events.event_counters.items()},
          'alerts': [{'alert': serialize_alert(entry.alert), 'start_frame': entry.start_frame, 'end_frame': entry.end_frame, 'added_frame': entry.added_frame}
                     for entry in machine.AM.alerts.values()], 'current_alert': serialize_alert(machine.AM.current_alert),
          'previous': machine.CS_prev.to_dict(), 'events_previous': machine.events_prev.copy(), 'startup_event': machine.startup_event,
          'cutins': [vars(candidate) for candidate in machine.cutin_audio_tracker.previous]}


def source_health(fixture):
  if fixture.machine is None:
    return None
  sm = fixture.machine.sm
  topics = []
  for name in sm.services:
    tracker = sm.freq_tracker[name]
    topics.append({'service': name, 'seen': sm.seen[name], 'updated': sm.updated[name], 'receive_time': sm.recv_time[name],
                   'receive_frame': sm.recv_frame[name], 'log_mono_time': sm.logMonoTime[name], 'alive': sm.alive[name],
                   'frequency_ok': sm.freq_ok[name], 'valid': sm.valid[name], 'polled': True,
                   'tracker': {'min_frequency': tracker.min_freq, 'max_frequency': tracker.max_freq,
                               'average': {k: v for k, v in vars(tracker.avg_dt).items() if k != 'window_size'},
                               'recent': {k: v for k, v in vars(tracker.recent_avg_dt).items() if k != 'window_size'},
                               'previous_time': tracker.prev_time}})
  return {'frame': sm.frame, 'topics': topics, 'ignore_alive': sm.ignore_alive.copy(), 'ignore_valid': sm.ignore_valid.copy(),
          'ignore_frequency': sm.ignore_average_freq.copy()}


def native_snapshot(row):
  state = row['state']
  if state is not None:
    state['policy']['pose_calibrator'].pop('rpy')
    with car.CarState.from_bytes(bytes(state['previous'])) as message:
      state['previous'] = message.to_dict()
  health = row['health']
  if health is not None:
    for topic in health['topics']:
      topic['service'] = topic['service']['name']
  return row


def normalize(value):
  """Normalize transport-only containers and nonfinite snapshot values."""
  match value:
    case float() if not math.isfinite(value):
      return None
    case dict():
      return {key: normalize(child) for key, child in value.items()}
    case list() | tuple():
      return [normalize(child) for child in value]
    case _:
      return value


def compress_buffers(value):
  match value:
    case dict():
      result = {}
      for key, child in value.items():
        if key == 'buffer':
          runs = []
          for element in child:
            if runs and runs[-1][1] == element:
              runs[-1][0] += 1
            else:
              runs.append([1, element])
          result[key] = runs
        else:
          result[key] = compress_buffers(child)
      return result
    case list():
      return [compress_buffers(child) for child in value]
    case _:
      return value
