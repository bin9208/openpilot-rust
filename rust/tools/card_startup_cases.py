"""Fingerprint and MAIN hold input cases from the original static catalog."""

import random

from can_source import load


def fingerprints():
  load()
  from opendbc.car.fingerprints import _FINGERPRINTS
  cases = []
  empty = dict(mono_time=0, frames=[])
  def packet(version, bus=0):
    return dict(mono_time=0, frames=[dict(address=address, data=[0] * length, bus=bus) for address, length in version.items()])
  for versions in _FINGERPRINTS.values():
    for version in versions:
      one = packet(version)
      cases.append([[one]] + [[empty]] * 201)
      cases.append([[one] + [empty] * 299])
    if versions:
      cases.append([[packet(versions[0], 1)]] + [[empty]] * 201)
    else:
      # Source keeps CHEVROLET_VOLT_CC in the legacy list with no passive signatures.
      cases.append([[packet({0x700: 1}, 1)]] + [[empty]] * 201)
  cases.extend([[[dict(mono_time=0, frames=[])]] * 202,
                [[packet({0x700: 1})]] * 202,
                [[packet({1880: 8})]] * 202,
                [[packet({1880: 9})]] * 202,
                [[], [], []] + [[packet({0x7df: 8, 0x7e0: 8, 0x7e8: 8, 0x18DAF110: 64}, 128)]] * 202])
  versions = [versions for versions in _FINGERPRINTS.values() if versions]
  both = packet(versions[0][0])
  both['frames'] += packet(versions[-1][0], 1)['frames']
  both['frames'] += packet({0x800: 8, 1880: 8, 0x7df: 8, 0x7e0: 8, 0x7e8: 8}, 9)['frames']
  cases.append([[both]] + [[empty]] * 201)
  return cases


def toggles():
  edge = [dict(buttons=[dict(kind=8, pressed=True)], engaged=False, now=0),
          dict(buttons=[], engaged=False, now=1.999999999),
          dict(buttons=[], engaged=False, now=2),
          dict(buttons=[], engaged=False, now=2.000000001),
          dict(buttons=[dict(kind=8, pressed=False), dict(kind=8, pressed=True)], engaged=False, now=3),
          dict(buttons=[], engaged=True, now=6),
          dict(buttons=[], engaged=False, now=6.1)]
  rng = random.Random(177)
  cases = [edge]
  for _ in range(20):
    clock = 0.
    steps = []
    for _ in range(1000):
      clock += rng.choice([0., 0.01, 0.5, 1.999999999, 2., 2.000000001, 3.])
      buttons = [dict(kind=rng.choice([8, 0, 5, 6, 9]), pressed=bool(rng.randrange(2))) for _ in range(rng.randrange(4))]
      steps.append(dict(buttons=buttons, engaged=bool(rng.randrange(2)), now=clock))
    cases.append(steps)
  return cases
