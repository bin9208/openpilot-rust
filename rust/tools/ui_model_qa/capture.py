"""Capture actual source drawing arguments while executing the original GL calls."""

import numpy as np


def f32(value):
  return float(np.float32(value))


def xy(value):
  if hasattr(value, 'x'):
    return [float(value.x), float(value.y)]
  return [f32(value[0]), f32(value[1])]


def rectangle(value):
  return [float(value.x), float(value.y), float(value.width), float(value.height)]


def color(value):
  return int.from_bytes(bytes([value.r, value.g, value.b, value.a] if hasattr(value, 'r') else value), 'little')


class Capture:
  """Mutable per-frame draw-command accumulator; calls always reach real GL."""

  def __init__(self, rl, polygon, text_draw):
    self.commands = []
    self.rl, self.polygon, self.text_draw = rl, polygon, text_draw
    self.original = {
      name: getattr(rl, name) for name in ('draw_line_ex', 'draw_circle', 'draw_rectangle_rounded', 'draw_rectangle_rounded_lines_ex', 'measure_text')
    }
    rl.draw_line_ex = self.line
    rl.draw_circle = self.circle
    rl.draw_rectangle_rounded = self.rounded
    rl.draw_rectangle_rounded_lines_ex = self.rounded_outline
    rl.measure_text = self.measure
    self.raw_text = text_draw._RAW_DRAW_TEXT_EX
    text_draw._RAW_DRAW_TEXT_EX = self.text

  def solid(self, points, tint):
    if len(points) >= 3:
      self.commands.append({'kind': 'strip', 'points': self.polygon.triangulate(np.asarray(points, dtype=np.float32)), 'color': color(tint)})
    self.polygon.draw_polygon_solid(points, tint)

  def shaded(self, rect, points, tint=None, gradient=None):
    if len(points) >= 3:
      if gradient is None:
        paint = {'color': color(tint)}
      else:
        paint = {
          'start': [f32(rect.x + gradient.start[0] * rect.width), f32(rect.y + gradient.start[1] * rect.height)],
          'end': [f32(rect.x + gradient.end[0] * rect.width), f32(rect.y + gradient.end[1] * rect.height)],
          'colors': [color(c) for c in gradient.colors],
          'stops': [f32(np.clip(s, 0.0, 1.0)) for s in gradient.stops],
        }
      self.commands.append({'kind': 'shader', 'points': self.polygon.triangulate(np.asarray(points, dtype=np.float32)), 'paint': paint})
    self.polygon.draw_polygon(rect, points, tint, gradient)

  def line(self, start, end, thickness, tint):
    self.commands.append({'kind': 'line', 'start': xy(start), 'end': xy(end), 'thickness': f32(thickness), 'color': color(tint)})
    return self.original['draw_line_ex'](start, end, thickness, tint)

  def circle(self, x, y, radius, tint):
    self.commands.append({'kind': 'circle', 'position': [f32(x), f32(y)], 'radius': f32(radius), 'color': color(tint)})
    return self.original['draw_circle'](x, y, radius, tint)

  def rounded(self, rect, roundness, segments, tint):
    self.commands.append({'kind': 'rounded', 'rect': rectangle(rect), 'roundness': f32(roundness), 'segments': segments, 'color': color(tint), 'border': False})
    return self.original['draw_rectangle_rounded'](rect, roundness, segments, tint)

  def rounded_outline(self, rect, roundness, segments, thickness, tint):
    self.commands.append(
      {'kind': 'rounded_outline', 'rect': rectangle(rect), 'roundness': f32(roundness), 'segments': segments, 'thickness': f32(thickness), 'color': color(tint)}
    )
    return self.original['draw_rectangle_rounded_lines_ex'](rect, roundness, segments, thickness, tint)

  def measure(self, text, size):
    value = self.original['measure_text'](text, size)
    self.commands.append({'kind': 'measure_default', 'text': text, 'size': size, 'value': value})
    return value

  def text(self, font, text, position, size, spacing, tint):
    self.commands.append({'kind': 'text', 'text': text.decode(), 'position': xy(position), 'size': f32(size), 'spacing': f32(spacing), 'color': color(tint)})
    return self.raw_text(font, text, position, size, spacing, tint)
