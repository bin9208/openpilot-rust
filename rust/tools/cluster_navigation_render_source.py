from __future__ import annotations

import argparse
from dataclasses import asdict
import hashlib
import json
from pathlib import Path
import sys
import time
from types import SimpleNamespace

from cluster_navigation_render_cases import cases, serializable


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument("root", type=Path)
  parser.add_argument("output", type=Path)
  parser.add_argument("--prepare-only", action="store_true")
  parser.add_argument("--suite", choices=("navigation", "panels", "widgets", "alerts", "status", "plots", "hud", "replay"), default="navigation")
  args = parser.parse_args()
  root, output = args.root.resolve(), args.output.resolve()
  sys.path.insert(0, str(root))
  sys.path.insert(0, str(root / "openpilot/selfdrive/carrot/cluster"))
  import cluster_renderer as source
  import pyray as rl
  from cluster_system_monitor import SystemStats

  output.mkdir(parents=True, exist_ok=True)
  if args.suite == "panels":
    from cluster_panel_render_cases import cases as selected_cases
  elif args.suite == "widgets":
    from cluster_hud_render_cases import cases as selected_cases
  elif args.suite == "alerts":
    from cluster_alert_render_cases import cases as selected_cases
  elif args.suite == "status":
    from cluster_status_render_cases import cases as selected_cases
  elif args.suite == "plots":
    from cluster_plot_render_cases import cases as selected_cases
  elif args.suite == "hud":
    from cluster_hud_composition_cases import cases as selected_cases
  elif args.suite == "replay":
    from cluster_replay_render_cases import cases as selected_cases
  else:
    selected_cases = cases
  records = list(selected_cases(root))
  stats = SystemStats(
    memory_total_bytes=8 * 1024**3,
    memory_used_bytes=3 * 1024**3,
    memory_used_percent=37.5,
    cpu_core_percents=(12.5, 25.0, None, 50.0, 62.5, 75.0, 87.5, 99.0),
    cpu_used_percent=53.5,
    disk_used_percent=41.25,
  )
  request = {"system_stats": asdict(stats), "cases": [serializable(case) for case in records]}
  (output / "input.json").write_text(json.dumps(request, ensure_ascii=False))
  if args.prepare_only:
    print(json.dumps({"cases": len(records), "input": str(output / "input.json")}))
    return
  renderer = None
  dimensions = None
  target = None
  captures = []
  try:
    for index, case in enumerate(records):
      options = case["options"]
      size = (options["width"], options["height"])
      if size != dimensions:
        if target is not None:
          rl.unload_render_texture(target)
        if renderer is not None:
          renderer.close()
        renderer = source.ClusterUiRenderer(**options)
        renderer._system_stats.close()
        renderer._system_stats = SimpleNamespace(sample=lambda: stats, close=lambda: None)
        renderer.open(hidden=True)
        target = rl.load_render_texture(*size)
        dimensions = size
      renderer.set_theme_mode(options["theme_mode"])
      renderer.set_screen_mode(options["screen_mode"])
      renderer.set_panel_layout(options["panel_layout"])
      renderer.set_display_preferences(options["language"], options["is_metric"])
      source.time = SimpleNamespace(
        monotonic=lambda value=case["monotonic"]: value,
        perf_counter=lambda value=case["monotonic"]: value,
        time=lambda value=case["wall_time"]: value,
        strftime=time.strftime,
        localtime=time.localtime,
      )
      state = case["state"]
      for warm in range(2):
        rl.begin_texture_mode(target)
        rl.clear_background(source.rl_color(renderer._current_theme().bg))
        rl.rl_push_matrix()
        if case["surface"] not in ("alerts", "hud", "replay"):
          rl.rl_scalef(size[0] / 1920.0, size[1] / 480.0, 1.0)
        try:
          match case["surface"]:
            case "replay":
              replay = case["replay"]
              renderer.camera_overlay_pitch_offset_deg = replay["pitch_deg"]
              renderer.camera_overlay_z_offset_m = replay["height_m"]
              renderer._draw_route_replay_controls(replay["playback_s"], replay["duration_s"], 0.0, replay["paused"], replay["tuning_visible"])
            case "hud":
              renderer._draw_hud(state)
            case "plot-full" | "plot-right" | "plot-compact":
              bounds = {"plot-full": (500.0, 18.0, 1392.0, 444.0), "plot-right": (1124.0, 1.0, 792.0, 478.0), "plot-compact": (100.0, 20.0, 650.0, 440.0)}[
                case["surface"]
              ]
              renderer._draw_debug_plot(state.debug_plot, *bounds)
            case "status":
              renderer._draw_drive_status(state)
              renderer._draw_git_status(state.git_status, state.network_address, state.actual_fps)
              renderer._draw_cluster_core_usage(state.cluster_core_usage_text)
            case "speed":
              renderer._draw_speed_block(state)
            case "route":
              renderer._draw_route_overlay(state.route_overlay)
            case "alerts":
              renderer._draw_alert_overlay(state.alert)
            case "clock":
              renderer._draw_center_clock(state)
            case "gauges":
              renderer._draw_accel_block(state)
              renderer._draw_steering_output_block(state)
            case "signals":
              renderer._draw_turn_signal("left", state.left_signal, show_inactive=state.debug_ui_visible)
              renderer._draw_turn_signal("right", state.right_signal, show_inactive=state.debug_ui_visible)
            case "system":
              renderer._draw_system_dashboard_panel(state)
            case "trip":
              renderer._draw_trip_report_panel_contents(state)
            case "live-debug":
              renderer._draw_live_debug_panel(state)
            case "dashboard":
              renderer._draw_navi_dashboard(state)
            case "live":
              renderer._draw_navi_live_panel(state)
            case "debug":
              renderer._draw_navi_debug_panel(state.navi_debug)
            case "traffic":
              renderer._draw_navi_traffic_light_panel(state.navi_debug.traffic_light)
            case _:
              raise ValueError(case["surface"])
        finally:
          rl.rl_pop_matrix()
          rl.end_texture_mode()
        image = rl.load_image_from_texture(target.texture)
        try:
          rl.image_flip_vertical(image)
          path = output / f"{case['name']}-{warm}.png"
          assert rl.export_image(image, str(path)), path
        finally:
          rl.unload_image(image)
        renderer._flush_pending_stroked_text_textures()
        captures.append(
          {"name": case["name"], "warm": warm, "width": size[0], "height": size[1], "path": str(path), "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}
        )
      if index % 20 == 0:
        print(f"source navigation {index + 1}/{len(records)}", flush=True)
  finally:
    if target is not None:
      rl.unload_render_texture(target)
    if renderer is not None:
      renderer.close()
  source_path = root / "openpilot/selfdrive/carrot/cluster/cluster_renderer.py"
  result = {
    "cases": len(records),
    "frames": len(captures),
    "captures": captures,
    "source_sha256": hashlib.sha256(source_path.read_bytes()).hexdigest(),
    "scope": "Unchanged original navigation drawing methods, actual raylib, fixed external clocks/system-stat inputs",
  }
  (output / "source-result.json").write_text(json.dumps(result, indent=2))
  print(json.dumps({"cases": len(records), "frames": len(captures)}))


if __name__ == "__main__":
  main()
