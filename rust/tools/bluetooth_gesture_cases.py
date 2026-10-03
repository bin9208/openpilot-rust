from dataclasses import dataclass
import json
import math
from pathlib import Path
import random
from typing import Literal, TypeAlias, TypedDict


class Event(TypedDict):
  kind: int
  code: int
  value: int
  at: float


class Feed(TypedDict):
  op: Literal['feed']
  event: Event


class Flush(TypedDict):
  op: Literal['flush']
  at: float


class Cancel(TypedDict):
  op: Literal['cancel']


Operation: TypeAlias = Feed | Flush | Cancel
Profile: TypeAlias = Literal['generic', 'yiser-j6']


@dataclass(frozen=True, slots=True)
class Case:
  profile: Profile
  mapping: dict[str, str]
  learning: bool
  operations: list[Operation]


def feed(raw: tuple[int, int, int, float]) -> Feed:
  kind, code, value, at = raw
  return {'op': 'feed', 'event': {'kind': kind, 'code': code, 'value': value, 'at': at}}


def operations() -> list[Operation]:
  result: list[Operation] = []
  at = 20.0
  for boundary in (0, .35, .4, .7, 1.5, 10):
    for duration in (math.nextafter(boundary, -math.inf), boundary, math.nextafter(boundary, math.inf)):
      result.extend([feed((1, 30, 1, at)), feed((0, 0, 0, at)), {'op': 'flush', 'at': at + duration},
                     feed((1, 30, 0, at + duration)), feed((0, 0, 0, at + duration)),
                     {'op': 'flush', 'at': at + duration + .35}, {'op': 'flush', 'at': at + duration + .4}])
      at += 30
  for gap in (.1, math.nextafter(.35, 0), .35, math.nextafter(.35, 1), .4, .5):
    for stamp in (at, at + .1 + gap):
      result.extend([feed((1, 30, 1, stamp)), feed((0, 0, 0, stamp)),
                     feed((1, 30, 0, stamp + .1)), feed((0, 0, 0, stamp + .1))])
    result.append({'op': 'flush', 'at': at + 1})
    at += 5
  for key in (30, 115, 255, 256, 329, 331, 351, 352, 767, 768, 65535):
    result.extend([feed((1, key, 1, at)), feed((0, 0, 0, at))])
    for offset in (.69, .7, 1.19, 1.2, 2.2, 9.99, 10, 10.001):
      result.append({'op': 'flush', 'at': at + offset})
    result.extend([{'op': 'cancel'}, feed((1, key, 0, at + 10.1)), feed((0, 0, 0, at + 10.1))])
    at += 15
  for x, y in ((300, 500), (235, 435), (365, 565), (366, 565), (420, 850), (355, 785),
               (485, 915), (-2147483648, 2147483647), (-13, 12), (-12, 13), (12, -13), (13, -12)):
    result.extend([feed((3, 0, x, at)), feed((3, 1, y, at)), feed((1, 330, 1, at)), feed((0, 0, 0, at))])
    for dx, dy in ((119, 0), (120, 0), (-120, 120), (0, -120), (0, 0)):
      at += .2
      result.extend([feed((3, 0, max(-2147483648, min(2147483647, x + dx)), at)),
                     feed((3, 1, max(-2147483648, min(2147483647, y + dy)), at)),
                     feed((0, 0, 0, at)), {'op': 'flush', 'at': at + .8}])
    result.extend([feed((1, 330, 0, at + 1)), feed((0, 0, 0, at + 1)), {'op': 'flush', 'at': at + 2}])
    at += 5
  rng = random.Random(155)
  events = [(0, 0, 0), (0, 3, 0), (1, 30, 1), (1, 30, 0), (1, 30, 2),
            (1, 115, 1), (1, 115, 0), (1, 330, 1), (1, 330, 0), (3, 0, 300), (3, 1, 500)]
  for _ in range(10000):
    at += rng.choice((-.01, 0, .001, .1, .35, .4, .5, .7, 1, 10))
    kind, code, value = rng.choice(events)
    result.append(feed((kind, code, value, at)))
    result.append({'op': 'flush', 'at': at + rng.choice((0, .35, .5, .7, 11))})
    if rng.randrange(10) == 0:
      result.append({'op': 'cancel'})
  return result


def cases(root: Path) -> list[Case]:
  gestures = {token + suffix: action for token in ('key:30', 'key:115', '1', '2', 'up', 'down', 'left', 'right', 'center')
              for suffix, action in (('', 'gapAdjustCruise'), ('@long', 'accelCruiseLong'), ('@double', 'cancel'))}
  result = [Case(profile, mapping, learning, operations()) for profile in ('generic', 'yiser-j6')
            for mapping in ({}, gestures, dict.fromkeys(gestures, 'none')) for learning in (False, True)]
  for boundary in (0, .35, .4, .7, 1.5, 10):
    for stamp in (math.nextafter(boundary, -math.inf), boundary, math.nextafter(boundary, math.inf)):
      result.append(Case('generic', gestures, False, [feed((1, 30, 1, 0.0)), feed((0, 0, 0, 0.0)),
                         {'op': 'flush', 'at': stamp}, feed((1, 30, 0, stamp)), feed((0, 0, 0, stamp)),
                         {'op': 'flush', 'at': stamp + .4}]))
  for boundary in (.35, .4):
    for stamp in (math.nextafter(boundary, -math.inf), boundary, math.nextafter(boundary, math.inf)):
      result.append(Case('generic', gestures, False, [feed((1, 30, 1, -.1)), feed((0, 0, 0, -.1)),
                         feed((1, 30, 0, 0.0)), feed((0, 0, 0, 0.0)), {'op': 'flush', 'at': stamp},
                         feed((1, 30, 1, stamp)), feed((1, 30, 0, stamp)), feed((0, 0, 0, stamp))]))
  recording = json.loads((root / 'openpilot/selfdrive/carrot/bluetooth/tests/yiser_j6.json').read_text())
  result.append(Case('yiser-j6', {}, False, [feed((kind, code, value, stamp)) for stamp, kind, code, value in recording]))
  return result
