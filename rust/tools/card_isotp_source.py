"""Execute entire unchanged CanClient/IsoTpMessage with transport and time boundaries."""
from collections import deque
from typing import TypedDict
import types

from can_source import load
from card_isotp_cases import Case, Frame, Request


class Output(TypedDict):
  cases: list[object]
  addresses: list[object]


class Transport:
  def __init__(self) -> None:
    self.batches: deque[list[Frame]] = deque()
    self.sent: list[Frame] = []
    self.delays: list[float] = []
    self.receives = 0
    self.now = 0.
    self.clock_step = 0.

  def receive(self) -> list[tuple[int, bytes, int]]:
    self.receives += 1
    batch = self.batches.popleft() if self.batches else []
    return [(frame['address'], bytes(frame['data']), frame['bus']) for frame in batch]

  def send(self, address: int, data: bytes, bus: int) -> None:
    self.sent.append(Frame(address=address, data=list(data), bus=bus))

  def clock(self) -> float:
    self.now += self.clock_step
    return self.now


def trace(case: Case) -> object:
  from opendbc.car import uds
  transport = Transport()
  uds.time = types.SimpleNamespace(monotonic=transport.clock, sleep=transport.delays.append)
  client = uds.CanClient(transport.send, transport.receive, case['tx_addr'], case['rx_addr'], case['bus'], case['sub_addr'], case['rx_sub_addr'])
  try:
    state = uds.IsoTpMessage(client, case['timeout'], case['single_frame_mode'], case['separation_time'])
  except Exception as error:
    return dict(error=type(error).__name__, detail=str(error))
  steps = []
  for step in case['steps']:
    transport.batches.extend(step['batches'])
    transport.clock_step = step['clock_step']
    try:
      match step['op']:
        case 'send':
          state.send(bytes(step['data']), step['setup_only'])
          result = dict(success=True)
        case 'receive':
          data, in_progress = state.recv(step['timeout'])
          result = dict(data=list(data) if data is not None else None, in_progress=in_progress)
        case 'drain':
          client._recv_buffer(True)
          result = dict(success=True)
        case unknown:
          raise ValueError(f'unknown transport operation {unknown}')
    except Exception as error:
      result = dict(error=type(error).__name__, detail=str(error))
    snapshot = dict(client=dict(tx_addr=client.tx_addr, rx_addr=client.rx_addr, bus=client.bus, sub_addr=client.sub_addr,
                               rx_sub_addr=client.rx_sub_addr, rx_buff=[list(data) for data in client.rx_buff]),
                    timeout=state.timeout, single_frame_mode=state.single_frame_mode, max_len=state.max_len,
                    flow_control_msg=list(state.flow_control_msg), tx_dat=list(state.tx_dat), tx_len=state.tx_len, tx_idx=state.tx_idx,
                    tx_done=state.tx_done, rx_dat=list(state.rx_dat), rx_len=state.rx_len, rx_idx=state.rx_idx, rx_done=state.rx_done)
    io = dict(sent=transport.sent.copy(), delays=transport.delays.copy(), receives=transport.receives, now=transport.now)
    steps.append(dict(result=result, state=snapshot, io=io))
  return dict(steps=steps)


def run(request: Request) -> Output:
  load()
  from opendbc.car import uds
  addresses = []
  for tx, offset in request['addresses']:
    try:
      addresses.append(dict(value=uds.get_rx_addr_for_tx_addr(tx, offset)))
    except Exception as error:
      addresses.append(dict(error=type(error).__name__, detail=str(error)))
  return Output(cases=[trace(case) for case in request['cases']], addresses=addresses)
