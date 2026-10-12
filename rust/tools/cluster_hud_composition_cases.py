from __future__ import annotations

from dataclasses import replace
import itertools


def cases(root):
  from cluster_navigation_render_cases import cases as navigation_cases
  from cluster_scene_compare import source

  _, models, _, base = source(root)
  navigation = next(case["state"] for case in navigation_cases(root) if case["name"] == "light-ko-live-media")
  moving = replace(
    navigation,
    onroad=True,
    gear_text="D",
    speed_kph=88.5,
    display_speed_kph=89.0,
    cruise_kph=100,
    cruise_display_state="engaged",
    cruise_gap=3,
    lfa_active=True,
    steering_angle_deg=12.5,
    active_lane_line=True,
    accel_mps2=-0.45,
    steering_output=240.0,
    steering_output_normalized=0.24,
    steering_output_kind="torque",
    fuel_gauge=0.72,
    urea_gauge=0.55,
    network_connected=True,
    network_address="192.0.2.253",
    actual_fps=19.85,
    center_clock_text="23:59",
    egpu_active=True,
    git_status=models.GitBranchStatus(branch="rust-runtime", state="ok", detail=""),
    debug_plot=models.DebugPlotSnapshot(mode=1, title="조향 상태", values=(0.1, 0.2, -0.3)),
    external_nav_active=True,
    left_signal=True,
  )
  states = [
    ("standby", base),
    ("moving", moving),
    ("parked", replace(moving, gear_text="P")),
    ("camera", replace(moving, camera_view_mode=2, right_signal=True)),
  ]
  for theme, small, mode in itertools.product(["light", "dark"], [False, True], range(-1, 7)):
    for name, state in states:
      yield {
        "name": f"{theme}-hud-{mode}-{name}-{'swapped' if small else 'normal'}",
        "surface": "hud",
        "options": {
          "width": 1280 if small else 1920,
          "height": 320 if small else 480,
          "theme_mode": theme,
          "language": "en" if small else "ko",
          "is_metric": not small,
          "screen_mode": mode,
          "panel_layout": int(small),
        },
        "state": state,
        "monotonic": 102.5,
        "wall_time": 1791792000.0,
      }
