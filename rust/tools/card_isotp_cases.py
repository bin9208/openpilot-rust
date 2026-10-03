"""Public synthetic ISO-TP transport scenarios for unchanged-source comparison."""
from typing import TypedDict


class Frame(TypedDict):
  address: int
  data: list[int]
  bus: int


class Step(TypedDict):
  op: str
  data: list[int]
  setup_only: bool
  batches: list[list[Frame]]
  timeout: float | None
  clock_step: float


class Case(TypedDict):
  tx_addr: int
  rx_addr: int | None
  bus: int
  sub_addr: int | None
  rx_sub_addr: int | None
  timeout: float
  single_frame_mode: bool
  separation_time: float
  steps: list[Step]


class Request(TypedDict):
  cases: list[Case]
  addresses: list[tuple[int, int]]


def step(op: str, data: list[int] | None = None, frames: list[Frame] | None = None,
         batches: list[list[Frame]] | None = None, setup_only: bool = False,
         timeout: float | None = None) -> Step:
  return Step(op=op, data=data or [], setup_only=setup_only, batches=batches if batches is not None else [frames or []],
              timeout=timeout, clock_step=0.01)


def response(data: list[int], sub: int | None = None, address: int = 0x708, bus: int = 0) -> Frame:
  return Frame(address=address, data=([sub] if sub is not None else []) + data, bus=bus)


def segmented(data: list[int], sub: int | None = None, address: int = 0x708, bus: int = 0) -> list[Frame]:
  size = 8 if sub is None else 7
  if len(data) < size:
    return [response(([len(data)] + data + [0] * size)[:size], sub, address, bus)]
  frames = [response([0x10 | (len(data) >> 8), len(data) & 255] + data[:size - 2], sub, address, bus)]
  for index, offset in enumerate(range(size - 2, len(data), size - 1), 1):
    frames.append(response(([0x20 | (index & 15)] + data[offset:offset + size - 1] + [0] * size)[:size], sub, address, bus))
  return frames


def case(steps: list[Step], sub: int | None = None, single: bool = False, separation: float = 0.01,
         tx: int = 0x700, rx: int | None = 0x708, bus: int = 0, rx_sub: int | None = None) -> Case:
  return Case(tx_addr=tx, rx_addr=rx, bus=bus, sub_addr=sub, rx_sub_addr=rx_sub, timeout=0.,
              single_frame_mode=single, separation_time=separation, steps=steps)


def cases() -> Request:
  result: list[Case] = []
  for sub in (None, 0, 0x10, 0xff):
    for single in (False, True):
      for length in (0, 1, 5, 6, 7, 8, 9, 13, 20, 119, 120, 121, 300, 4095):
        data = [index % 256 for index in range(length)]
        frames = segmented(data, sub)
        result.append(case([step('send', [0x22, 0xf1, 0x90]), step('receive', frames=frames)], sub, single))
        result.append(case([step('send', data), step('receive', frames=[response([0x30, 0, 0], sub)]),
                            step('receive', frames=segmented([0x62, 0xf1, 0x90, 0x55], sub))], sub, single))
  for separation in (0., .0001, .00015, .00025, .0005, .00075, .0009, .0009000001,
                     .001, .0015, .0025, .0035, .01, .1265, .127, -.0001, .127000001):
    result.append(case([step('send', [1])], separation=separation))
  for indicator in (0x30, 0x31, 0x32, 0x33, 0x3f):
    for count in (0, 1, 2, 30):
      for delay in (0, 1, 0x7f, 0x80, 0xf1, 0xf9):
        result.append(case([step('send', [0x22] * 20), step('receive', frames=[response([indicator, count, delay])]),
                            step('receive', frames=[response([0x30, 0, 0])])]))
  malformed = [[0x0f], [0x08] + [0] * 7, [0x10], [0x10, 7] + [0] * 6, [0x10, 9] + [0] * 5,
               [0x21], [0x22], [0x30], [0x31], [0x32], [0x33], [0x40], [0xff], [0]]
  for payload in malformed:
    result.append(case([step('send', [1]), step('receive', frames=[response(payload)])]))
  for payload in ([0x10, 9, 1, 2, 3, 4, 5, 6], [1, 4], [0x22, 7], [0x21, 7, 8], [0x30, 0, 0]):
    result.append(case([step('send', [1]), step('receive', frames=[response([0x10, 9, 1, 2, 3, 4, 5, 6])]),
                        step('receive', frames=[response(payload)])]))
  # Source buffers multiple complete responses and returns one per recv call.
  result.append(case([step('send', [1]), step('receive', frames=[response([3, 0x7f, 0x22, 0x78]), response([2, 0x62, 1])]), step('receive')]))
  # Source send's unconsumed generator leaves queued transport data intact.
  result.append(case([step('send', [1], frames=[response([1, 9])]), step('receive'), step('drain'), step('send', [2]), step('receive')]))
  # rx_buffer continues after a full 254-frame batch, and serves during each ten sends.
  result.append(case([step('send', [1]), step('receive', batches=[[response([], address=0x710)] * 254, [response([1, 9])]])]))
  result.append(case([step('send', [1] * 120), step('receive', frames=[response([0x30, 0, 0]), response([1, 9])])]))
  # Functional standard-address source branch accepts a response from another bus.
  result.append(case([step('send', [1]), step('receive', frames=[response([1, 9], address=0x7e8, bus=1)])], tx=0x7df, rx=None))
  result.append(case([step('send', [1]), step('receive', frames=[response([1, 9], address=0x18daf110)])], tx=0x18db33f1, rx=None))
  result.append(case([step('send', [1]), step('receive', frames=[response([1, 9], address=0x18daf110, bus=1)]), step('receive')], tx=0x18db33f1, rx=None))
  # Functional query setup, separate receive subaddress and empty remote frames.
  result.append(case([step('send', [1], setup_only=True), step('receive', frames=[response([1, 9])])]))
  result.append(case([step('send', [1]), step('receive', frames=[response([1, 9], 0x10)])], sub=0x20, rx_sub=0x10))
  result.append(case([step('send', [1]), step('receive', frames=[response([1, 9], 0x11)])], sub=0x20, rx_sub=0x10))
  # Blocking receive timeout and optimized CAN FD single frame.
  result.append(case([step('send', [1]), step('receive', timeout=.025)]))
  for sub in (None, 0x10):
    for length in (0, 1, 8, 61, 62, 63, 255):
      result.append(case([step('send', [1]), step('receive', frames=[response([0, length] + [1] * 62, sub)])], sub=sub))
  # Invalid large request keeps the source unsigned-short packing failure.
  result.append(case([step('send', [1] * 65536)]))
  addresses = [(address, offset) for address in (0, 0x700, 0x7df, 0xfff7, 0xfff8, 0x10000000, 0x10000001,
               0x18db33f1, 0x18da10f1, 0x18daf110, 0xfffffffe, 0xffffffff) for offset in (-640, -0x701, -32, 0, 3, 8, 0x400)]
  return Request(cases=result, addresses=addresses)
