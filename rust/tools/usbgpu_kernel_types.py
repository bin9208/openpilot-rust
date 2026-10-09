"""Recover declared float views at their original kernel argument word locations."""

from __future__ import annotations

from dataclasses import dataclass
from enum import StrEnum
from typing import assert_never

from import_usbgpu_hcq import Exporter
from usbgpu_hcq_data import ArtifactError, Record, state
from usbgpu_hcq_dispatch import dtype, sources


@dataclass(frozen=True, slots=True)
class Location:
  buffer: int
  offset: int


@dataclass(frozen=True, slots=True)
class TypedWord:
  target: Location
  source: Location
  format: str
  bytes: int


class ViewKind(StrEnum):
  BITCAST = 'BITCAST'
  SHRINK = 'SHRINK'
  BUFFER = 'BUFFER'
  PARAM = 'PARAM'


def kind(exporter: Exporter, node: Record) -> ViewKind:
  operation = exporter.artifact.op(node)
  try:
    return ViewKind(operation)
  except ValueError as error:
    raise ArtifactError(f'unsupported typed kernel view: {operation}') from error


def scalar(exporter: Exporter, node: Record) -> str:
  """Read the declared scalar without inferring it from the storage allocation."""
  match kind(exporter, node):
    case ViewKind.BITCAST:
      return dtype(node.arguments[2])[3]
    case ViewKind.SHRINK:
      return scalar(exporter, sources(node)[0])
    case ViewKind.BUFFER | ViewKind.PARAM:
      return dtype(state(node.arguments[2])['dtype'])[3]
    case unreachable:
      assert_never(unreachable)


def extent(exporter: Exporter, node: Record) -> int:
  """Keep the original SHRINK extent, in bytes, through an outer BITCAST."""
  match kind(exporter, node):
    case ViewKind.BITCAST:
      return extent(exporter, sources(node)[0])
    case ViewKind.SHRINK:
      base, _offset, count = sources(node)
      return exporter.constant(count) * exporter.width(base)
    case ViewKind.BUFFER | ViewKind.PARAM:
      return state(node.arguments[2])['size'] * exporter.width(node)
    case unreachable:
      assert_never(unreachable)


def typed_words(exporter: Exporter) -> list[TypedWord]:
  """Resolve direct GETADDR patches; aliases remain distinct by target word."""
  result = []
  for node in exporter.artifact.records:
    if not node.global_name.endswith('.UOp') or exporter.artifact.op(node) != 'STORE':
      continue
    target, value = sources(node)
    if exporter.artifact.op(target) != 'INDEX':
      continue
    base, offsets = sources(target)
    if exporter.artifact.op(offsets) != 'STACK' or exporter.artifact.op(value) != 'STACK':
      continue
    for offset, word in zip(sources(offsets), sources(value), strict=True):
      if exporter.artifact.op(word) != 'GETADDR' or word.arguments[2] != ('AMD',):
        continue
      source = sources(word)[0]
      format_code = scalar(exporter, source)
      if format_code not in ('f', 'e'):
        continue
      target_view, source_view = exporter.view(base), exporter.view(source)
      result.append(
        TypedWord(
          Location(target_view['buffer'], target_view['offset'] + exporter.constant(offset) * exporter.width(word)),
          Location(source_view['buffer'], source_view['offset']),
          format_code,
          extent(exporter, source),
        )
      )
  return result
