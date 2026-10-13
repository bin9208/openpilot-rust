# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
# Run with the retained source Raylib environment:
# python cluster_replay_input_source.py ROOT INPUT OUTPUT
"""Drive unchanged source replay methods through an owned visible window."""
from __future__ import annotations

from dataclasses import replace
import json
from pathlib import Path
import sys
import time
from types import SimpleNamespace

from cluster_navigation_render_cases import serializable
from cluster_scene_compare import source as load_source


def main() -> None:
    root_arg, input_arg, output_arg = sys.argv[1:]
    root, input_path, output = (Path(value).resolve() for value in (root_arg, input_arg, output_arg))
    sys.path.insert(0, str(root / "openpilot/selfdrive/carrot/cluster"))
    import cluster_renderer as source
    import pyray as rl
    from cluster_system_monitor import SystemStats

    _, _, _, base = load_source(root)
    options = json.loads(input_path.read_text())["options"]
    state = replace(base, onroad=False, camera_view_mode=2)
    input_path.write_text(json.dumps({"options": options, "state": serializable({"state": state})["state"]}))
    output.mkdir(parents=True, exist_ok=True)
    renderer = source.ClusterUiRenderer(**options)
    renderer._system_stats.close()
    renderer._system_stats = SimpleNamespace(sample=lambda: SystemStats(), close=lambda: None)
    source.time = SimpleNamespace(monotonic=lambda: 100.0, perf_counter=lambda: 100.0, time=lambda: 1791792000.0, strftime=time.strftime, localtime=time.localtime)
    try:
        renderer.render_route_replay_frame(state, 20.0, 100.0, 0.25, False)
        print(json.dumps({"ready": True}), flush=True)
        for line in sys.stdin:
            step = json.loads(line)
            renderer.route_camera_tuning_visible = step["tuning_visible"]
            renderer.render_route_replay_frame(state, 20.0, step["duration_s"], 0.25, False)
            seek, corner, active = renderer.route_replay_control_input(20.0, step["duration_s"], 0.25)
            down = renderer.route_replay_mouse_down()
            renderer.render_route_replay_frame(state, 20.0, step["duration_s"], 0.25, False)
            path = output / f"{step['name']}.png"
            image = rl.load_image_from_screen()
            try:
                assert rl.export_image(image, str(path))
            finally:
                rl.unload_image(image)
            print(json.dumps({"name": step["name"], "input": {"seek_s": seek, "corner_lateral_offset_m": corner, "control_active": active}, "mouse_down": down, "height_m": renderer.camera_overlay_z_offset_m, "pitch_deg": renderer.camera_overlay_pitch_offset_deg, "path": str(path)}), flush=True)
    finally:
        renderer.close()


if __name__ == "__main__":
    main()
