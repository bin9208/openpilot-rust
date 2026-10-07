# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# Imported by check_card_diagnostic.py using the existing source-oracle environment.
from collections import deque
from typing import TypeAlias, TypedDict
import types
from can_source import load
from card_diagnostic_cases import Case
from card_isotp_cases import Frame

Json: TypeAlias = str | int | float | bool | None | list['Json'] | dict[str, 'Json']


class IoResult(TypedDict):
  sent: list[Frame]
  delays: list[float]
  receives: list[bool]
  now: float


class Result(TypedDict):
  result: Json
  io: IoResult


def trace(case: Case) -> Result:
  load()
  from opendbc.car import uds, ecu_addrs, disable_ecu, vin, isotp_parallel_query
  from opendbc.car.can_definitions import CanData
  input_data = case.get('io', dict(batches=[], clock_step=0.))
  batches = deque(input_data['batches'])
  output = IoResult(sent=[], delays=[], receives=[], now=0.)
  def clock() -> float:
    output['now'] += input_data['clock_step']
    return output['now']
  def receive(wait_for_one: bool = False):
    output['receives'].append(wait_for_one)
    packets = batches.popleft() if batches else []
    return [[CanData(frame['address'], bytes(frame['data']), frame['bus']) for frame in packet] for packet in packets]
  def send(frames) -> None:
    output['sent'].extend(Frame(address=frame.address, data=list(frame.dat), bus=frame.src) for frame in frames)
  timer = types.SimpleNamespace(monotonic=clock, sleep=output['delays'].append)
  uds.time = timer
  ecu_addrs.time = timer
  isotp_parallel_query.time = timer
  match case['op']:
    case 'decode':
      result = vin.decode_vin(bytes(case['bytes']))
    case 'valid':
      result = vin.is_valid_vin(case['vin'])
    case 'scan':
      result = [list(item) for item in sorted(ecu_addrs.get_ecu_addrs(receive, send, set(case['queries']), set(case['responses']), case['timeout']))]
      output['sent'].sort(key=lambda frame: (frame['address'], frame['data'], frame['bus']))
    case 'disable':
      address, subaddress, bus = case['target']
      result = disable_ecu.disable_ecu(receive, send, bus, address, subaddress, bytes(case['request']), case['timeout'], case['retry'])
    case 'vin':
      result = list(vin.get_vin(receive, send, case['buses'], case['timeout'], case['retry']))
    case unreachable:
      from typing import assert_never
      assert_never(unreachable)
  return Result(result=result, io=output)
