import copy
import itertools
import random
import struct
from typing import TypedDict


class Identity(TypedDict):
  hardware_type: int
  serial: list[int]


class Input(TypedDict):
  onroad: bool
  engaged: bool
  spoofing_started: bool


class PandaInput(TypedDict):
  health: list[int] | None
  can: list[list[int] | None]
  healthy: bool


class Step(TypedDict):
  input: Input
  now_ns: int
  pandas: list[PandaInput]
  listed: list[list[int]]


class Case(TypedDict):
  identities: list[Identity]
  steps: list[Step]


def cases() -> list[Case]:
  rng = random.Random(175)
  output: list[Case] = []
  for hardware in [[], [6], [7], [255], [6, 7], [7, 6], [6, 6], [10, 7, 8]]:
    identities: list[Identity] = [{'hardware_type': kind, 'serial': list(f'p{index}'.encode())} for index, kind in enumerate(hardware)]
    rows: list[Step] = []
    for iteration, (onroad, engaged, spoof, line, ignition_can, model, power) in enumerate(itertools.product(
        [False, True], [False, True], [False, True], [0, 1, 2], [0, 1], [0, 19, 1, 255], [0, 1, 2])):
      pandas: list[PandaInput] = []
      for index in range(len(hardware)):
        health = bytearray(rng.randbytes(58))
        health[32:34] = bytes([line if index == 0 else 0, ignition_can if index == 0 else 0])
        health[36] = model
        health[40] = power
        struct.pack_into('<H', health, 50, iteration % 7)
        pandas.append({'health': list(health), 'can': [list(rng.randbytes(64)) for _ in range(3)],
                       'healthy': iteration % 5 != 0 or index != 0})
      listed = [identity['serial'] for identity in identities]
      if iteration % 3 == 0:
        listed = [*listed, list(b'new'), list(b'later')]
      rows.append({'input': {'onroad': onroad, 'engaged': engaged, 'spoofing_started': spoof},
                   'now_ns': 1_000_000_000 + iteration * 100_000_000, 'pandas': pandas, 'listed': listed})
    output.append({'identities': identities, 'steps': rows})
  ids: list[Identity] = [{'hardware_type': 6, 'serial': list(b'internal')}, {'hardware_type': 7, 'serial': list(b'external')}]
  base: Step = {'input': {'onroad': True, 'engaged': True, 'spoofing_started': False}, 'now_ns': 1_000_000_000,
                'pandas': [{'health': [0] * 58, 'can': [[0] * 64 for _ in range(3)], 'healthy': True} for _ in ids],
                'listed': [identity['serial'] for identity in ids]}
  for index in range(2):
    for part in range(4):
      failure = copy.deepcopy(base)
      if part == 0:
        failure['pandas'][index]['health'] = None
      else:
        failure['pandas'][index]['can'][part - 1] = None
      output.append({'identities': ids, 'steps': [copy.deepcopy(base), failure, copy.deepcopy(base)]})
  return output
