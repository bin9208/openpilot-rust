from __future__ import annotations

from typing import NotRequired, TypedDict

from radarcan_exact import Json


class Frame(TypedDict):
  address: int
  data: list[int]
  bus: int


class Packet(TypedDict):
  mono_time: int
  frames: list[Frame]


class Metadata(TypedDict):
  firstCanMonoTime: int
  lastCanMonoTime: int
  canPacketCount: int
  receiveMonoTime: int


class Action(TypedDict):
  time: float
  packets: list[Packet]
  v_ego: float
  a_ego: float
  metadata: NotRequired[Metadata]
  state_count: NotRequired[int]
  can_delay_ms: NotRequired[float]


class Case(TypedDict, total=False):
  name: str
  candidate: str
  brand: str
  delay: float
  period: float
  unavailable: bool
  flags: int
  ext_flags: int
  safety_count: int
  params: dict[str, str]
  constructor_ns: int
  dbc_root: str
  actions: list[Action]
  order: str
  flip: bool
  expected_publications: int
  expected_error_publications: int
  prequeue: bool
  params_after_first: dict[str, str]


class Publication(TypedDict):
  received_ns: int
  event: Json
  raw: str
