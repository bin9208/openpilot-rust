from __future__ import annotations

from dataclasses import replace
import itertools


def cases(root):
  from cluster_scene_compare import source

  _, _, _, base = source(root)
  variants = [("gauges-empty", "gauges", base)]
  for index, (accel, output, normalized, kind, fuel, urea, label) in enumerate(
    [
      (-0.004, 0.0, 0.0, "torque", 0.15, 0.15, "fuel"),
      (0.005, 999.0, 0.5, "torque", 0.1500001, 0.1500001, "fuel"),
      (-5.0, 1000.0, -1.2, "torque", 1.5, -1.0, "battery"),
      (5.0, 25.5, 1.2, "angle", 0.0, 1.0, "battery"),
      (-0.006, -24.5, -0.3, "angle", 0.73, None, "battery"),
    ]
  ):
    for camera in (0, 2):
      variants.append(
        (
          f"gauges-{index}-camera{camera}",
          "gauges",
          replace(
            base,
            accel_mps2=accel,
            steering_output=output,
            steering_output_normalized=normalized,
            steering_output_kind=kind,
            fuel_gauge=fuel,
            urea_gauge=urea,
            energy_gauge_label=label,
            camera_view_mode=camera,
          ),
        )
      )
  for left, right, inactive in itertools.product([False, True], repeat=3):
    variants.append(
      (
        f"signals-{int(left)}{int(right)}{int(inactive)}",
        "signals",
        replace(
          base,
          left_signal=left,
          right_signal=right,
          debug_ui_visible=inactive,
        ),
      )
    )
  for theme, language, small in itertools.product(["light", "dark"], ["ko", "en"], [False, True]):
    for name, surface, state in variants:
      yield {
        "name": f"{theme}-{language}-{name}-{'small' if small else 'normal'}",
        "surface": surface,
        "options": {
          "width": 1280 if small else 1920,
          "height": 320 if small else 480,
          "theme_mode": theme,
          "language": language,
          "is_metric": language == "ko",
          "screen_mode": 0,
          "panel_layout": 0,
        },
        "state": state,
        "monotonic": 102.5,
        "wall_time": 1791792000.0,
      }
