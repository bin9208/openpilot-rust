from __future__ import annotations

from dataclasses import replace
import itertools


def cases(root):
  from cluster_scene_compare import source

  _, models, _, base = source(root)
  for group, (theme, small, surface) in enumerate(itertools.product(["light", "dark"], [False, True], ["plot-full", "plot-right", "plot-compact"])):
    for index, (offset, mode, values) in enumerate(
      [
        (0.0, None, (0.0, 0.0, 0.0)),
        (0.01, 1, (1.0, 2.0, -3.0)),
        (0.059, 1, (9.0, 8.0, 7.0)),
        (0.061, 1, (-2.0, 1.25, 3.75)),
        (0.2, 1, (6.5, -7.25, 0.0)),
        (0.4, 2, (-0.1, 0.1, 0.2)),
        (0.6, 0, (1.0, 2.0, 3.0)),
        (0.8, 3, (100.0, -100.0, 0.0)),
      ]
    ):
      plot = None if mode is None else models.DebugPlotSnapshot(mode=mode, title="실시간 조향 가속도 그래프 " * 7, values=values)
      yield {
        "name": f"{theme}-{surface}-{'small' if small else 'normal'}-{index}",
        "surface": surface,
        "options": {
          "width": 1280 if small else 1920,
          "height": 320 if small else 480,
          "theme_mode": theme,
          "language": "ko",
          "is_metric": True,
          "screen_mode": 3,
          "panel_layout": 0,
        },
        "state": replace(base, debug_plot=plot),
        "monotonic": 1000.0 + group * 20.0 + offset,
        "wall_time": 1791792000.0,
      }
