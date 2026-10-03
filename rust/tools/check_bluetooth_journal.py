# /// script
# requires-python = ">=3.12,<3.13"
# dependencies = []
# ///
import argparse
import hashlib
import json
import math
from pathlib import Path
import re
import subprocess
import sys
from typing import Literal, TypedDict, TypeAlias, assert_never

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))
from openpilot.selfdrive.carrot.bluetooth.model import CommandWriter

MACS = ['66:C0:0C:7B:6E:71', 'AA:BB:CC:DD:EE:FF']


class Send(TypedDict):
  op: Literal['send']
  address: str
  action: str
  at: float
  hold: str | None
  repeat: bool


class Prune(TypedDict):
  op: Literal['prune']
  addresses: list[str]
  at: float
  holds: list[str] | None


class Publish(TypedDict):
  op: Literal['publish']
  channel: Literal['cruise', 'lane']


Operation: TypeAlias = Send | Prune | Publish


def operations() -> list[Operation]:
  result: list[Operation] = [{'op': 'publish', 'channel': 'cruise'}]
  for index in range(300):
    result.append({'op': 'send', 'address': MACS[index % 2], 'action': ['accelCruise', 'cancel', 'laneLeft', 'laneRight'][index % 4],
                   'at': 10 + index * .0001, 'hold': None, 'repeat': False})
  result.extend([{'op': 'prune', 'addresses': MACS, 'at': 10.03, 'holds': None},
                 {'op': 'prune', 'addresses': [MACS[0]], 'at': 10.03, 'holds': []}])
  for index, at in enumerate((0.0, math.nextafter(.4, 0), .4, math.nextafter(.4, 1), 10.0, 9.9, 10.4, 10.5)):
    result.append({'op': 'send', 'address': MACS[0], 'action': 'decelCruiseLong', 'at': at,
                   'hold': '/dev/input/event0:up@long', 'repeat': bool(index % 2)})
    result.append({'op': 'prune', 'addresses': MACS, 'at': at, 'holds': ['/dev/input/event0:up@long']})
  for index in range(200):
    at = 20 + index * .07
    result.append({'op': 'send', 'address': MACS[index % 2], 'action': 'gapAdjustCruise', 'at': at,
                   'hold': [None, '', 'held-A', 'held-B'][index % 4], 'repeat': bool(index % 3)})
    result.append({'op': 'prune', 'addresses': MACS, 'at': at, 'holds': None if index % 3 == 0 else ['held-A']})
  result.extend([{'op': 'prune', 'addresses': [], 'at': 100, 'holds': []}, {'op': 'publish', 'channel': 'lane'}])
  return result


def original(root: Path, commands: list[Operation]):
  writer = CommandWriter(root)
  for channel in writer.events:
    writer.publish(channel)
  result = []
  for command in commands:
    match command['op']:
      case 'send':
        writer.send(command['address'], command['action'], command['at'], hold=command['hold'], repeat=command['repeat'])
      case 'prune':
        holds = None if command['holds'] is None else set(command['holds'])
        writer.prune(set(command['addresses']), command['at'], holds)
      case 'publish':
        writer.publish(command['channel'])
      case unreachable:
        assert_never(unreachable)
    result.append([{'channel': channel, 'bytes': (root / f'{channel}.json').read_text(),
                    'mode': (root / f'{channel}.json').stat().st_mode & 0o777,
                    'inode': (root / f'{channel}.json').stat().st_ino} for channel in writer.events])
  return result


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  commands = operations()
  payload = ''.join(json.dumps(command) + '\n' for command in commands)
  (args.output / 'input.jsonl').write_text(payload)
  expected = original(args.output / 'original', commands)
  (args.output / 'original.jsonl').write_text(''.join(json.dumps(step) + '\n' for step in expected))
  native = subprocess.run([str(args.binary.resolve()), str(args.output.resolve() / 'native')],
                          input=payload, text=True, capture_output=True, timeout=30)
  (args.output / 'native.jsonl').write_text(native.stdout)
  (args.output / 'native.stderr').write_text(native.stderr)
  native.check_returncode()
  actual = [json.loads(line) for line in native.stdout.splitlines()]
  assert len(actual) == len(expected)
  previous = [{}, {}]
  sessions = [set(), set()]
  max_events = 0
  for index, (source, replacement) in enumerate(zip(expected, actual, strict=True)):
    normalized = []
    for side, snapshot in enumerate((source, replacement)):
      contents = []
      for file in snapshot:
        assert file['mode'] == 0o600
        journal = json.loads(file['bytes'])
        max_events = max(max_events, len(journal['events']))
        assert len(journal['events']) <= 64
        for event in journal['events']:
          assert re.fullmatch(r'[0-9a-f]{32}:[1-9][0-9]*', event['id']), event
          session, sequence = event['id'].split(':')
          sessions[side].add(session)
          event['id'] = sequence
        channel = file['channel']
        rewritten = None if channel not in previous[side] else previous[side][channel] != file['inode']
        previous[side][channel] = file['inode']
        contents.append({'channel': channel, 'journal': journal, 'rewritten': rewritten})
      normalized.append(contents)
    assert normalized[0] == normalized[1], (index, commands[index], normalized)
  assert all(len(values) == 1 for values in sessions), sessions
  assert max_events == 64
  for side in ('original', 'native'):
    assert sorted(path.name for path in (args.output / side).iterdir()) == ['cruise.json', 'lane.json']
  summary = {'operations': len(commands), 'max_events_per_channel': max_events, 'mode': '0600',
             'values_and_rewrite_decisions': 'exact after UUID session normalization',
             'source_sha256': hashlib.sha256((ROOT / 'openpilot/selfdrive/carrot/bluetooth/model.py').read_bytes()).hexdigest(),
             'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest()}
  (args.output / 'result.json').write_text(json.dumps(summary, indent=2))
  print(json.dumps(summary))


if __name__ == '__main__':
  main()
