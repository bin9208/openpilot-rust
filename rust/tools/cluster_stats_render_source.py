# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
from __future__ import annotations

from dataclasses import asdict
import hashlib
import json
from pathlib import Path
import sys


def main() -> None:
  root, output = (Path(value).resolve() for value in sys.argv[1:])
  sys.path.insert(0, str(root / "openpilot/selfdrive/carrot/cluster"))
  import cluster_renderer as source
  from cluster_scene_compare import source as source_types
  import pyray as rl
  from cluster_system_monitor import SystemStats
  _, _, _, state = source_types(root)

  output.mkdir(parents=True, exist_ok=True)
  snapshots = [
    ("normal", SystemStats(8 * 1024**3, 3 * 1024**3, 37.5, (12.5, None, 60.0, 85.0), 52.5, None)),
    ("large", SystemStats(18889465931478580853760, 18889465931478580853760 // 3, 100 / 3, (0.0, 100.0), 50.0, None)),
    ("negative", SystemStats(-102400, None, None, (None, 100.0), 100.0, None)),
    ("missing", SystemStats()),
    ("many-cores", SystemStats(102400, 81920, 80.0, tuple(index * 6.25 for index in range(17)), 50.0, None)),
    ("float-overflow", SystemStats(10**500, 10**500 // 2, 50.0, (25.0,), 25.0, None)),
  ]
  cases = []
  for theme in ("light", "dark"):
    for width, height in ((1920, 480), (1280, 320)):
      for detailed in (False, True):
        for name, stats in snapshots:
          options = dict(width=width, height=height, target_fps=0, theme_mode=theme, language="ko", is_metric=True, screen_mode=0, panel_layout=0)
          cases.append(dict(name=f"{theme}-{width}-{int(detailed)}-{name}", options=options, stats=asdict(stats), detailed=detailed))
  (output / "input.json").write_text(json.dumps(dict(cases=cases)))
  observations = []
  for case in cases:
    renderer = source.ClusterUiRenderer(**case["options"])
    renderer.open(hidden=True)
    size = (case["options"]["width"], case["options"]["height"])
    target = rl.load_render_texture(*size)
    try:
      for warm in range(2):
        error = None
        rl.begin_texture_mode(target)
        rl.clear_background(source.rl_color(renderer._current_theme().bg))
        rl.rl_push_matrix()
        rl.rl_scalef(size[0] / 1920.0, size[1] / 480.0, 1.0)
        try:
          renderer._draw_system_stats_panel(state, stats=SystemStats(**case["stats"]), show_runtime_info=case["detailed"])
        except OverflowError as failure:
          error = str(failure)
        finally:
          rl.rl_pop_matrix()
          rl.end_texture_mode()
        path = None
        if error is None:
          image = rl.load_image_from_texture(target.texture)
          try:
            rl.image_flip_vertical(image)
            path = output / f"{case['name']}-{warm}.png"
            assert rl.export_image(image, str(path))
          finally:
            rl.unload_image(image)
        renderer._flush_pending_stroked_text_textures()
        observations.append(dict(name=case["name"], warm=warm, error=error, path=str(path) if path else None))
    finally:
      rl.unload_render_texture(target)
      renderer.close()
  (output / "result.json").write_text(json.dumps(observations, indent=2))
  (output / "source-sha256.json").write_text(json.dumps({str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in (root / "openpilot/selfdrive/carrot/cluster/cluster_renderer.py", root / "openpilot/selfdrive/carrot/cluster/cluster_system_monitor.py")}))


if __name__ == "__main__":
  main()
