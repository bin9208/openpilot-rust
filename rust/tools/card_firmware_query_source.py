# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# Synthetic ECU transport for unchanged firmware orchestration; never opens physical CAN.
from dataclasses import dataclass, field
from typing import TypedDict, Literal
import types
from card_isotp_cases import Frame, segmented
from card_firmware_source import setup


class Reply(TypedDict):
  target: tuple[int, int | None]
  bus: int
  offset: int
  request: list[int]
  response: list[int]


class Input(TypedDict):
  replies: list[Reply]
  clock_step: float


class Case(TypedDict, total=False):
  op: Literal['firmware', 'presence', 'brand_matches', 'ordered']
  brand: str | None
  pandas: int
  timeout: float
  present: list[tuple[int, int | None, int]]
  vin: str
  io: Input


@dataclass(slots=True)  # noqa: MUTABLE_OK
class Wire:
  """Mutable synthetic ECU receive queue, request reassembly and transport receipt."""
  input: Input
  pending: list[Frame] = field(default_factory=list)
  assembly: dict[tuple[int, int], tuple[tuple[int, int | None], int, list[int]]] = field(default_factory=dict)
  sent: list[Frame] = field(default_factory=list)
  delays: list[float] = field(default_factory=list)
  receives: list[bool] = field(default_factory=list)
  obd: list[bool] = field(default_factory=list)
  now: float = 0.
  clock_reads: int = 0

  def receive(self, wait_for_one: bool = False):
    from opendbc.car.can_definitions import CanData
    self.receives.append(wait_for_one)
    if wait_for_one:
      self.now += self.input['clock_step']
    packet, self.pending = self.pending, []
    return [[CanData(frame['address'], bytes(frame['data']), frame['bus']) for frame in packet]] if packet else []

  def clock(self) -> float:
    self.clock_reads += 1
    return self.now

  def send(self, frames) -> None:
    for frame in frames:
      request = Frame(address=frame.address, data=list(frame.dat), bus=frame.src)
      self.sent.append(request)
      self.respond(request)

  def respond_payload(self, target: tuple[int, int | None], bus: int, payload: list[int]) -> None:
    from opendbc.car.uds import get_rx_addr_for_tx_addr
    emitted = []
    for reply in self.input['replies']:
      if reply['target'] != target or reply['bus'] != bus or reply['request'] != payload:
        continue
      for frame in segmented(reply['response'], target[1], address=get_rx_addr_for_tx_addr(target[0], reply['offset']), bus=bus):
        if frame not in emitted:
          emitted.append(frame)
          self.pending.append(frame)

  def respond(self, frame: Frame) -> None:
    from opendbc.car.uds import get_rx_addr_for_tx_addr
    key = frame['address'], frame['bus']
    if key in self.assembly:
      target, length, payload = self.assembly[key]
      offset = int(target[1] is not None)
      if len(frame['data']) > offset and frame['data'][offset] >> 4 == 2:
        payload.extend(frame['data'][offset + 1:])
        if len(payload) >= length:
          del self.assembly[key]
          self.respond_payload(target, frame['bus'], payload[:length])
        return
    for reply in self.input['replies']:
      target = reply['target']
      if target[0] != frame['address'] or reply['bus'] != frame['bus']:
        continue
      offset = int(target[1] is not None)
      if target[1] is not None and frame['data'][0] != target[1]:
        continue
      if len(frame['data']) <= offset:
        continue
      header = frame['data'][offset]
      match header >> 4:
        case 0:
          length = header & 15
          payload = frame['data'][offset + 1:offset + 1 + length]
          if payload == reply['request']:
            self.respond_payload(target, frame['bus'], payload)
            return
        case 1:
          length = (header & 15) * 256 + frame['data'][offset + 1]
          payload = frame['data'][offset + 2:]
          if len(reply['request']) == length and reply['request'][:len(payload)] == payload:
            self.assembly[key] = target, length, payload
            data = ([target[1]] if target[1] is not None else []) + [0x30, 0, 0]
            data += [0] * (8 - len(data))
            self.pending.append(Frame(address=get_rx_addr_for_tx_addr(target[0], reply['offset']), data=data, bus=reply['bus']))
            return
        case _:
          continue

  def snapshot(self):
    return dict(sent=self.sent, delays=self.delays, receives=self.receives, obd=self.obd, now=self.now, clock_reads=self.clock_reads)


def trace(case: Case):
  from opendbc.car import fw_versions, uds, isotp_parallel_query, ecu_addrs
  wire = Wire(case.get('io', Input(replies=[], clock_step=0.)))
  timer = types.SimpleNamespace(monotonic=wire.clock, sleep=wire.delays.append)
  uds.time = timer
  isotp_parallel_query.time = timer
  ecu_addrs.time = timer
  match case['op']:
    case 'firmware':
      result = fw_versions.get_fw_versions(wire.receive, wire.send, wire.obd.append, case['brand'], timeout=case['timeout'], num_pandas=case['pandas'])
    case 'presence':
      result = [list(item) for item in fw_versions.get_present_ecus(wire.receive, wire.send, wire.obd.append, case['pandas'])]
    case 'brand_matches':
      matches = fw_versions.get_brand_ecu_matches(set(case['present']))
      result = [[brand, values.count(True), len(values)] for brand, values in matches.items()]
    case 'ordered':
      result = fw_versions.get_fw_versions_ordered(wire.receive, wire.send, wire.obd.append, case['vin'], set(case['present']), case['timeout'], case['pandas'])
    case unreachable:
      from typing import assert_never
      assert_never(unreachable)
  if case['op'] in ('firmware', 'ordered'):
    result = [dict(ecu=str(version.ecu), fw_version=list(version.fwVersion), address=version.address, response_address=version.responseAddress,
                   request=[list(item) for item in version.request], brand=version.brand, bus=version.bus, logging=version.logging,
                   obd_multiplexing=version.obdMultiplexing, sub_address=version.subAddress) for version in result]
  return dict(result=result, io=wire.snapshot())
