from __future__ import annotations

from dataclasses import dataclass
from functools import cache
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]


@dataclass(frozen=True)
class Scenario:
  candidate: str
  enabled: bool
  alpha_long: bool | None = None


@cache
def frames(candidate: str, tick: int, corpus_root: Path | None = None) -> list[dict]:
  if candidate.startswith('TESLA_'):
    return [frame for packet in tesla_case(candidate, corpus_root)['steps'][tick]['packets'] for frame in packet['frames']]
  if candidate == 'GENESIS_G70':
    case = genesis_case(corpus_root)
    return case['steps'][tick]['packets'][0]['frames']
  if candidate == 'COMMA_BODY':
    messages = (('MOTORS_DATA', {'SPEED_L': 400 + tick, 'SPEED_R': 300 + tick}),
                ('VAR_VALUES', {'MOTOR_ERR_L': 0, 'MOTOR_ERR_R': 0, 'FAULT': 0}),
                ('BODY_DATA', {'CHARGER_CONNECTED': tick % 2, 'BATT_PERCENTAGE': tick % 101}))
    result = []
    for name, values in messages:
      address, data, bus = body_packer().make_can_msg(name, 0, values)
      result.append({'address': address, 'data': list(data), 'bus': bus})
    return result
  return [{'address': 0x123, 'data': [0] * 8, 'bus': 0}]


@cache
def body_packer():
  from opendbc.can.packer import CANPacker
  return CANPacker(str(ROOT / 'opendbc_repo/opendbc/dbc/comma_body.dbc'))


@cache
def genesis_case(corpus_root: Path | None = None) -> dict:
  source = corpus_root / 'hyundai/state.json' if corpus_root is not None else ROOT / '.omo/evidence/card-hyundai/fixtures/state.json'
  rows = json.loads(source.read_text())
  return next(row for row in rows if row['candidate'] == 'GENESIS_G70')



@cache
def tesla_case(candidate: str, corpus_root: Path | None = None) -> dict:
  source = corpus_root / 'tesla/input.json' if corpus_root is not None else ROOT / '.omo/evidence/card-tesla/green/input.json'
  rows = json.loads(source.read_text())
  return next(row for row in rows if row['candidate'] == candidate and row['op'] != 'params')
