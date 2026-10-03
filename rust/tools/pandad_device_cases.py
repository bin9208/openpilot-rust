import random
from typing import NotRequired, TypedDict


class Reply(TypedDict):
  request: int
  count: int
  bytes: list[int]


class Operation(TypedDict):
  op: str
  bus: NotRequired[int]
  port: NotRequired[int]


class Case(TypedDict):
  replies: list[Reply]
  operations: list[Operation]


def cases() -> list[Case]:
  output: list[Case] = []
  for hardware in range(256):
    output.append({'replies': [{'request': 0xc1, 'count': 1, 'bytes': [hardware]}], 'operations': []})
  rng = random.Random(175)
  for request, operation, size in [(0xd2, 'health', 58), (0xc2, 'can_health', 64), (0xb2, 'fan_speed', 2), (0xd0, 'serial', 16)]:
    for count in [-4, -1, *range(size + 1)]:
      for pattern in [0, 255, 73]:
        payload = [pattern] * max(0, count)
        if pattern == 73:
          payload = [rng.randrange(256) for _ in payload]
        op: Operation = {'op': operation}
        if operation == 'can_health':
          op['bus'] = rng.choice([0, 1, 2, 3, 65535])
        output.append({'replies': [{'request': 0xc1, 'count': 0, 'bytes': []},
                                   {'request': request, 'count': count, 'bytes': payload}], 'operations': [op]})
  for _ in range(1000):
    output.append({'replies': [{'request': 0xc1, 'count': -4, 'bytes': []},
                               {'request': 0xd2, 'count': 58, 'bytes': [rng.randrange(256) for _ in range(58)]},
                               {'request': 0xc2, 'count': 64, 'bytes': [rng.randrange(256) for _ in range(64)]}],
                   'operations': [{'op': 'health'}, {'op': 'can_health', 'bus': rng.randrange(65536)}]})
  for first in [-4, 0, 63, 64]:
    for second in [-4, 0, 63, 64]:
      output.append({'replies': [{'request': 0xc1, 'count': 1, 'bytes': [10]},
                                 {'request': 0xd3, 'count': first, 'bytes': [1] * max(0, first)},
                                 {'request': 0xd4, 'count': second, 'bytes': [2] * max(0, second)}],
                     'operations': [{'op': 'signature'}]})
  for length in [0, 1, 2, 16, 63, 64]:
    for terminal in [-4, -1, 0]:
      data = [rng.randrange(256) for _ in range(length)]
      if length:
        data[length // 2] = 0
      replies: list[Reply] = [{'request': 0xc1, 'count': 1, 'bytes': [7]}]
      if length:
        replies.extend([{'request': 0xe0, 'count': length, 'bytes': data}] * 3)
      replies.append({'request': 0xe0, 'count': terminal, 'bytes': []})
      output.append({'replies': replies, 'operations': [{'op': 'serial_read', 'port': 65535}]})
  return output
