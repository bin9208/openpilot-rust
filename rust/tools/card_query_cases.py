"""Original-source parallel diagnostic query scenarios, with synthetic public CAN."""
from typing import TypedDict
from card_isotp_cases import Frame, segmented, response


class Case(TypedDict):
  bus: int
  addrs: list[tuple[int, int | None]]
  request: list[list[int]]
  response: list[list[int]]
  response_offset: int
  functional_addrs: list[int]
  response_pending_timeout: float
  timeout: float
  total_timeout: float
  batches: list[list[list[Frame]]]
  clock_step: float


def case(batches: list[list[list[Frame]]], addrs: list[tuple[int, int | None]] | None = None,
         request: list[list[int]] | None = None, expected: list[list[int]] | None = None,
         functional: list[int] | None = None, timeout: float = .1, total: float = 60., bus: int = 0) -> Case:
  return Case(bus=bus, addrs=addrs if addrs is not None else [(0x700, None)], request=request if request is not None else [[0x22, 0xf1, 0x90]],
              response=expected if expected is not None else [[0x62, 0xf1, 0x90]], response_offset=8,
              functional_addrs=functional or [], response_pending_timeout=.1, timeout=timeout, total_timeout=total,
              batches=[[]] + batches, clock_step=.001)


def cases() -> list[Case]:
  output: list[Case] = []
  for sub in (None, 0, 0x10, 0xff):
    for length in (0, 1, 7, 8, 9, 13, 20, 119, 120, 121, 300, 4095):
      payload = [0x62, 0xf1, 0x90] + [index % 256 for index in range(length)]
      frames = segmented(payload, sub)
      output.append(case([[frames]], addrs=[(0x700, sub)]))
      output.append(case([[[frame]] for frame in frames], addrs=[(0x700, sub)]))
  output.append(case([[segmented([0x50, 3])], [segmented([0x62, 0xf1, 0x90, 5])]], request=[[0x10, 3], [0x22, 0xf1, 0x90]], expected=[[0x50, 3], [0x62, 0xf1, 0x90]]))
  for delay in (0, 1, 10, 100):
    output.append(case([[segmented([0x7f, 0x22, 0x78])]] + [[]] * delay + [[segmented([0x62, 0xf1, 0x90, 5])]], timeout=.01))
  for wrong in ([0x7f, 0x22, 0x10], [0x62, 0xf1], [], [0x00, 0x00, 0x78], [0x62, 0xff, 0x90]):
    output.append(case([[segmented(wrong)], [segmented([0x62, 0xf1, 0x90, 5])]]))
  for timeout in (0., .001, .01, .1):
    output.append(case([], timeout=timeout))
    output.append(case([[segmented([0x62, 0xf1, 0x90, 5])]], timeout=timeout))
  output.append(case([[segmented([0x62, 0xf1, 0x90, 5])]], expected=[[]]))
  output.append(case([[segmented([])]], expected=[[]]))
  for bus in (0, 1, 3):
    for wrong_bus in (0, 1, 3):
      output.append(case([[segmented([0x62, 0xf1, 0x90, 5], bus=wrong_bus)]], bus=bus))
  for functional in ([0x7df], [0x18db33f1], [0x7df, 0x18db33f1]):
    output.append(case([[segmented([0x62, 0xf1, 0x90, 5])]], functional=functional))
  # Same response address, independently buffered subaddresses, duplicates preserve dict order.
  output.append(case([[segmented([0x62, 0xf1, 0x90, 5], 0x10) + segmented([0x62, 0xf1, 0x90, 6], 0x20)]], addrs=[(0x700, 0x10), (0x700, 0x20), (0x700, 0x10)]))
  output.append(case([[segmented([0x62, 0xf1, 0x90, 5], 0x20)]], addrs=[(0x700, 0x10)]))
  output.append(case([[[response([])]]], addrs=[(0x700, 0x10)]))
  output.append(case([[segmented([0x62, 0xf1, 0x90, 5], address=0x709) + segmented([0x62, 0xf1, 0x90, 6])]], addrs=[(0x700, None), (0x701, None)]))
  output.append(case([[segmented([0x62, 0xf1, 0x90, 5])]], total=0.))
  output.append(case([], addrs=[]))
  for payload in ([0x10], [0x10, 7, 1, 2, 3, 4, 5, 6], [0x22, 1], [0x40], [0x08, 1, 2, 3, 4, 5, 6, 7]):
    output.append(case([[[response(payload)]], [segmented([0x62, 0xf1, 0x90, 5])]]))
  # Source starts second request even when the first response times out this iteration.
  output.append(case([[segmented([0x50, 3])], [segmented([0x62, 0xf1, 0x90, 5])]], request=[[0x10, 3], [0x22, 0xf1, 0x90]], expected=[[0x50, 3], [0x62, 0xf1, 0x90]], timeout=0.))
  # Explicit source-constructor and runtime failures; no repaired protocol assumptions.
  output.append(case([], addrs=[(0x7df, None)]))
  output.append(case([], addrs=[(0xfff8, None)]))
  output.append(case([], request=[]))
  output.append(case([[segmented([0x62, 0xf1, 0x90, 5])]], expected=[]))
  for offset in (-640, -32, 0, 32):
    signed = case([[segmented([0x62, 0xf1, 0x90, 5], address=0x720 + offset)]], addrs=[(0x720, None)])
    signed['response_offset'] = offset
    output.append(signed)
  return output
