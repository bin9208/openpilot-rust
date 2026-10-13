from __future__ import annotations

from dataclasses import replace
import itertools


def cases(root):
  from cluster_model_types import default_record
  from cluster_scene_compare import source

  _, models, _, base = source(root)
  variants = [("status-empty", "status", base), ("speed-empty", "speed", base)]
  for index, (status, active, angle, lane, network, egpu) in enumerate(
    [
      ("ok", True, 35.0, True, True, True),
      ("pull", False, -35.0, True, False, False),
      ("missing", None, 0.0, False, True, False),
      ("unknown", True, 120.0, False, False, True),
    ]
  ):
    variants.append(
      (
        f"status-{index}",
        "status",
        replace(
          base,
          gear_text="D",
          lfa_active=active,
          steering_angle_deg=angle,
          active_lane_line=lane,
          network_connected=network,
          egpu_active=egpu,
          network_address="192.0.2.253:8082",
          actual_fps=19.85,
          cluster_core_usage_text="C0 25% C1 50% C6 75% C7 99%",
          git_status=models.GitBranchStatus(branch="rust-port-long-branch-name-" * 3, state=status, detail="updates available" if index == 1 else ""),
        ),
      )
    )
  for index, (speed, cruise, mode, gear, override, apply, limit, traffic) in enumerate(
    [
      (0.0, None, "off", "P", None, 0, None, 0),
      (88.5, 100, "engaged", "D", 75.5, 1, 80, 1),
      (105.5, 123, "paused", "U", 80.5, 2, 120, 2),
      (-2.0, 0, "engaged", " r ", 0.0, 3, 30, 3),
      (999.0, 150, "engaged", "long", 123.5, 4, 125, 1),
    ]
  ):
    variants.append(
      (
        f"speed-{index}",
        "speed",
        replace(
          base,
          speed_kph=speed,
          cruise_kph=cruise,
          cruise_display_state=mode,
          gear_text=gear,
          cruise_override_kph=override,
          cruise_override_label="긴 내비게이션 감속 사유",
          cruise_override_color_mode=apply,
          speed_limit_kph=limit,
          traffic_state=traffic,
          driving_mode=index,
          cruise_gap=index,
          ev_mode_valid=True,
          ev_mode_active=index % 2 == 0,
          camera_view_mode=2 if index == 3 else 0,
          tpms=models.TpmsInfo(fl=30.5, fr=31.0, rl=None, rr=36.5),
        ),
      )
    )
  variants.extend(
    [
      ("route-none", "route", base),
      ("route-hidden", "route", replace(base, route_overlay=replace(default_record(models.RouteOverlay), panel_visible=False))),
      ("route-wait", "route", replace(base, route_overlay=replace(default_record(models.RouteOverlay), video_status="qcamera loading"))),
    ]
  )
  pixels = bytes([20, 90, 180, 220]) * (320 * 160)
  video = replace(
    default_record(models.RouteOverlay),
    video_rgba=pixels,
    video_width=320,
    video_height=160,
    video_frame_id="frame-a",
    cutin_status="NEW CUTIN: YES | vehicle 42",
    data_lines=tuple(f"detail row {i}" for i in range(8)),
  )
  variants.extend(
    [
      ("route-video", "route", replace(base, route_overlay=video)),
      ("route-cached", "route", replace(base, route_overlay=replace(video, video_rgba=bytes([240, 0, 0, 255]) * (320 * 160)))),
      ("route-invalid-retain", "route", replace(base, route_overlay=replace(video, video_frame_id="frame-b", video_rgba=b"bad"))),
      (
        "route-video-new",
        "route",
        replace(
          base, route_overlay=replace(video, video_frame_id="frame-c", video_rgba=bytes([200, 60, 20, 255]) * (320 * 160), cutin_status="NEW CUTIN: waiting")
        ),
      ),
    ]
  )
  for theme, language, small in itertools.product(["light", "dark"], ["ko", "en"], [False, True]):
    for name, surface, state in variants:
      yield {
        "name": f"{theme}-{language}-{name}-{'swapped' if small else 'normal'}",
        "surface": surface,
        "options": {
          "width": 1280 if small else 1920,
          "height": 320 if small else 480,
          "theme_mode": theme,
          "language": language,
          "is_metric": language == "ko",
          "screen_mode": 0,
          "panel_layout": int(small),
        },
        "state": state,
        "monotonic": 102.5,
        "wall_time": 1791792000.0,
      }
