#!/usr/bin/env python3
import argparse
import hashlib
import json
import sys
from pathlib import Path


def layout(width, height, stride, y_scanlines, uv_scanlines, gap, tail):
  uv_offset = stride * y_scanlines + gap
  return {
    "width": width,
    "height": height,
    "stride": stride,
    "y_scanlines": y_scanlines,
    "uv_scanlines": uv_scanlines,
    "uv_offset": uv_offset,
    "byte_count": uv_offset + stride * uv_scanlines + tail,
  }


def cases():
  groups = []
  for width, height in [(96, 64), (1920, 480)]:
    for theme in ["light", "dark"]:
      layouts = [
        ("direct", layout(height, width, height, width, (width + 1) // 2, 0, height * 3)),
        ("full-stride", layout(height, width, height + 4, width + 2, (width + 1) // 2 + 1, 3, 5)),
        ("row-copy", layout(height - 1, width - 1, height + 5, width + 1, (width + 1) // 2 + 1, 7, 13)),
        (
          "venus",
          {
            "width": height,
            "height": width,
            "stride": 128 if width == 96 else 512,
            "y_scanlines": 96 if width == 96 else 1920,
            "uv_scanlines": 48 if width == 96 else 960,
            "uv_offset": 12_288 if width == 96 else 983_040,
            "byte_count": 303_104 if width == 96 else 1_757_184,
          },
        ),
      ]
      rows = []
      for name, value in layouts:
        for flip_x in [False, True]:
          prefix = f"{width}x{height}-{theme}-{name}-{int(flip_x)}"
          rows.extend(
            [
              {"name": f"{prefix}-cold", "layout": value, "flip_x": flip_x, "phase": 0, "seed": None, "reuse": False},
              {"name": f"{prefix}-warm", "layout": value, "flip_x": flip_x, "phase": 3, "seed": None, "reuse": True},
              {"name": f"{prefix}-seed", "layout": value, "flip_x": flip_x, "phase": 7, "seed": 77, "reuse": False},
            ]
          )
      groups.append({"width": width, "height": height, "theme": theme, "cases": rows})
  return groups


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument("root", type=Path)
  parser.add_argument("output", type=Path)
  parser.add_argument("--prepare-only", action="store_true")
  args = parser.parse_args()
  root, output = args.root.resolve(), args.output.resolve()
  output.mkdir(parents=True, exist_ok=True)
  groups = cases()
  (output / "input.json").write_text(json.dumps({"groups": groups}, indent=2))
  if args.prepare_only:
    return
  sys.path.insert(0, str(root))
  sys.path.insert(0, str(root / "openpilot/selfdrive/carrot/cluster"))
  import cluster_renderer as source
  from cluster_scene_compare import source as scene_source
  import pyray as rl

  _, _, _, state = scene_source(root)

  records = []
  for group in groups:
    width, height = group["width"], group["height"]
    renderer = source.ClusterUiRenderer(width=width, height=height, theme_mode=group["theme"])
    renderer.open(hidden=True)
    previous = None
    try:
      for case in group["cases"]:
        phase = case["phase"]

        def draw(_state, renderer=renderer, phase=phase):
          width, height = renderer.width, renderer.height
          rl.clear_background(source.rl_color(renderer._current_theme().bg))
          for row in range(4):
            for column in range(8):
              color = ((column * 31 + phase) % 256, (row * 61 + phase * 3) % 256, ((column + row) * 21 + phase * 5) % 256, 255)
              rl.draw_rectangle_rec(rl.Rectangle(column * width / 8, row * height / 4, width / 8, height / 4), source.rl_color(color))
          rl.draw_rectangle_rec(rl.Rectangle(3.25, 7.5, 11.25, 9.5), source.rl_color((180, 30, 210, 127)))

        renderer.render = draw
        value = case["layout"]
        buffer = previous if case["reuse"] else None if case["seed"] is None else bytearray([case["seed"]]) * value["byte_count"]
        with renderer.render_to_nv12_buffer(
          state,
          value["width"],
          value["height"],
          value["stride"],
          value["y_scanlines"],
          value["uv_scanlines"],
          value["uv_offset"],
          value["byte_count"],
          buffer=buffer,
          flip_x=case["flip_x"],
        ) as packed:
          previous = bytearray(packed)
        path = output / f"{case['name']}.nv12"
        path.write_bytes(previous)
        records.append({"name": case["name"], "bytes": len(previous), "path": str(path), "sha256": hashlib.sha256(previous).hexdigest(), "layout": value})
    finally:
      renderer.close()
  (output / "source-result.json").write_text(
    json.dumps(
      {
        "cases": len(records),
        "records": records,
        "source_sha256": hashlib.sha256((root / "openpilot/selfdrive/carrot/cluster/cluster_renderer.py").read_bytes()).hexdigest(),
      },
      indent=2,
    )
  )
  print(json.dumps({"cases": len(records)}))


if __name__ == "__main__":
  main()
