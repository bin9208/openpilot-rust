import json
from pathlib import Path
import subprocess
import time
from typing import TypedDict


class Phase(TypedDict):
  wchan: str
  syscall: str


class Observation(TypedDict):
  at: float
  state: Phase


def wait_original_gps_sleep(process: subprocess.Popen[str], output: Path) -> Phase:
  deadline = time.monotonic() + 5
  proc = Path('/proc') / str(process.pid)
  observations: list[Observation] = []
  while True:
    assert process.poll() is None, ('source exited before GPS sleep', process.returncode)
    state: Phase = {'wchan': (proc / 'wchan').read_text().strip(), 'syscall': (proc / 'syscall').read_text().strip()}
    if not observations or state != observations[-1]['state']:
      observations.append({'at': time.monotonic(), 'state': state})
      (output / 'sleep-readiness.json').write_text(json.dumps(observations, indent=2) + '\n')
    if state['wchan'] == 'hrtimer_nanosleep':
      return state
    assert time.monotonic() < deadline, ('source did not enter GPS sleep', observations[-1])
    time.sleep(0.005)
