from __future__ import annotations

from dataclasses import dataclass
from enum import StrEnum
from typing import TypeAlias, TypedDict

Json: TypeAlias = str | bool | int | float | None | list['Json'] | dict[str, 'Json']


class Output(TypedDict, total=False):
  result: list[bool | str]
  exception: str
  message: str


class Effect(TypedDict, total=False):
  show: bool
  detail: str | None
  lock_held: bool
  notify: str
  cancel_lock_held: bool


class StepResult(TypedDict):
  output: Output
  ready_calls: int
  state: Json
  cache_head: str


class Result(TypedDict):
  steps: list[StepResult]
  effects: list[Effect]


class Mode(StrEnum):
  NORMAL = 'normal'
  FETCH_ERROR = 'fetch-error'
  BRANCH_CHANGE = 'branch-change'
  HEAD_CHANGE = 'head-change'
  BRANCH_ERROR = 'branch-error'
  HEAD_ERROR = 'head-error'
  CONFIG_ERROR = 'config-error'
  TARGET_MOVE = 'target-move'
  RESET_BUSY = 'reset-busy'
  CONFIG_CANCEL = 'config-cancel'


class Step(TypedDict):
  now: float
  ready: list[bool]
  restore: bool
  warm: bool


class Config(TypedDict):
  repository: str
  launcher: str
  state: str
  lock: str
  proc_root: str
  phase: str
  base: str
  steps: list[Step]
  busy: bool
  index_lock: bool
  cancel: bool
  cancel_ready: str
  cancel_release: str


@dataclass(frozen=True, slots=True)
class Case:
  name: str
  mode: Mode = Mode.NORMAL
  ready: tuple[bool, ...] = ()
  times: tuple[float, ...] = (1000.,)
  busy: bool = False
  index_lock: bool = False
  pulling: bool = False
  up_to_date: bool = False
  warm: bool = True


CASES = (
  Case('initial-not-ready', ready=(False,), warm=False),
  Case('verified-fetch-required', mode=Mode.FETCH_ERROR),
  Case('up-to-date', up_to_date=True),
  Case('not-ready-after-lock', ready=(True, False)),
  Case('not-ready-after-branch', ready=(True, True, False)),
  Case('not-ready-after-head', ready=(True, True, True, False)),
  Case('not-ready-after-config', ready=(True, True, True, True, False)),
  Case('branch-changed', mode=Mode.BRANCH_CHANGE),
  Case('branch-read-failed', mode=Mode.BRANCH_ERROR),
  Case('head-changed', mode=Mode.HEAD_CHANGE),
  Case('head-read-failed', mode=Mode.HEAD_ERROR),
  Case('configuration-failed', mode=Mode.CONFIG_ERROR),
  Case('fresh-index-busy-idle', index_lock=True),
  Case('fresh-index-busy-pulling', index_lock=True, pulling=True),
  Case('cooperative-lock-busy-idle', busy=True),
  Case('cooperative-lock-busy-pulling', busy=True, pulling=True),
  Case('reset-index-busy-waits', mode=Mode.RESET_BUSY),
  Case('updated-dirty-owned-repo'),
  Case('prepare-pins-newly-fetched-target', mode=Mode.TARGET_MOVE),
  Case('cooldown-just-before-and-at-300', times=(1000., 1299.999, 1300.)),
  Case('busy-pull-still-starts-cooldown', mode=Mode.RESET_BUSY, times=(1000., 1299.999, 1300.)),
  Case('failed-config-does-not-start-cooldown', mode=Mode.CONFIG_ERROR, times=(1000., 1001.)),
  Case('cancel-config-keeps-lock-until-completion', mode=Mode.CONFIG_CANCEL),
)
