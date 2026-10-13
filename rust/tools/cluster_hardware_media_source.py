from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import sys
from types import SimpleNamespace


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument("root", type=Path)
  parser.add_argument("output", type=Path)
  args = parser.parse_args()
  os.environ["OPENPILOT_PREFIX"] = f"-owned-cluster-media-{os.getpid()}"
  sys.path.insert(0, str(args.root.resolve()))
  sys.path.insert(0, str(args.root.resolve() / "openpilot/selfdrive/carrot/cluster"))
  import cluster_renderer as source
  from cluster_models import NaviMediaFrame
  import pyray as rl
  output = args.output.resolve()
  output.mkdir(parents=True, exist_ok=True)
  renderer = source.ClusterUiRenderer(width=640, height=480, theme_mode="dark")
  renderer.open(hidden=True)
  target = rl.load_render_texture(640, 480)
  rect = rl.Rectangle(30.25, 18.75, 530.5, 270.25)

  def capture(frame):
    rl.begin_texture_mode(target)
    try:
      rl.clear_background(rl.BLACK)
      assert renderer._draw_navi_media(frame, rect)
    finally:
      rl.end_texture_mode()
    image = rl.load_image_from_texture(target.texture)
    try:
      rl.image_flip_vertical(image)
      rl.image_format(image, rl.PixelFormat.PIXELFORMAT_UNCOMPRESSED_R8G8B8A8)
      rgba = bytes(rl.ffi.buffer(image.data, 640 * 480 * 4))
      assert rl.export_image(image, str(output / "preserved-image.png"))
      return rgba
    finally:
      rl.unload_image(image)

  failures = []
  try:
    rgba = NaviMediaFrame(key="owned-hardware", sequence=1, present=True, mime="image/rgba", width=8, height=4, data=bytes([180, 60, 20, 255] * 32))
    original = capture(rgba)
    initial = None
    for index in range(12):
      fd = os.memfd_create("owned-cluster-media")
      try:
        buffer = SimpleNamespace(fd=fd, stride=8, uv_offset=32, token=(index // 4, index), mark_egl_import_failed=failures.append)
        frame = NaviMediaFrame(key="owned-hardware", sequence=index+2, present=True, mime="video/nv12-dmabuf", width=8, height=4, hardware_buffer=buffer)
        assert capture(frame) == original
        assert failures[-1] == "EGL DMA-BUF import is unavailable"
      finally:
        os.close(fd)
      if initial is None:
        initial = len(os.listdir("/proc/self/fd"))
      assert len(os.listdir("/proc/self/fd")) == initial
    (output / "result.json").write_text(json.dumps(dict(frames=12, generations=3, failures=failures, fd_delta=0, fd_baseline_after_first_import=True, fallback_pixels_equal=True), indent=2))
  finally:
    rl.unload_render_texture(target)
    renderer.close()


if __name__ == "__main__":
  main()
