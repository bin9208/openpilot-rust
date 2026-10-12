# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
from __future__ import annotations

from dataclasses import replace
from enum import StrEnum
import json
from pathlib import Path
import sys
import time
from types import SimpleNamespace
from typing import assert_never

from cluster_navigation_render_cases import serializable
from cluster_scene_compare import source as load_source


class Stage(StrEnum):
  IMPLICIT_FIRST = "implicit-first"
  REOPEN_HIDDEN = "reopen-hidden"
  REOPEN_VISIBLE = "reopen-visible"


def main() -> None:
  root, input_path = (Path(value).resolve() for value in sys.argv[1:3])
  fps = int(sys.argv[3]) if len(sys.argv) == 4 else 0
  sys.path.insert(0, str(root / "openpilot/selfdrive/carrot/cluster"))
  import cluster_renderer as source
  from cluster_system_monitor import SystemStats
  _, _, _, base = load_source(root)
  options = {"width": 320, "height": 80, "title": "Owned output visibility", "target_fps": fps, "theme_mode": "light", "screen_mode": 3, "language": "en", "is_metric": True}
  state = replace(base, onroad=False, camera_view_mode=2)
  input_path.write_text(json.dumps({"options": options, "state": serializable({"state": state})["state"]}))
  renderer = source.ClusterUiRenderer(**options)
  renderer._system_stats.close()
  renderer._system_stats = SimpleNamespace(sample=lambda: SystemStats(), close=lambda: None)
  renderer.profile_enabled = True
  source.time = SimpleNamespace(monotonic=lambda: 100.0, perf_counter=lambda: 100.0, time=lambda: 1791792000.0, strftime=time.strftime, localtime=time.localtime)
  try:
    for stage in Stage:
      match stage:
        case Stage.IMPLICIT_FIRST:
          pass
        case Stage.REOPEN_HIDDEN:
          renderer.close()
          renderer.open(hidden=True)
          renderer.close()
        case Stage.REOPEN_VISIBLE:
          renderer.close()
          renderer.open(hidden=False)
          renderer.close()
        case unreachable:
          assert_never(unreachable)
      renderer._profile_samples.clear()
      renderer.render_to_rgba_bytes(state)
      print(json.dumps({"stage": stage, "profile_names": [name for name, _elapsed in renderer._profile_samples]}), flush=True)
      if not sys.stdin.readline():
        raise EOFError
  finally:
    renderer.close()


if __name__ == "__main__":
  main()
