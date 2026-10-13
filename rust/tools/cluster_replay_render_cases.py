from __future__ import annotations

import itertools


def cases(root):
  from cluster_scene_compare import source

  _, _, _, base = source(root)
  for theme, small in itertools.product(["light", "dark"], [False, True]):
    for index, (playback, duration, paused, tuning, pitch, height) in enumerate(
      [
        (-1.0, 0.0, False, False, 0.0, 0.0),
        (0.5, 59.5, True, False, 0.0, 0.0),
        (1.5, 60.5, False, True, -3.0, -0.8),
        (59.5, 120.0, True, True, 3.0, 0.4),
        (60.5, 120.0, False, True, -0.2, -0.05),
        (1000.0, 120.0, False, True, 0.2, 0.05),
        (90.01, 4000.0, True, True, -4.0, -1.0),
        (3599.5, 7200.0, False, True, 4.0, 1.0),
      ]
    ):
      yield {
        "name": f"{theme}-replay-{'small' if small else 'normal'}-{index}",
        "surface": "replay",
        "options": {
          "width": 1280 if small else 1920,
          "height": 320 if small else 480,
          "theme_mode": theme,
          "language": "en",
          "is_metric": True,
          "screen_mode": 0,
          "panel_layout": 0,
        },
        "state": base,
        "replay": {"playback_s": playback, "duration_s": duration, "paused": paused, "tuning_visible": tuning, "pitch_deg": pitch, "height_m": height},
        "monotonic": 1000.0,
        "wall_time": 1791792000.0,
      }
