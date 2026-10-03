#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# ─── How to run ───
# Use retained repository bindings: python rust/tools/check_card_runtime_pumped.py --help
# ──────────────────
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
from typing import Final, TypedDict

from card_runtime_source import load_binding
from card_qa.runtime_inputs import Scenario, frames
from card_qa.runtime_catalog import CORPUS_DIRECTORIES, EXTENDED_BRANDS, FIXTURE_NAMES
from card_qa.runtime_pumped_capture import capture
from card_qa.runtime_pumped_types import Capture, Publications, RuntimeInvocation
from card_qa.runtime_pump import Frame, Row
from check_card_runtime import ROOT, SERVICES, wire
from check_card_vehicle import compare


BASE_SCENARIOS: Final = (('COMMA_BODY', True), ('COMMA_BODY', False), ('MOCK', True), ('GENESIS_G70', True), ('TESLA_MODEL_3', True), ('TESLA_MODEL_Y', True))


class FixturePacket(TypedDict):
  frames: list[Frame]


class FixtureStep(TypedDict):
  packets: list[FixturePacket]


class VehicleFixture(TypedDict):
  name: str
  candidate: str
  op: str
  steps: list[FixtureStep]


def extended_frames(candidate: str, corpus_root: Path | None = None) -> list[list[Frame]]:
  brand = EXTENDED_BRANDS[candidate]
  source = (corpus_root / brand / 'input.json' if corpus_root is not None else
    ROOT / '.omo/evidence' / ('card-' + brand) / CORPUS_DIRECTORIES.get(brand, 'green') / 'input.json')
  cases: list[VehicleFixture] = json.loads(source.read_text())
  matching = [row for row in cases if row['candidate'] == candidate and row['op'] == 'runtime']
  case = next(row for row in matching if row['name'] == FIXTURE_NAMES[candidate]) if candidate in FIXTURE_NAMES else matching[0]
  assert len(case['steps']) >= 80
  packets = [[frame for packet in step['packets'] for frame in packet['frames']] for step in case['steps'][:80]]
  if candidate == 'FORD_F_150_MK14':
    # Fixed-fingerprint startup observes the bus0 bootstrap packet;
    # the retained manual-transmission case exercised the additional Panda.
    assert all(frame['bus'] in (4, 6) for frames in packets for frame in frames)
    packets = [[{**frame, 'bus': frame['bus'] - 4} for frame in frames] for frames in packets]
  return packets


def fixture(candidate: str, corpus_root: Path | None = None) -> list[Row]:
  """Prebuild complete wire inputs so encoding cannot stall the sender."""
  rows = []
  retained_frames = extended_frames(candidate, corpus_root) if candidate in EXTENDED_BRANDS else None
  for tick in range(-1, 80):
    control = {'enabled': 20 <= tick < 65, 'latActive': 20 <= tick < 65, 'longActive': 20 <= tick < 65,
      'actuators': {'torque': (tick % 20 - 10) / 10 if tick >= 0 else 0., 'accel': .5, 'steeringAngleDeg': 2.}, 'hudControl': {'setSpeed': 20.}}
    messages = {name: list(wire(name, {})) for name in SERVICES if name not in ('pandaStates', 'onroadEvents', 'customReservedRawData0')}
    messages['pandaStates'] = list(wire('pandaStates', [{}]))
    messages['onroadEvents'] = list(wire('onroadEvents', []))
    messages['carControl'] = list(wire('carControl', control, valid=tick < 0 or tick % 11 != 0))
    can_frames = frames(candidate, max(tick, 0), corpus_root) if retained_frames is None else retained_frames[max(tick, 0)]
    rows.append({'messages': messages, 'frames': can_frames})
  return rows


def measured(capture_value: Capture, sends: Path) -> Publications:
  """Require one CAN packet per measured state before exact wire comparison."""
  rows = capture_value['stream']
  timestamps = [json.loads(line)['timestamp'] for line in sends.read_text().splitlines() if json.loads(line)['mode'] == 'stream']
  begin = next(index for index, row in enumerate(rows['carState']) if row['carState']['radarInput']['firstCanMonoTime'] == timestamps[0])
  end = next(index for index, row in enumerate(rows['carState']) if row['carState']['radarInput']['lastCanMonoTime'] == timestamps[-1]) + 1
  (sends.parent.parent / 'measured-window.json').write_text(json.dumps({'first_can_timestamp': timestamps[0], 'last_can_timestamp': timestamps[-1],
    'first_event_index': begin, 'end_event_index': end, 'raw_event_count': len(rows['carState']), 'pre_events': begin,
    'post_events': len(rows['carState']) - end}) + '\n')
  assert len(rows['carState']) == len(rows['carOutput'])
  assert len(timestamps) == end - begin == 80, (len(timestamps), end - begin, {key: len(value) for key, value in rows.items()})
  rows = {name: values[begin:end] for name, values in rows.items()}
  initial_timeout = capture_value['warmup']['carState'][-1]['carState']['canErrorCounter']
  for timestamp, row in zip(timestamps, rows['carState'], strict=True):
    state = row['carState']
    radar = state.pop('radarInput')
    assert radar['firstCanMonoTime'] == radar['lastCanMonoTime'] == timestamp, radar
    assert radar['canPacketCount'] == 1, radar
    assert radar['receiveMonoTime'] > 0
    assert state['canErrorCounter'] == initial_timeout, (state['canErrorCounter'], initial_timeout)
    state.pop('cumLagMs')
  assert len(rows['sendcan']) == (0 if capture_value['passive'] else 80)
  assert any(row['valid'] for row in rows['carOutput'])
  assert any(not row['valid'] for row in rows['carOutput'])
  if capture_value['params']['carFingerprint'] != 'MOCK':
    assert any(row['carState']['canValid'] for row in rows['carState']), 'no healthy CAN state in measured stream'
  for values in rows.values():
    for row in values:
      row.pop('logMonoTime')
  return rows


def main() -> None:
  """Compare original Card and native CLI at an independent nominal 100 Hz."""
  parser = argparse.ArgumentParser()
  for name in ('binary', 'numerics', 'binding', 'evidence'):
    parser.add_argument('--' + name, type=Path, required=True)
  parser.add_argument('--elf', type=Path)
  parser.add_argument('--simulation', action='store_true')
  parser.add_argument('--candidate', action='append')
  parser.add_argument('--can-interval', type=float, default=.01)
  parser.add_argument('--cc-every', type=int)
  parser.add_argument('--runtime-root', type=Path, default=ROOT)
  parser.add_argument('--corpus-root', type=Path)
  arguments = parser.parse_args()
  arguments.evidence.mkdir(parents=True, exist_ok=True)
  load_binding(arguments.binding)
  controls_every = arguments.cc_every if arguments.cc_every is not None else (5 if arguments.simulation else 1)
  if not 0 < arguments.can_interval < .02 or controls_every < 1:
    parser.error('CAN interval must be below20ms and control cadence positive')
  invocation = RuntimeInvocation(arguments.binary, arguments.numerics, arguments.binding, arguments.evidence, arguments.simulation,
    arguments.can_interval, controls_every, arguments.runtime_root)
  rows = []
  scenarios = BASE_SCENARIOS
  if arguments.candidate:
    available = {candidate for candidate, _ in BASE_SCENARIOS} | EXTENDED_BRANDS.keys()
    if unknown := set(arguments.candidate) - available:
      parser.error('unsupported fixture candidates: ' + ', '.join(sorted(unknown)))
    scenarios += tuple((candidate, True) for candidate in EXTENDED_BRANDS)
  for candidate, enabled in scenarios:
    if arguments.candidate and candidate not in arguments.candidate:
      continue
    (arguments.evidence / (candidate + '-inputs.json')).write_text(json.dumps(fixture(candidate, arguments.corpus_root)) + '\n')
    scenario = Scenario(candidate, enabled,
      alpha_long=True if candidate in FIXTURE_NAMES or candidate == 'FORD_MAVERICK_MK1' else None)
    source = capture(invocation, scenario, True)
    native = capture(invocation, scenario, False)
    expected = measured(source, arguments.evidence / f'{candidate}-{enabled}-source/pump/sends.jsonl')
    actual = measured(native, arguments.evidence / f'{candidate}-{enabled}-native/pump/sends.jsonl')
    compare(source['params'], native['params'])
    compare(expected, actual)
    rows.append({'candidate': candidate, 'enabled': enabled, 'steps': 80, 'passive': native['passive']})
  result = {'status': 'pass', 'scenarios': rows, 'simulation': arguments.simulation, 'runtime_python': False,
    'nominal_can_hz': 1. / arguments.can_interval, 'nominal_controls_hz': 1. / (arguments.can_interval * controls_every),
    'executed_elf_sha256': hashlib.sha256((arguments.elf or arguments.binary).read_bytes()).hexdigest(),
    'observable': ('full CP/CS/prior actuator output/CAN wire bytes and validity; actual constructor, interface, cruise, ' +
                   'startup readiness, SIGINT and async Params drain'),
    'scope': ('independent measured CAN+prebuilt topics at explicitly recorded nominal rates; 320 completed warmup steps ' +
              'with receiver completion gate and actual frequency/readiness barrier; ' +
              '80 independent compared sends with exact timestamp/packet count and no additional timeouts; ' +
              'no physical CAN/device or CPU claim')}
  (arguments.evidence / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
  print(json.dumps(result))


if __name__ == '__main__':
  main()
