from __future__ import annotations
from collections.abc import Callable
from enum import IntEnum
from typing import TYPE_CHECKING, NotRequired, Protocol, TypedDict

if TYPE_CHECKING:
  from msgq.visionipc import VisionBuf, VisionIpcClient, VisionStreamType
  from openpilot.system.ui.lib.application import GuiApplication
  from openpilot.system.ui.widgets import Widget


class Rect(TypedDict):
  x: float
  y: float
  width: float
  height: float


class Item(Rect):
  visible: bool


class Config(TypedDict):
  big: bool
  large_viewport: bool
  pc: bool
  scale: float


class RoadStep(TypedDict):
  frame: int
  messages: list[list[int]]
  started: bool
  status: NotRequired[int]
  started_time: NotRequired[float]
  started_frame: NotRequired[int]
  params: NotRequired[dict[str, str]]
  memory: NotRequired[dict[str, str]]
  suppress: NotRequired[bool]
  lat_active: NotRequired[bool]


class RoadOptions(TypedDict):
  steps: list[RoadStep]


class RootStep(TypedDict):
  frame: int
  page: NotRequired[str]
  timeout: NotRequired[bool]


class RootOptions(TypedDict):
  steps: list[RootStep]


class Scene(TypedDict):
  config: Config
  road: RoadOptions
  root: RootOptions
  params: dict[str, str]
  memory: NotRequired[dict[str, str]]


class CalibrationStep(TypedDict):
  reset: bool
  rect: Rect
  stream: int
  speed: float
  messages: list[list[int]]


class CalibrationResult(TypedDict):
  camera: list[list[float]]
  model: list[list[float]]
  position: str


class CameraSnapshot(TypedDict):
  frame: int | None
  stream: int
  streams: list[int]
  mode: int
  position: str


class RootSnapshot(TypedDict):
  mode: str | None
  settings_panel: str | None
  sidebar: bool
  scroll: float | None
  in_plot_mode: bool
  recording: bool
  stack: int


class Trace(TypedDict):
  prime: int
  camera: NotRequired[CameraSnapshot]
  root: NotRequired[RootSnapshot]


class Parameters(Protocol):
  def get(self, key: str) -> str | bytes | None: ...
  def get_bool(self, key: str) -> bool: ...


class Message(Protocol):
  def as_builder(self) -> Message: ...


class Messages(Protocol):
  recv_frame: dict[str, int]
  recv_time: dict[str, float]
  alive: dict[str, bool]
  valid: dict[str, bool]
  updated: dict[str, bool]
  seen: dict[str, bool]

  def __setitem__(self, key: str, value: Message | list[Message] | bytes) -> None: ...


class State(Protocol):
  params: Parameters
  params_memory: Parameters
  sm: Messages
  is_metric: bool
  always_on_dm: bool
  recording_audio: bool
  started_frame: int
  started_time: float
  panda_type: int
  status: int
  lat_active: bool
  started: bool
  ignition: bool
  engaged: bool

  def update_params(self) -> None: ...


class Road(Protocol):
  _name: str
  frame: VisionBuf | None
  client: VisionIpcClient
  stream_type: VisionStreamType
  available_streams: list[VisionStreamType]

  def set_cluster_hud_connected(self, connected: bool, show_camera: bool) -> None: ...
  def _road_view_mode(self) -> int: ...


class Camera(Protocol):
  def before(self, index: int) -> None: ...
  def snapshot(self) -> CameraSnapshot: ...


class Layout(Protocol):
  _current_panel: IntEnum


class Visible(Protocol):
  is_visible: bool


class ScrollPanel(Protocol):
  def get_offset(self) -> float: ...


class Scroller(Protocol):
  scroll_panel: ScrollPanel


class Root(Protocol):
  owned_road: Road
  owned_timeout: list[Callable[[], None]]
  _current_mode: IntEnum
  _layouts: dict[int, Layout]
  _sidebar: Visible
  _scroller: Scroller
  _in_plot_mode: bool
  _home_layout: Widget
  _settings_layout: Widget

  def _set_mode_for_state(self) -> None: ...
  def open_settings(self, panel: IntEnum) -> None: ...
  def _scroll_to(self, widget: Widget) -> None: ...


class Device(Protocol):
  add_interactive_timeout_callback: Callable[[Callable[[], None]], None]


class Context(Protocol):
  ui: State
  gui: GuiApplication
  params: type[Parameters]
  device: Device


class Pages(TypedDict):
  settings: list[str]
  overlays: list[str]
  dialogs: list[str]
  frames: int


class Command(TypedDict, total=False):
  params: dict[str, str | None]
  memory: dict[str, str | None]
  page: str
  dialog: str
  prime: int
  timeout: int
  alert: str
  capture: str
  asleep: bool
  bookmark: bool
  fail: bool
  close: bool
