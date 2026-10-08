from dataclasses import asdict, dataclass
import json
from pathlib import Path
import time
from typing import TypeAlias

import httpx2

Settings: TypeAlias = tuple[float, float, float, float, float]


def requested_settings(body: bytes) -> Settings:
  value = json.loads(body)
  return (value['threshold'], value['smoothingSeconds'], value['baseIntervalSeconds'],
          value['laneThreshold'], value['laneIntervalSeconds'])


def observed_settings(body: bytes) -> Settings:
  value = json.loads(body)
  return (value['threshold'], value['smoothingSeconds'], value['baseIntervalSeconds'],
          value['lane']['threshold'], value['lane']['intervalSeconds'])


@dataclass(frozen=True, slots=True)
class ReloadSample:
  status: int
  body: str
  observed_at: float


def wait_settings_reload(connection: httpx2.Client, body: bytes, output: Path) -> None:
  desired = requested_settings(body)
  wanted = (round(desired[0] * 100) / 100, round(desired[1] * 1000) / 1000,
            round(desired[2] * 1000) / 1000, round(desired[3] * 100) / 100,
            round(desired[4] * 1000) / 1000)
  samples: list[ReloadSample] = []
  deadline = time.monotonic() + 3.0
  while time.monotonic() < deadline:
    response = connection.get('/api/status')
    samples.append(ReloadSample(response.status_code, response.text, time.monotonic()))
    output.write_text(json.dumps({'wanted': wanted, 'samples': [asdict(sample) for sample in samples]}, indent=2) + '\n')
    response.raise_for_status()
    if observed_settings(response.content) == wanted:
      return
    time.sleep(.01)
  raise TimeoutError('Xiaoge settings never exposed their natural rounded Params reload')
