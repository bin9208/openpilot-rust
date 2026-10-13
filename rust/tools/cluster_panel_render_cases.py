from __future__ import annotations

from dataclasses import replace
import itertools
import math


def cases(root):
  from cluster_scene_compare import source

  _, models, _, base = source(root)
  report = models.TripReportState(
    duration_s=3661.5,
    moving_time_s=3520.0,
    distance_m=12345.6,
    average_speed_kph=62.25,
    max_speed_kph=121.5,
    max_accel_mps2=2.75,
    max_decel_mps2=-4.125,
    auto_ratio_percent=82.5,
    hard_accel_count=3,
    hard_brake_count=12,
    hard_corner_count=105,
  )
  full = replace(
    base,
    trip_report=report,
    onroad=True,
    actual_fps=19.85,
    network_address="192.0.2.253",
    cpu_usage_percent=91.0,
    cpu_temp_c=90.0,
    memory_used_percent=60.0,
    disk_used_percent=85.0,
    camera_calibration_euler=(0.0, math.radians(2.75), math.radians(-3.25)),
  )
  debug = models.LiveDebugInfo(
    live_delay_calibration_percent=82.5,
    live_delay_lateral_s=0.123,
    live_torque_calibration_percent=99.5,
    live_torque_valid=True,
    live_torque_lat_accel_factor=2.125,
    live_torque_friction=0.095,
    live_steer_ratio=16.75,
    custom_steer_ratio=15.25,
    steer_actuator_delay_s=0.24,
  )
  variants = [("system-empty", "system", base), ("system-values", "system", full)]
  for index, (cpu, temp, memory, disk, pitch, yaw) in enumerate(
    [
      (0.0, 74.5, 59.5, 84.5, 0.0, 0.0),
      (-12.0, 75.0, 105.0, -3.0, -10.0, 10.0),
      (60.0, 89.5, 85.0, 100.0, 6.0, -6.0),
    ]
  ):
    variants.append(
      (
        f"system-boundary-{index}",
        "system",
        replace(
          full,
          cpu_usage_percent=cpu,
          cpu_temp_c=temp,
          memory_used_percent=memory,
          disk_used_percent=disk,
          camera_calibration_euler=(0.0, math.radians(pitch), math.radians(yaw)),
        ),
      )
    )
  variants.extend(
    [
      ("trip-empty", "trip", base),
      ("trip-values", "trip", full),
      ("trip-negative", "trip", replace(full, trip_report=replace(report, duration_s=-2.0, distance_m=-1.0))),
      ("trip-time-tie", "trip", replace(full, trip_report=replace(report, duration_s=3599.5, distance_m=999.5))),
      ("debug-empty", "live-debug", base),
      ("debug-values", "live-debug", replace(full, live_debug=debug, lateral_plan_debug_text="서울 차로 유지 " * 12)),
      ("debug-off", "live-debug", replace(full, live_debug=replace(debug, live_torque_valid=False), lateral_plan_debug_text="cost=2.35")),
    ]
  )
  for theme, language, swapped in itertools.product(["light", "dark"], ["ko", "en"], [False, True]):
    for name, surface, state in variants:
      yield {
        "name": f"{theme}-{language}-{name}-{'swapped' if swapped else 'normal'}",
        "surface": surface,
        "options": {
          "width": 1280 if swapped else 1920,
          "height": 320 if swapped else 480,
          "theme_mode": theme,
          "language": language,
          "is_metric": language == "ko",
          "screen_mode": 4,
          "panel_layout": int(swapped),
        },
        "state": state,
        "monotonic": 102.5,
        "wall_time": 1791792000.0,
      }
