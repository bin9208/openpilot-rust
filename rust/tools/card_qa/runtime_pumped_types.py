from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
import subprocess
from typing import Protocol, TypeAlias, TypedDict

JsonValue: TypeAlias = None | bool | int | float | str | list['JsonValue'] | dict[str, 'JsonValue']


class Publication(TypedDict, total=False):
  logMonoTime: int
  valid: bool
  carState: dict[str, JsonValue]
  carOutput: dict[str, JsonValue]
  carParams: dict[str, JsonValue]
  sendcan: list[dict[str, JsonValue]]


class Publications(TypedDict):
  carParams: list[Publication]
  carOutput: list[Publication]
  carState: list[Publication]
  sendcan: list[Publication]


class Capture(TypedDict):
  params: dict[str, JsonValue]
  warmup: Publications
  stream: Publications
  passive: bool
  command: list[str]
  pump_command: list[str]
  startup_seconds: float


class Subscriber(Protocol):
  def receive(self, non_blocking: bool = False) -> bytes | None: ...


@dataclass(frozen=True, slots=True)
class RuntimeInvocation:
  binary: Path
  numerics: Path
  binding: Path
  evidence: Path
  simulation: bool
  can_interval: float
  controls_every: int
  runtime_root: Path


@dataclass(frozen=True, slots=True)
class RuntimePeer:
  process: subprocess.Popen
  pump: subprocess.Popen
  subscribers: dict[str, Subscriber]
  output: Path
  passive: bool
  candidate: str
  observed_frames: int
