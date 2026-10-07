# /// script
# requires-python = ">=3.12"
# dependencies = ["pycapnp==2.1.0", "pyzmq", "pyserial", "requests", "setproctitle", "zstandard", "numpy"]
# ///
from __future__ import annotations

import argparse
from collections.abc import Mapping
from dataclasses import dataclass
import gc
import json
import os
from pathlib import Path
import signal
import subprocess
import tempfile
import time
from typing import Final, Literal, TypedDict, assert_never

from capnp.lib.capnp import _DynamicStructBuilder
from openpilot.cereal import car, log, messaging
from radard_compare import differences

ROOT: Final = Path(__file__).resolve().parents[2]
type Mode = Literal['source', 'rust']
type Topic = Literal['modelV2', 'carState', 'liveTracks', 'livePose']
type Json = None | bool | int | float | str | list[Json] | dict[str, Json]
SERVICES: Final[list[Topic]] = ['modelV2', 'carState', 'liveTracks', 'livePose']


class Publication(TypedDict):
  valid: bool
  data: Json


@dataclass(frozen=True, slots=True)
class Paths:
  python: Path
  binary: Path
  binding: Path
  output: Path


@dataclass(frozen=True, slots=True)
class Scenario:
  brand: str
  radar_mode: int
  corner_mode: int
  unavailable: bool
  availability: bool = False


SCENARIOS: Final = {
  'hyundai-corner': Scenario('hyundai', 1, 1, False),
  'hyundai-scc': Scenario('hyundai', 2, 0, False),
  'hyundai-vision': Scenario('hyundai', -2, 0, False),
  'volkswagen-meb': Scenario('volkswagen', 3, 1, False),
  'toyota-unavailable': Scenario('toyota', 0, 1, True),
  'availability': Scenario('toyota', 0, 0, False, True),
}


def arguments(cases: tuple[str, ...]) -> tuple[Paths, str | None]:
  parser = argparse.ArgumentParser()
  for name in ('python', 'binary', 'binding', 'output'):
    parser.add_argument('--' + name, type=Path, required=True)
  parser.add_argument('--case', choices=cases)
  args = parser.parse_args()
  return Paths(args.python.absolute(), args.binary.absolute(), args.binding.absolute(), args.output.absolute()), args.case


def command(mode: Mode, paths: Paths) -> list[str]:
  match mode:
    case 'source':
      return [str(paths.python), '-P', '-c',
              f'from pathlib import Path; from card_runtime_source import load_binding; load_binding(Path({str(paths.binding)!r})); '
              'from openpilot.selfdrive.carrot.radar.radard_dpath import main; main()']
    case 'rust':
      return [str(paths.binary)]
    case unreachable:
      assert_never(unreachable)


def environment(params: Path, prefix: str) -> Mapping[str, str]:
  pythonpath = os.pathsep.join([os.environ.get('PYTHONPATH', ''), str(ROOT / 'rust/tools'), str(ROOT), str(ROOT / 'opendbc_repo')])
  return dict(os.environ, PARAMS_ROOT=str(params.parent), OPENPILOT_PREFIX=prefix, OPENPILOT_ROOT=str(ROOT),
              LOGPRINT='info', OPENBLAS_NUM_THREADS='1', PYTHONDONTWRITEBYTECODE='1', PYTHONPATH=pythonpath)


def packet(topic: Topic, index: int) -> _DynamicStructBuilder:
  stamp = 5_000_000_000 + index * 50_000_000
  event = messaging.new_message(topic, logMonoTime=stamp, valid=True)
  match topic:
    case 'carState':
      event.carState.vEgo = 20.0
    case 'livePose':
      pose = event.livePose
      pose.inputsOK = pose.sensorsOK = index != 7
      pose.angularVelocityDevice.valid = index != 8
      pose.angularVelocityDevice.z = 0.1 if index < 8 else 0.0
      event.valid = False
    case 'liveTracks':
      event.logMonoTime = stamp - (600_000_000 if index == 22 else 10_000_000)
      event.valid = index != 18
      event.liveTracks.errors.canError = index == 19
      points = event.liveTracks.init('points', 4 if index != 21 else 0)
      for n, point in enumerate(points):
        point.trackId, point.measured = n + 1, True
        point.radarSource = ['frontRadar', 'corner235', 'frontRadar', 'scc'][n]
        point.dRel = [40.0, 25.0, 25.0, 24.0][n]
        point.yRel = [0.0, 3.0 - min(index, 30) * 0.08, 3.0 - min(index, 30) * 0.08, 0.0][n]
        point.vRel, point.yvRel = -2.0, -1.6 if n in (1, 2) else 0.0
        point.aRel, point.vLead, point.aLead, point.jLead = -0.1, 18.0, -0.2, 0.1
        point.trackState = 4
    case 'modelV2':
      model = event.modelV2
      model.timestampEof = 0 if index == 10 else stamp - 20_000_000
      if index == 11:
        model.timestampEof = event.logMonoTime = 0
      model.position.x = [] if index == 24 else [float(n * 5) for n in range(33)]
      model.position.y = [] if index == 24 else [0.0] * 33
      model.velocity.x, model.laneLineProbs = [20.0] * 33, [0.1, 0.95, 0.95, 0.1]
      lead = model.init('leadsV3', 1)[0]
      lead.prob, lead.x, lead.y, lead.v, lead.a = 0.98, [41.52], [0.0], [18.0], [-0.2]
      lead.xStd, lead.yStd, lead.vStd = [1.0], [0.3], [0.3]
    case unreachable:
      assert_never(unreachable)
  return event


def wait_ready(publisher: messaging.PubMaster, child: subprocess.Popen[bytes]) -> None:
  deadline = time.monotonic() + 15.0
  while time.monotonic() < deadline:
    if child.poll() is not None:
      raise RuntimeError(f'daemon exited before ready: {child.returncode}')
    if all(socket.all_readers_updated() for socket in publisher.sock.values()):
      return
    time.sleep(0.01)
  raise TimeoutError('input subscriptions absent')


def run(mode: Mode, paths: Paths, scenario: Scenario) -> list[Publication]:
  case = paths.output / f'{scenario.brand}-{scenario.radar_mode}-{scenario.corner_mode}-{scenario.unavailable}-{mode}'
  case.mkdir(parents=True)
  with tempfile.TemporaryDirectory(prefix='msgq_radard_', dir='/dev/shm') as shared:
    prefix = Path(shared).name.removeprefix('msgq_')
    os.environ['OPENPILOT_PREFIX'] = prefix
    params = case / 'params' / prefix
    params.mkdir(parents=True)
    (params / 'EnableRadarTracks').write_text(str(scenario.radar_mode))
    (params / 'EnableCornerRadar').write_text(str(scenario.corner_mode))
    publisher = messaging.PubMaster(SERVICES)
    rows: list[Publication] = []
    with (case / 'stdout.log').open('wb') as stdout, (case / 'stderr.log').open('wb') as stderr:
      child = subprocess.Popen(command(mode, paths), env=environment(params, prefix), cwd=ROOT, stdout=stdout, stderr=stderr)
      try:
        time.sleep(0.2)
        assert child.poll() is None, 'daemon did not wait for CarParams'
        cp = car.CarParams.new_message(brand=scenario.brand, radarUnavailable=scenario.unavailable, radarDelay=0.1, extFlags=4096, flags=16)
        cp_bytes = cp.to_bytes()
        (case / 'CarParams.bin').write_bytes(cp_bytes)
        (params / 'CarParams').write_bytes(cp_bytes)
        wait_ready(publisher, child)
        subscriber = messaging.sub_sock('radarState', conflate=False, timeout=0)
        publisher.send('liveTracks', packet('liveTracks', 0))
        time.sleep(0.12)
        assert subscriber.receive(non_blocking=True) is None, 'published without model trigger'
        start = time.monotonic() + 0.05
        for index in range(40):
          time.sleep(max(0.0, start + index * 0.05 - time.monotonic()))
          for topic in SERVICES[1:]:
            if scenario.availability and (topic == 'livePose' or (topic == 'liveTracks' and 7 <= index < 23) or (topic == 'carState' and 26 <= index < 32)):
              continue
            publisher.send(topic, packet(topic, index))
          if scenario.availability and index == 36:
            time.sleep(0.12)
            assert subscriber.receive(non_blocking=True) is None, 'published during model absence'
          publisher.send('modelV2', packet('modelV2', index))
          deadline = time.monotonic() + 2.0
          raw = None
          while time.monotonic() < deadline:
            raw = subscriber.receive(non_blocking=True)
            if raw is not None:
              break
            if child.poll() is not None:
              raise RuntimeError(f'daemon exited at {index}: {child.returncode}')
            time.sleep(0.001)
          assert raw is not None, f'missing radarState at {index}'
          (case / f'{index:03}-radarState.bin').write_bytes(raw)
          with log.Event.from_bytes(raw) as event:
            rows.append({'valid': bool(event.valid), 'data': event.radarState.to_dict()})
        child.send_signal(signal.SIGTERM)
        assert child.wait(timeout=3) == -signal.SIGTERM, child.returncode
        del subscriber
      finally:
        if child.poll() is None:
          child.kill()
          child.wait(timeout=3)
    del publisher
    gc.collect()
  (case / 'records.json').write_text(json.dumps(rows, indent=2) + '\n')
  return rows


def main() -> None:
  paths, selected = arguments(tuple(SCENARIOS))
  paths.output.mkdir(parents=True)
  results = []
  for name in ([selected] if selected is not None else SCENARIOS):
    scenario = SCENARIOS[name]
    source, native = run('source', paths, scenario), run('rust', paths, scenario)
    diffs = differences(source, native)
    result = {'case': name, 'differences': len(diffs), 'first_differences': diffs[:10],
              'source_valid': [i for i, row in enumerate(source) if row['valid']],
              'rust_valid': [i for i, row in enumerate(native) if row['valid']]}
    results.append(result)
    (paths.output / 'receipt.json').write_text(json.dumps(results, indent=2) + '\n')
    print(json.dumps(result), flush=True)
    assert not diffs, result
    assert len(source) == len(native) == 40 and any(row['valid'] for row in native)
    if scenario.availability:
      assert not native[19]['valid'] and any(row['valid'] for row in native[32:]), 'missing input loss/recovery absent'
    else:
      assert not native[18]['valid'] and native[19]['valid'], 'invalid/recovery not observed'
  print('PASS actual native Params/msgq owner field comparisons', flush=True)


if __name__ == '__main__':
  main()
