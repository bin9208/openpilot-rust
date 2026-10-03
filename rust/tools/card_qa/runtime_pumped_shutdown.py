from __future__ import annotations

import json
import signal
import time
from typing import TypedDict

from card_qa.runtime_pumped_types import Publication, Publications, RuntimePeer


class Shutdown(TypedDict):
  runtime_exit: int
  pump_exit: int
  first_empty_can: Publication
  empty_can_counter_increment: int


def finish(peer: RuntimePeer, stream: Publications, source: bool) -> Shutdown:
  """Observe the source 20 ms empty-CAN behavior before owned SIGINT shutdown."""
  from card_qa.runtime_pumped_capture import drain, set_phase
  states = stream['carState']
  final_can = next(row for row in reversed(states) if row['carState']['radarInput']['canPacketCount'] > 0)
  empty = next((row for row in states if row['carState']['radarInput']['canPacketCount'] == 0), None)
  empty_frame = peer.observed_frames + states.index(empty) if empty else None
  captured = {name: [] for name in stream}
  peer.process.send_signal(signal.SIGCONT)
  deadline = time.monotonic() + 3
  while True:
    assert peer.process.poll() is None, peer.process.returncode
    assert time.monotonic() < deadline, 'post-stream empty CAN not observed'
    for name, rows in drain(peer.subscribers).items():
      captured[name].extend(rows)
    if empty is None:
      for index, row in enumerate(captured['carState']):
        if row['carState']['radarInput']['canPacketCount'] == 0:
          empty = row
          empty_frame = peer.observed_frames + len(states) + index
          break
    trace = (peer.output / 'frequency.jsonl').read_text().split('\n')[:-1]
    if empty is not None and trace and json.loads(trace[-1])['frame'] >= empty_frame:
      break
    time.sleep(.001)
  increment = empty['carState']['canErrorCounter'] - final_can['carState']['canErrorCounter']
  assert increment == 1, increment
  radar = empty['carState']['radarInput']
  assert radar['firstCanMonoTime'] == radar['lastCanMonoTime'] == radar['canPacketCount'] == 0
  peer.process.send_signal(signal.SIGINT)
  runtime_exit = peer.process.wait(timeout=5)
  assert runtime_exit == (-signal.SIGINT if source else 130), runtime_exit
  set_phase(peer.output, 'stop')
  pump_exit = peer.pump.wait(timeout=5)
  assert pump_exit == 0
  result = Shutdown(runtime_exit=runtime_exit, pump_exit=pump_exit, first_empty_can=empty, empty_can_counter_increment=increment)
  (peer.output / 'shutdown-raw.json').write_text(json.dumps(dict(result=result, publications=captured)) + '\n')
  return result
