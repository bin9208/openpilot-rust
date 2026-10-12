# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
from __future__ import annotations

import ctypes
import json
from pathlib import Path
import sys
import time
from typing import Protocol

import pyray as rl
from openpilot.selfdrive.carrot.cluster.cluster_gles_readback import GlesDirectReadback


class Texture(Protocol):
  width: int
  height: int


class Target(Protocol):
  id: int
  texture: Texture


class ReadbackTimeout(TimeoutError):
  def __init__(self, deadline: float) -> None:
    self.deadline = deadline
    super().__init__(f"source GPU readback exceeded deadline {deadline}")


def main() -> None:
  output = Path(sys.argv[1])
  output.mkdir(parents=True, exist_ok=True)
  rl.set_trace_log_level(4)
  rl.set_config_flags(128)
  rl.init_window(32, 24, "Owned source GLES readback")
  targets = [rl.load_render_texture(32, 24), rl.load_render_texture(16, 12), rl.load_render_texture(8, 8)]
  readback = GlesDirectReadback()
  gl = readback._gles
  saved = ctypes.c_uint()
  gl.glGenBuffers(1, ctypes.byref(saved))
  gl.glBindBuffer(0x88EB, saved.value)
  gl.glBufferData(0x88EB, 8, ctypes.c_void_p(), 0x88E1)
  records: list[dict[str, str | bool]] = []

  def bindings() -> list[int]:
    values = []
    for parameter in (0x8CA6, 0x88ED, 0x0D05):
      value = ctypes.c_int()
      gl.glGetIntegerv(parameter, ctypes.byref(value))
      values.append(value.value)
    return values

  def restore_fixture_bindings() -> None:
    gl.glBindFramebuffer(0x8D40, targets[2].id)
    gl.glBindBuffer(0x88EB, saved.value)
    gl.glPixelStorei(0x0D05, 8)

  def paint(index: int, frame: int) -> Target:
    target = targets[index]
    rl.begin_texture_mode(target)
    rl.clear_background(rl.BLANK)
    for y in range(target.texture.height):
      for x in range(target.texture.width):
        rl.draw_rectangle(x, y, 1, 1, rl.Color((x * 7 + frame * 13) % 256, y * 11 % 256, (x + y) * 4 % 256, 255))
    rl.end_texture_mode()
    return target

  def direct(target: Target, name: str) -> None:
    restore_fixture_bindings()
    before = bindings()
    data = bytearray([0xA5]) * (target.texture.width * target.texture.height * 4 + 7)
    view = (ctypes.c_ubyte * len(data)).from_buffer(data)
    readback.read_rgba(target.id, target.texture.width, target.texture.height, ctypes.addressof(view), len(data))
    path = output / f"{name}.rgba"
    path.write_bytes(data)
    records.append({"name": name, "path": path.name, "restored": before == bindings()})

  def enqueue(target: Target, name: str) -> None:
    restore_fixture_bindings()
    before = bindings()
    accepted = readback.enqueue_rgba(target.id, target.texture.width, target.texture.height)
    records.append({"name": name, "accepted": accepted, "can_enqueue": readback.async_can_enqueue(), "restored": before == bindings()})

  def copy(size: int, name: str) -> None:
    deadline = time.monotonic() + 0.9
    while not readback.async_ready():
      if time.monotonic() >= deadline:
        raise ReadbackTimeout(deadline)
    restore_fixture_bindings()
    before = bindings()
    data = bytearray([0xA5]) * (size + 7)
    view = (ctypes.c_ubyte * len(data)).from_buffer(data)
    copied = readback.copy_ready(ctypes.addressof(view), len(data))
    path = output / f"{name}.rgba"
    path.write_bytes(data)
    records.append({"name": name, "copied": copied, "path": path.name, "can_enqueue": readback.async_can_enqueue(), "restored": before == bindings()})

  try:
    records.append({"name": "initial", "supported": readback.async_supported, "can_enqueue": readback.async_can_enqueue(), "ready": readback.async_ready()})
    direct(paint(0, 0), "direct-0")
    for frame in range(1, 5):
      enqueue(paint(0, frame), f"enqueue-{frame}")
    for frame in range(1, 4):
      copy(32 * 24 * 4, f"copy-{frame}")
    enqueue(paint(0, 5), "enqueue-discarded")
    enqueue(paint(1, 6), "enqueue-resized")
    copy(16 * 12 * 4, "copy-resized")
    readback.disable_async()
    records.append({"name": "disabled", "supported": readback.async_supported, "can_enqueue": readback.async_can_enqueue(), "ready": readback.async_ready()})
    direct(paint(1, 7), "direct-disabled")
    readback.close()
    readback.close()
    records.append({"name": "closed", "ready": readback.async_ready()})
    (output / "result.json").write_text(json.dumps(records, indent=2) + "\n")
  finally:
    readback.close()
    gl.glBindBuffer(0x88EB, 0)
    gl.glBindFramebuffer(0x8D40, 0)
    gl.glDeleteBuffers(1, ctypes.byref(saved))
    for target in targets:
      rl.unload_render_texture(target)
    rl.close_window()


if __name__ == "__main__":
  main()
