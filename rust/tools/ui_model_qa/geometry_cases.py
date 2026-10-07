"""Deterministic projection inputs, independent of native implementation."""

from dataclasses import dataclass, field, replace
from typing import Literal
import numpy as np

Kind = Literal['sample', 'path', 'lanes', 'blindspot', 'quads', 'big_ribbon', 'small_ribbon', 'point']


@dataclass(frozen=True, slots=True)
class Case:
  name: str
  kind: Kind
  line: list[list[float]]
  transform: list[list[float]]
  clip: list[float] = field(default_factory=lambda: [-500.0, -500.0, 3160.0, 2080.0])
  distances: list[float] = field(default_factory=lambda: [0.0, 2.0, 10.0, 20.0, 40.0, 100.0])
  width: float = 0.16
  shift: float = 0.0
  z_start: float = 1.22
  z_end: float = 1.22
  end: int = 32
  distance: float = 40.0
  invert: bool = False


def cases() -> list[Case]:
  rng = np.random.default_rng(148)
  transform = [[1080.0, -900.0, 0.0], [540.0, 0.0, -900.0], [1.0, 0.0, 0.0]]
  result = []
  kinds: list[Kind] = ['sample', 'path', 'lanes', 'blindspot', 'quads', 'big_ribbon', 'small_ribbon', 'point']
  for index in range(40):
    x = np.linspace(-1.0, 130.0, 33)
    y = np.sin(x / 30.0) * (index % 7) + rng.normal(0.0, 0.05, len(x))
    z = np.sin(x / 15.0) * (index % 4) * 0.25
    if index % 5 == 0:
      x[8:10] = x[7]
    if index % 5 == 1:
      x[5] = x[4] - 1.0
    line = np.column_stack((x, y, z)).tolist()
    if index % 10 == 2:
      line = []
    if index % 10 == 3:
      line = line[:1]
    matrix = np.asarray(transform) + rng.normal(0.0, 0.01, (3, 3))
    if index % 10 in (4, 5, 6):
      matrix[2] = [0.0, 0.0, [0.0, 1e-6, -1e-6][index % 10 - 4]]
    for kind in kinds:
      if kind == "sample" and not line:
        continue
      result.append(
        Case(
          f'{kind}-{index}',
          kind,
          line,
          matrix.tolist(),
          width=0.025 * (1 + index % 4),
          shift=(-1.7 if index % 2 else 1.7),
          end=index % 33,
          distance=float(index * 3),
          z_start=float(index % 7 - 3),
          z_end=1.22,
          invert=index % 2 == 0,
        )
      )
  boundary = Case(
    'boundary',
    'big_ribbon',
    [[0.0, 0.0, 0.0], [1.0, 1.0, 1.0], [2.0, -1.0, -1.0]],
    [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
    clip=[-1.0, -1.0, 2.0, 2.0],
    end=2,
    distance=2.0,
    z_start=0.0,
  )
  result.extend([boundary, replace(boundary, name='boundary-small', kind='small_ribbon')])
  for kind in ['path', 'blindspot', 'quads', 'big_ribbon', 'small_ribbon', 'point']:
    result.append(replace(boundary, name=f'float32-overflow-{kind}', kind=kind, transform=[[1e39, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1e39]]))
  return result
