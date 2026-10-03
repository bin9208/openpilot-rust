"""Run unchanged IsoTpParallelQuery with clock and transport callbacks only."""
from collections import deque
from typing import TypedDict
import types
from can_source import load
from card_query_cases import Case
from card_isotp_cases import Frame


class Result(TypedDict):
  result: object
  io: object


def trace(case: Case) -> Result:
  load()
  from opendbc.car import uds, isotp_parallel_query
  from opendbc.car.can_definitions import CanData
  batches = deque(case['batches'])
  now = 0.
  sent: list[Frame] = []
  delays: list[float] = []
  receives: list[bool] = []
  diagnostics: list[list[str]] = []
  def clock() -> float:
    nonlocal now
    now += case['clock_step']
    return now
  def receive(wait_for_one: bool = False):
    receives.append(wait_for_one)
    packets = batches.popleft() if batches else []
    return [[CanData(frame['address'], bytes(frame['data']), frame['bus']) for frame in packet] for packet in packets]
  def send(frames) -> None:
    sent.extend(Frame(address=frame.address, data=list(frame.dat), bus=frame.src) for frame in frames)
  timer = types.SimpleNamespace(monotonic=clock, sleep=delays.append)
  uds.time = timer
  isotp_parallel_query.time = timer
  previous_logger = isotp_parallel_query.carlog
  isotp_parallel_query.carlog = types.SimpleNamespace(
      error=lambda message: diagnostics.append(['error', message]),
      exception=lambda message: diagnostics.append(['exception', message]))
  try:
    query = isotp_parallel_query.IsoTpParallelQuery(send, receive, case['bus'], [tuple(addr) for addr in case['addrs']],
                    [bytes(data) for data in case['request']], [bytes(data) for data in case['response']], case['response_offset'],
                    case['functional_addrs'], case['response_pending_timeout'])
    output = query.get_data(case['timeout'], case['total_timeout'])
    result = dict(data=[[list(key), list(data)] for key, data in output.items()])
  except Exception as error:
    result = dict(error=type(error).__name__, detail=str(error))
  finally:
    isotp_parallel_query.carlog = previous_logger
  return Result(result=result, io=dict(sent=sent, delays=delays, receives=receives, now=now, diagnostics=diagnostics))
