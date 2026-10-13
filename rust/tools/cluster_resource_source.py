from __future__ import annotations

import argparse
from dataclasses import asdict, replace
import hashlib
import json
from pathlib import Path
import sys
from types import SimpleNamespace


def cases(root):
  from cluster_scene_compare import source
  from cluster_navigation_render_cases import serializable
  _, models, _, base = source(root)
  records = []
  for theme in ("light", "dark"):
    for language in ("ko", "en"):
      options = dict(width=1920, height=480, theme_mode=theme, language=language, screen_mode=5, panel_layout=0, is_metric=language == "ko")
      prefix = f"{theme}-{language}"
      state = replace(base, onroad=True, gear_text="P", trip_report=models.TripReportState(duration_s=100.0, distance_m=1200.0, average_speed_kph=40.0))
      changed = replace(state, trip_report=replace(state.trip_report, duration_s=200.0, distance_m=9400.0))
      trip_steps = [("initial", state, 100.0, True, False), ("stale", changed, 100.5, False, False), ("before-refresh", changed, 100.999, False, False), ("refresh", changed, 101.0, False, False), ("invalidate", state, 101.1, True, False), ("hidden", state, 101.2, True, False), ("reshown", changed, 101.3, True, False), ("swapped", state, 101.4, True, False), ("reopen", changed, 101.5, False, True)]
      for label, value, now, setters, reopen in trip_steps:
        settings = dict(options, screen_mode=4 if label == "hidden" else 5, panel_layout=int(label == "swapped"))
        if label == "reopen":
          settings["panel_layout"] = 1
        records.append(dict(name=f"{prefix}-trip-{label}", action="trip", options=settings, state=value, overlay=None, now=now, setters=setters, reopen=reopen))
      for index, (label, width, height, frame_id, payload) in enumerate([
        ("none-id", 8, 4, None, [220, 20, 80, 255]),
        ("first", 8, 4, "one", [220, 20, 80, 255]),
        ("same-id", 8, 4, "one", [20, 220, 80, 255]),
        ("bad-length", 8, 4, "two", None),
        ("new-id", 8, 4, "two", [20, 220, 80, 255]),
        ("resize", 4, 8, "two", [40, 80, 220, 255]),
        ("resize-none-id", 6, 6, None, [200, 200, 40, 255]),
        ("after-resize", 6, 6, "three", [200, 200, 40, 255]),
      ]):
        data = bytes(payload * width * height) if payload else b"bad"
        overlay = models.RouteOverlay(video_rgba=data, video_width=width, video_height=height, video_frame_id=frame_id)
        records.append(dict(name=f"{prefix}-route-{label}", action="route", options=options, state=state, overlay=overlay, now=102.0 + index, setters=index == 0, reopen=False))
  encoded = [serializable(dict(row, overlay=asdict(row["overlay"]) if row["overlay"] is not None else None)) for row in records]
  return records, encoded


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument("root", type=Path)
  parser.add_argument("output", type=Path)
  parser.add_argument("--prepare-only", action="store_true")
  parser.add_argument("--media", action="store_true")
  args = parser.parse_args()
  root, output = args.root.resolve(), args.output.resolve()
  sys.path.insert(0, str(root))
  sys.path.insert(0, str(root / "openpilot/selfdrive/carrot/cluster"))
  import cluster_renderer as source
  import pyray as rl
  from cluster_system_monitor import SystemStats
  if args.media:
    from cluster_media_resource_cases import cases as media_cases
    records, encoded = media_cases(root)
  else:
    records, encoded = cases(root)
  stats = SystemStats(memory_total_bytes=8*1024**3, memory_used_bytes=3*1024**3, memory_used_percent=37.5, cpu_core_percents=(12.5, 25.0, None, 50.0), cpu_used_percent=53.5, disk_used_percent=41.25)
  output.mkdir(parents=True, exist_ok=True)
  (output / "input.json").write_text(json.dumps(dict(system_stats=asdict(stats), cases=encoded), ensure_ascii=False))
  if args.prepare_only:
    return
  renderer = source.ClusterUiRenderer(**records[0]["options"])
  renderer._system_stats.close()
  renderer._system_stats = SimpleNamespace(sample=lambda: stats, close=lambda: None)
  renderer.open(hidden=True)
  renderer.set_profile_enabled(True)
  target = rl.load_render_texture(1920, 480)
  captures, previous = [], None
  try:
    for case in records:
      options = case["options"]
      if case["reopen"]:
        rl.unload_render_texture(target)
        renderer.close()
        renderer.open(hidden=True)
        target = rl.load_render_texture(1920, 480)
        previous = None
      if case["setters"]:
        renderer.set_theme_mode(options["theme_mode"])
        renderer.set_screen_mode(options["screen_mode"])
        renderer.set_panel_layout(options["panel_layout"])
        renderer.set_display_preferences(options["language"], options["is_metric"])
      renderer.clear_profile_samples()
      texture = None
      fitted = None
      if case["action"] == "trip":
        renderer._prepare_trip_report_cache(case["state"], case["now"])
      elif case["action"] == "route":
        texture = renderer._route_video_texture_for_overlay(case["overlay"])
      elif case["action"] == "media":
        fitted = renderer._navi_media_fitted_size(case["frame"], rl.Rectangle(30.25, 18.75, 530.5, 270.25))
      reuse = texture.id == previous[0] if texture is not None and previous is not None and (texture.width, texture.height) == previous[1:] else None
      previous = (texture.id, texture.width, texture.height) if texture is not None else None
      rl.begin_texture_mode(target)
      try:
        rl.clear_background(source.rl_color(renderer._current_theme().bg))
        if texture is not None:
          rl.draw_texture_pro(texture, rl.Rectangle(0.0, 0.0, texture.width, texture.height), rl.Rectangle(30.25, 18.75, 530.5, 270.25), rl.Vector2(0.0, 0.0), 0.0, rl.WHITE)
        if case["action"] == "trip":
          renderer._draw_trip_report_panel(case["state"])
        if case["action"] == "media":
          renderer._draw_navi_media(case["frame"], rl.Rectangle(30.25, 18.75, 530.5, 270.25))
      finally:
        rl.end_texture_mode()
      image = rl.load_image_from_texture(target.texture)
      path = output / f"{case['name']}-0.png"
      try:
        rl.image_flip_vertical(image)
        assert rl.export_image(image, str(path))
      finally:
        rl.unload_image(image)
      renderer._flush_pending_stroked_text_textures()
      captures.append(dict(name=case["name"], warm=0, width=1920, height=480, path=str(path), sha256=hashlib.sha256(path.read_bytes()).hexdigest(), fitted=fitted, reuse_previous=reuse, profile_names=[name for name, _ in renderer.profile_samples()]))
  finally:
    rl.unload_render_texture(target)
    renderer.close()
  (output / "source-result.json").write_text(json.dumps(dict(cases=len(records), frames=len(captures), captures=captures, source_sha256=hashlib.sha256((root / "openpilot/selfdrive/carrot/cluster/cluster_renderer.py").read_bytes()).hexdigest()), indent=2))
  print(json.dumps(dict(frames=len(captures))))


if __name__ == "__main__":
  main()
