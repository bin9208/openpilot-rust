from __future__ import annotations

from dataclasses import dataclass

from tinygrad.uop.ops import UOp

from .schema import Bindings


@dataclass(frozen=True, slots=True)
class InitializedBuffer:
    buffer: UOp
    data: bytes


@dataclass(frozen=True, slots=True)
class OfflineModel:
    bindings: Bindings
    initialized: tuple[InitializedBuffer, ...]
