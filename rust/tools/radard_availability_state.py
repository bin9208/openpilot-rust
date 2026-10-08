# /// script
# requires-python = ">=3.12"
# dependencies = ["pycapnp==2.1.0", "numpy"]
# ///
# Run with the existing oracle environment: python -P rust/tools/radard_availability_state.py --binary STATE_PROBE --output OUT
from __future__ import annotations

import argparse
import json
from pathlib import Path
import subprocess
from typing import Final, TypedDict

from check_message_state import source

TIMES: Final = (1.0, 1.499999, 1.5, 1.500001, 1.55)
ALIVE: Final = (True, True, False, False, True)


class Snapshot(TypedDict):
  time: float
  received: float
  alive: bool


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True)
  scope, environment = source()
  environment['simulation'] = '0'
  state = scope['SubMaster'](['liveTracks'], poll='liveTracks')
  originals: list[Snapshot] = []
  request = args.output / 'request.jsonl'
  actual = args.output / 'native.jsonl'
  with request.open('w') as requests:
    for index, now in enumerate(TIMES):
      messages = [scope['new_message']('liveTracks', valid=True)] if index in (0, 4) else []
      for message in messages:
        message.logMonoTime = index * 50_000_000
      configuration = {'services': ['liveTracks'], 'options': {'poll': {'one': 'liveTracks'}}} if index == 0 else None
      requests.write(json.dumps({'configuration': configuration, 'time': now,
                                 'messages': [list(message.to_bytes()) for message in messages], 'checks': [[]]}) + '\n')
      state.update_msgs(now, [message.as_reader() for message in messages])
      originals.append({'time': now, 'received': state.recv_time['liveTracks'], 'alive': state.alive['liveTracks']})
  (args.output / 'source.json').write_text(json.dumps(originals, indent=2) + '\n')
  command = [str(args.binary.resolve()), str(request.resolve()), str(actual.resolve())]
  subprocess.run(command, check=True)
  native = [json.loads(line) for line in actual.read_text().splitlines()]
  for index, expected in enumerate(ALIVE):
    assert originals[index]['alive'] is expected, originals[index]
    assert native[index]['topics'][0]['alive'] is expected, native[index]
    assert native[index]['topics'][0]['receive_time'] == originals[index]['received']
    assert native[index]['error'] is None
  report = {'updates': len(TIMES), 'source_native_alive': list(ALIVE), 'passed': True,
            'epsilon_seconds': 1e-6, 'deadline_seconds': 0.5, 'simulation': False, 'command': command}
  (args.output / 'receipt.json').write_text(json.dumps(report, indent=2) + '\n')
  print(json.dumps(report))


if __name__ == '__main__':
  main()
