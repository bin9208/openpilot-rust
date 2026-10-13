from __future__ import annotations

from dataclasses import asdict, replace
import hashlib
import json
from pathlib import Path
import sys
import time
from types import SimpleNamespace


def main() -> None:
  root_arg, output_arg = sys.argv[1:3]
  png = len(sys.argv) > 3 and sys.argv[3] == "--png"
  rgba = len(sys.argv) > 3 and sys.argv[3] == "--rgba"
  root, output = Path(root_arg).resolve(), Path(output_arg).resolve()
  sys.path.insert(0, str(root))
  sys.path.insert(0, str(root / "openpilot/selfdrive/carrot/cluster"))
  import cluster_renderer as source
  import cluster_models as models
  import pyray as rl
  from cluster_system_monitor import SystemStats
  from cluster_hud_composition_cases import cases
  from cluster_navigation_render_cases import serializable

  records = list(cases(root))
  pixels = bytes(channel for y in range(24) for x in range(32) for channel in ((x * 7) % 256, (y * 11) % 256, (x + y) * 4 % 256, 255))
  video = models.RouteOverlay(video_rgba=pixels, video_width=32, video_height=24, video_frame_id="owned-frame", camera_stream="wide")
  for case in records:
    case["surface"] = "frame"
    case["capture"] = dict(portrait_upload=False, output_width=None, output_height=None)
    if case["state"].camera_view_mode in (2, 3, 4):
      case["state"] = replace(case["state"], route_overlay=video)
  for index, original in enumerate(records[:8]):
    for label, width, height in (("portrait", None, None), ("portrait-small", 320, 1280), ("portrait-zero", 0, 0)):
      records.append(dict(original, name=f"{original['name']}-{label}-{index}", capture=dict(portrait_upload=True, output_width=width, output_height=height)))
  if png or rgba:
    selected = [records[index] for index in (0, 1, 3, 12, 24, 31, 64, 65, 67, 96)]
    records = [dict(case, name=f"{case['name']}-png-{'portrait' if portrait else 'landscape'}", portrait=portrait) for case in selected for portrait in (False, True)]
  if rgba:
    records = [dict(case, name=f"{case['name']}-rgba-{'error' if error else 'copy'}", callback_error=error) for case in records for error in (False, True)]
  stats = SystemStats(memory_total_bytes=8 * 1024**3, memory_used_bytes=3 * 1024**3, memory_used_percent=37.5, cpu_core_percents=(12.5, 25.0, None, 50.0, 62.5, 75.0, 87.5, 99.0), cpu_used_percent=53.5, disk_used_percent=41.25)
  output.mkdir(parents=True, exist_ok=True)
  (output / "input.json").write_text(json.dumps(dict(system_stats=asdict(stats), cases=[serializable(case) for case in records]), ensure_ascii=False))
  renderer, dimensions, captures = None, None, []
  try:
    for case in records:
      options = case["options"]
      size = options["width"], options["height"]
      if size != dimensions or png or rgba:
        if renderer is not None:
          renderer.close()
        renderer = source.ClusterUiRenderer(**options)
        renderer._system_stats.close()
        renderer._system_stats = SimpleNamespace(sample=lambda: stats, close=lambda: None)
        renderer.open(hidden=True)
        renderer.set_profile_enabled(True)
        dimensions = size
      renderer.set_theme_mode(options["theme_mode"])
      renderer.set_screen_mode(options["screen_mode"])
      renderer.set_panel_layout(options["panel_layout"])
      renderer.set_display_preferences(options["language"], options["is_metric"])
      source.time = SimpleNamespace(monotonic=lambda value=case["monotonic"]: value, perf_counter=lambda value=case["monotonic"]: value, time=lambda value=case["wall_time"]: value, strftime=time.strftime, localtime=time.localtime)
      for warm in range(2):
        renderer.clear_profile_samples()
        path = output / f"{case['name']}-{warm}.png"
        error_profile_names = []
        if png:
          path.write_bytes(renderer.render_to_png_bytes(case["state"], portrait_upload=case["portrait"]))
          width, height = (size[1], size[0]) if case["portrait"] else size
        elif rgba:
          from PIL import Image

          if case["callback_error"]:
            try:
              with renderer.render_to_rgba_buffer(case["state"], portrait_upload=case["portrait"]) as (buffer, width, height):
                assert len(buffer) == width * height * 4
                raise RuntimeError("owned callback error")
            except RuntimeError as error:
              assert str(error) == "owned callback error"
            error_profile_names = [name for name, _ in renderer.profile_samples()]
            renderer.clear_profile_samples()
          pixels, width, height = renderer.render_to_rgba_bytes(case["state"], portrait_upload=case["portrait"])
          path.with_suffix(".rgba").write_bytes(pixels)
          Image.frombytes("RGBA", (width, height), pixels).save(path)
        else:
          image = renderer._render_to_image(case["state"], **case["capture"])
          try:
            assert rl.export_image(image, str(path))
            width, height = image.width, image.height
          finally:
            rl.unload_image(image)
        row = dict(name=case["name"], warm=warm, width=width, height=height, path=str(path), sha256=hashlib.sha256(path.read_bytes()).hexdigest(), profile_names=[name for name, _ in renderer.profile_samples()])
        if rgba:
          row["error_profile_names"] = error_profile_names
        captures.append(row)
  finally:
    if renderer is not None:
      renderer.close()
  (output / "source-result.json").write_text(json.dumps(dict(cases=len(records), frames=len(captures), captures=captures, source_sha256=hashlib.sha256((root / "openpilot/selfdrive/carrot/cluster/cluster_renderer.py").read_bytes()).hexdigest()), indent=2))
  print(json.dumps(dict(cases=len(records), frames=len(captures))))


if __name__ == "__main__":
  main()
