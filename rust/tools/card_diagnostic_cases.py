# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# Imported by check_card_diagnostic.py.
from typing import Literal, TypedDict
import random
from card_isotp_cases import Frame, segmented


class Input(TypedDict):
  batches: list[list[list[Frame]]]
  clock_step: float


class Case(TypedDict, total=False):
  op: Literal['decode', 'valid', 'scan', 'disable', 'vin']
  bytes: list[int]
  vin: str
  queries: list[tuple[int, int | None, int]]
  responses: list[tuple[int, int | None, int]]
  target: tuple[int, int | None, int]
  request: list[int]
  timeout: float
  retry: int
  buses: list[int]
  io: Input


def cases() -> list[Case]:
  vin = b'1HGCM82633A004352'
  output: list[Case] = []
  for padding in (b'', b'\0', b'\xff', b'\0\xff', b' ', b'\t', b'\x1c', b'\x1f', b'\x85'):
    for lower in (False, True):
      value = vin.lower() if lower else vin
      output.append(Case(op='decode', bytes=list(padding + value + padding)))
      output.append(Case(op='decode', bytes=list(padding + b'\x11' + value + b'ignored' + padding)))
  for index in range(17):
    for byte in range(256):
      value = vin[:index] + bytes([byte]) + vin[index + 1:]
      output.append(Case(op='decode', bytes=list(value)))
      output.append(Case(op='valid', vin=value.decode('latin1')))
  rng = random.Random(177)
  for length in range(35):
    for _ in range(10):
      output.append(Case(op='decode', bytes=[rng.randrange(256) for _ in range(length)]))
  for subaddress in (None, 0, 0x10, 0xff):
    for bus in (0, 1, 5):
      for payload in ([], [0], [2], [2, 0x7e], [2, 0x7e, 0], [3, 0x7f, 0x3e, 0x11],
                      [0, 0x7e, 0], [8, 0x7e, 0], [2, 0x7f, 0], [2, 0x7e, 0, 0, 0, 0, 0, 0], [2, 0x7e, 0, 0, 0, 0, 0, 0, 0]):
        data = ([subaddress] if subaddress is not None else []) + payload
        frames = [Frame(address=0x708, data=data, bus=bus), Frame(address=0x708, data=data, bus=(bus + 1) % 6)]
        output.append(Case(op='scan', queries=[(0x700, subaddress, bus)], responses=[(0x708, subaddress, bus)], timeout=.004,
                           io=Input(batches=[[], [frames], [frames]], clock_step=.001)))
  for retry in (0, 1, 2, 10):
    for timeout in (0., .003):
      for response in ([], segmented([0x50, 3]), segmented([0x7f, 0x10, 0x10]), segmented([0x50, 2])):
        output.append(Case(op='disable', target=(0x700, None, 0), request=[0x28, 0x83, 1], timeout=timeout, retry=retry,
                           io=Input(batches=[[], [response], [], []], clock_step=.001)))
  protocols = [([0x62, 0xf1, 0x90], 0x7e0, 8), ([0x49, 2, 1], 0x7e0, 8), ([0x5a, 0x90], 0x24b, 0x400),
               ([0x61, 0x81], 0x797, 3), ([0x62, 0xf1, 0x90], 0x74f, 0x6a), ([0x62, 0xf1, 0x90], 0x733, 0x40)]
  for attempt, (prefix, address, offset) in enumerate(protocols):
    for data in (list(vin), list(vin.lower()), [0x11] + list(vin), [0] + list(vin) + [255], list(b'INVALID')):
      batches = [[], []] * attempt + [[], [segmented(prefix + data, address=address + offset)]]
      output.append(Case(op='vin', buses=[0], timeout=0., retry=1, io=Input(batches=batches, clock_step=.001)))
  for buses in ([], [0], [1], [2], [0, 1], [1, 0]):
    for retry in (0, 1, 2):
      output.append(Case(op='vin', buses=buses, timeout=0., retry=retry, io=Input(batches=[], clock_step=.001)))
  output.append(Case(op='vin', buses=[1], timeout=0., retry=1,
                     io=Input(batches=[[], [segmented([0x62, 0xf1, 0x90] + list(vin), bus=1, address=0x7e8)]], clock_step=.001)))
  return output
