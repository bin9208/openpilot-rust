#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# ─── How to run ───
# Use the repository's retained native Python bindings and PYTHONPATH:
# python rust/tools/card_qa/runtime_pump.py --help
# ──────────────────
from __future__ import annotations

import argparse
from dataclasses import dataclass
from enum import StrEnum
import json
import os
from pathlib import Path
import time
from typing import Final, TypedDict, assert_never

INTERVAL: Final = .01
WARMUP_FRAMES: Final = 320


class Phase(StrEnum):
  STARTUP = 'startup'
  WARMUP = 'warmup'
  STREAM = 'stream'
  PAUSE = 'pause'
  STOP = 'stop'


class Frame(TypedDict):
  address: int
  bus: int
  data: list[int]


class Row(TypedDict):
  messages: dict[str, list[int]]
  frames: list[Frame]


@dataclass(frozen=True, slots=True)
class PreparedRow:
  messages: tuple[tuple[str, bytes], ...]
  frames: list[Frame]


def main() -> None:
  """Publish immutable IPC inputs independently of the comparison receiver."""
  parser = argparse.ArgumentParser()
  for name in ('inputs', 'control', 'evidence'):
    parser.add_argument('--' + name, type=Path, required=True)
  parser.add_argument('--prefix', required=True)
  parser.add_argument('--cc-every', type=int, default=1)
  parser.add_argument('--can-interval', type=float, default=INTERVAL)
  args = parser.parse_args()
  if not 0 < args.can_interval < .02 or args.cc_every < 1:
    parser.error('CAN interval must be below20ms and control cadence positive')
  os.environ['OPENPILOT_PREFIX'] = args.prefix
  from openpilot.cereal import messaging
  from check_card_runtime import can_packet
  rows: list[Row] = json.loads(args.inputs.read_text())
  prepared = [PreparedRow(tuple((name, bytes(raw)) for name, raw in row['messages'].items()), row['frames']) for row in rows]
  publishers = {name: messaging.pub_sock(name) for name, _ in prepared[0].messages}
  publishers['can'] = messaging.pub_sock('can')
  args.evidence.mkdir(parents=True, exist_ok=True)
  (args.evidence / 'ready').write_text('ready\n')
  previous = None
  deadline = time.monotonic()
  tick = 0
  complete_marked = False
  with (args.evidence / 'sends.jsonl').open('w') as output:
    while True:
      command = args.control.read_text() if args.control.exists() else 'pause'
      phase = Phase(command)
      if phase != previous:
        previous = phase
        tick = 0
        complete_marked = False
        deadline = time.monotonic()
        (args.evidence / 'phase').write_text(phase.value)
      match phase:
        case Phase.STOP:
          return
        case Phase.PAUSE:
          time.sleep(.001)
          continue
        case Phase.WARMUP | Phase.STREAM:
          if phase == Phase.STREAM and tick == len(prepared) - 1 or phase == Phase.WARMUP and tick == WARMUP_FRAMES:
            if not complete_marked:
              (args.evidence / 'complete').write_text(phase.value)
              complete_marked = True
            time.sleep(.001)
            continue
          row = prepared[0] if phase == Phase.WARMUP else prepared[tick + 1]
        case Phase.STARTUP:
          row = prepared[0]
        case unreachable:
          assert_never(unreachable)
      delay = deadline - time.monotonic()
      if delay > 0:
        time.sleep(delay)
      if phase == Phase.WARMUP and tick:
        # Warmup establishes identical controller step counts, including slow
        # ECU initialization. The measured stream never waits for the receiver.
        start = int((args.evidence.parent / 'warmup-start-frame').read_text())
        trace = (args.evidence.parent / 'frequency.jsonl').read_text().split('\n')[:-1]
        if not trace or json.loads(trace[-1])['frame'] < start + tick - 1:
          time.sleep(.001)
          continue
      # Scheduler pauses remain in actual send-time evidence. Receiver progress
      # does not drive deadlines and missed deadlines cause no catch-up burst.
      for name, raw in row.messages:
        if (phase != Phase.STARTUP or name == 'pandaStates') and (name != 'carControl' or tick % args.cc_every == 0):
          publishers[name].send(raw)
      timestamp = time.monotonic_ns()
      frames = row.frames if phase != Phase.STARTUP else [Frame(address=291, bus=0, data=[0] * 8)]
      publishers['can'].send(can_packet(frames, timestamp))
      deadline = max(deadline + args.can_interval, time.monotonic() + args.can_interval)
      output.write(json.dumps({'tick': tick, 'mode': phase.value, 'timestamp': timestamp,
        'control_sent': phase != Phase.STARTUP and tick % args.cc_every == 0}) + '\n')
      output.flush()
      tick += 1


if __name__ == '__main__':
  main()
