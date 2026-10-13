from __future__ import annotations

import argparse
from dataclasses import replace
import hashlib
import itertools
import json
from pathlib import Path
import subprocess
import sys
from types import SimpleNamespace


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument("root", type=Path)
  parser.add_argument("output", type=Path)
  parser.add_argument("--binary", type=Path)
  args = parser.parse_args()
  root, output = args.root.resolve(), args.output.resolve()
  sys.path.insert(0, str(root))
  from cluster_model_types import default_record
  from cluster_scene_compare import source

  _, models, _, base = source(root)
  import cluster_renderer as original

  renderer = original.ClusterUiRenderer()
  modes, expected_modes = [], []
  try:
    for mode, camera, (onroad, gear), navigation in itertools.product(
      range(-1, 7), [0, 2, 3, 4], [(False, "P"), (True, "P"), (True, "D"), (True, "  p\x1f")], range(5)
    ):
      state = replace(base, camera_view_mode=camera, onroad=onroad, gear_text=gear, external_nav_active=navigation == 4)
      if navigation in (1, 2):
        state = replace(state, navi_dashboard=replace(default_record(models.NaviDashboardState), connected=navigation == 2))
      if navigation == 3:
        state = replace(state, navi_live=replace(default_record(models.NaviLiveState), current=default_record(models.NaviGuidanceInfo)))
      renderer.screen_mode = mode
      modes.append({"mode": mode, "camera": camera, "onroad": onroad, "gear": gear, "navigation": navigation})
      expected_modes.append(renderer._effective_screen_mode(state))

    signals, expected_signals = [], []
    for index in range(200):
      now = 100.0 + index * 0.125
      phase = index // 7 % 4
      left, right = phase in (1, 3), phase in (2, 3)
      highlight = [None, "left", "right", "invalid"][index // 11 % 4]
      offset = -1.0 if index % 9 else None
      state = replace(base, left_signal=left, right_signal=right, highlight_lane=highlight, highlight_lane_offset=offset)
      original.time = SimpleNamespace(perf_counter=lambda now=now: now)
      lights = renderer._turn_signal_lights(state)
      lit = renderer._highlight_lane_lit(state, lights)
      signals.append({"now": now, "left": left, "right": right, "highlight": highlight, "offset": offset})
      expected_signals.append({"lights": list(lights), "highlight": lit})

    plots, expected_plots = [], []
    now = 0.0
    for index in range(720):
      now += [0.05, 0.06, 0.049, 0.001, 0.1][index % 5]
      mode = 1 if index < 710 else 2 if index < 715 else 0 if index < 718 else 3
      values = [float(index % 29 - 14), index * 0.005, -index * 0.007]
      plots.append({"now": now, "mode": mode, "values": values})
      plot = models.DebugPlotSnapshot(mode=mode, title="oracle", values=tuple(values))
      renderer._draw_debug_plot_panel = lambda *_args: None
      original.time = SimpleNamespace(perf_counter=lambda now=now: now)
      renderer._draw_debug_plot(plot, 0.0, 0.0, 300.0, 200.0)
      expected_plots.append(
        {
          "bounds": [renderer._debug_plot_min, renderer._debug_plot_max],
          "samples": [[renderer._debug_plot_value(series, offset) for series in range(3)] for offset in range(renderer._debug_plot_size)],
        }
      )
  finally:
    renderer.close()
  output.mkdir(parents=True, exist_ok=True)
  inputs = {"modes": modes, "signals": signals, "plots": plots}
  expected = {"modes": expected_modes, "signals": expected_signals, "plots": expected_plots}
  (output / "input.json").write_text(json.dumps(inputs))
  (output / "expected.json").write_text(json.dumps(expected))
  receipt = {
    "source_sha256": hashlib.sha256((root / "openpilot/selfdrive/carrot/cluster/cluster_renderer.py").read_bytes()).hexdigest(),
    "modes": len(modes),
    "signals": len(signals),
    "plots": len(plots),
  }
  if args.binary:
    binary = args.binary.resolve()
    command = [str(binary), str(output / "input.json")]
    result = subprocess.run(command, capture_output=True, check=True)
    (output / "command.json").write_text(json.dumps(command))
    (output / "native.json").write_bytes(result.stdout)
    actual = json.loads(result.stdout)
    assert actual.keys() == expected.keys(), actual.keys()
    mismatches = {key: [i for i, (a, b) in enumerate(zip(actual[key], expected[key], strict=True)) if a != b] for key in expected}
    receipt.update(binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(), mismatches=mismatches)
    (output / "result.json").write_text(json.dumps(receipt, indent=2))
    print(json.dumps(receipt))
    assert not any(mismatches.values()), mismatches
  else:
    (output / "baseline.json").write_text(json.dumps(receipt, indent=2))
    print(json.dumps(receipt))


if __name__ == "__main__":
  main()
