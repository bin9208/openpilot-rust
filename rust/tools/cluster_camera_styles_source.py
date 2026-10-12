"""Render the existing UI shader constants unchanged as cluster provider regressions."""

from __future__ import annotations

import ast
import json
import os
from pathlib import Path
import sys

import pyray as rl


def shaders(path: Path) -> tuple[str, str]:
  tree = ast.parse(path.read_text())
  nodes = []
  for node in tree.body:
    if isinstance(node, ast.Assign) and any(isinstance(target, ast.Name) and target.id in ('VERSION', 'VERTEX_SHADER') for target in node.targets):
      nodes.append(node)
    if isinstance(node, ast.If) and isinstance(node.test, ast.Name) and node.test.id == 'TICI':
      nodes.extend(node.orelse)
  namespace = {}
  exec(compile(ast.Module(body=nodes, type_ignores=[]), str(path), 'exec'), namespace)
  return namespace['VERTEX_SHADER'], namespace['FRAME_FRAGMENT_SHADER']


def main() -> None:
  root, output = map(Path, sys.argv[1:3])
  width, height = map(int, sys.argv[3:])
  rl.set_config_flags(32)
  rl.init_window(width, height, 'Camera style regression')
  rl.set_target_fps(0)
  large = rl.load_shader_from_memory(*shaders(root / 'openpilot/selfdrive/ui/onroad/cameraview.py'))
  compact = rl.load_shader_from_memory(*shaders(root / 'openpilot/selfdrive/ui/mici/onroad/cameraview.py'))
  y = rl.load_texture_from_image(rl.Image(None, 16, 4, 1, rl.PixelFormat.PIXELFORMAT_UNCOMPRESSED_GRAYSCALE))
  uv = rl.load_texture_from_image(rl.Image(None, 8, 2, 1, rl.PixelFormat.PIXELFORMAT_UNCOMPRESSED_GRAY_ALPHA))
  data = bytes(range(32, 128))
  rl.update_texture(y, rl.ffi.cast('void *', rl.ffi.from_buffer(data[:64])))
  rl.update_texture(uv, rl.ffi.cast('void *', rl.ffi.from_buffer(data[64:])))
  for name, shader, engaged, driver in (
    ('large', large, None, None),
    ('compact-00', compact, 0, 0),
    ('compact-10', compact, 1, 0),
    ('compact-01', compact, 0, 1),
    ('compact-11', compact, 1, 1),
    ('large-after-compact', large, None, None),
  ):
    if engaged is not None:
      rl.set_shader_value(shader, rl.get_shader_location(shader, 'engaged'), rl.ffi.new('int[1]', [engaged]), rl.ShaderUniformDataType.SHADER_UNIFORM_INT)
      rl.set_shader_value(shader, rl.get_shader_location(shader, 'enhance_driver'), rl.ffi.new('int[1]', [driver]), rl.ShaderUniformDataType.SHADER_UNIFORM_INT)
    rl.begin_drawing()
    rl.clear_background(rl.Color(22, 33, 44, 255))
    rl.begin_shader_mode(shader)
    rl.set_shader_value_texture(shader, rl.get_shader_location(shader, 'texture1'), uv)
    rl.draw_texture_pro(y, rl.Rectangle(0, 0, 8, 4), rl.Rectangle(7.25, 5.5, width - 22.5, height - 18.25), rl.Vector2(0, 0), 0, rl.WHITE)
    rl.end_shader_mode()
    rl.take_screenshot(os.path.relpath(output / f'{name}.png'))
    rl.end_drawing()
  rl.unload_texture(y)
  rl.unload_texture(uv)
  rl.unload_shader(large)
  rl.unload_shader(compact)
  rl.close_window()
  (output / 'result.json').write_text(json.dumps({'pass': True, 'cases': 6}))


if __name__ == '__main__':
  main()
