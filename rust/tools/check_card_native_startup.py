import argparse
from contextlib import ExitStack, redirect_stdout
import hashlib
import io
import json
import os
from pathlib import Path
import select
import shutil
import subprocess
import time
import uuid

from openpilot.cereal import car, log, messaging
from can_source import ROOT, load
from card_vehicle_source import normalize
from check_card_startup_params import source_prepare, decoded
from check_card_vehicle import compare


class Settings:
  def __init__(self, values: dict[str, bytes]) -> None:
    self.values = values

  def get(self, key: str):
    value = self.values.get(key)
    if value in (None, b''):
      return None
    return value.decode() if key in ('FingerPrints', 'SecOCKey', 'NNFFModelName', 'CarSelected3') else value

  def get_bool(self, key: str) -> bool:
    return self.values.get(key) == b'1'

  def get_int(self, key: str) -> int:
    return int(self.values.get(key) or b'0')

  def get_float(self, key: str) -> float:
    return float(self.values.get(key) or b'0')

  def put_int(self, key: str, value: int) -> None:
    self.values[key] = str(value).encode()

  def put_bool(self, key: str, value: bool) -> None:
    self.values[key] = b'1' if value else b'0'

  def put(self, key: str, value: str | bytes) -> None:
    self.values[key] = value.encode() if isinstance(value, str) else value

  put_nonblocking = put


def source(candidate: str, identification: dict, values: dict[str, bytes]) -> dict:
  load()
  from opendbc.car import interfaces
  brand = {'COMMA_BODY': 'body', 'MOCK': 'mock', 'GENESIS_G70': 'hyundai'}[candidate]
  interface = __import__(f'opendbc.car.{brand}.interface', fromlist=['CarInterface']).CarInterface
  settings = Settings(values.copy())
  interfaces.Params = lambda: settings
  if candidate not in ('MOCK', 'COMMA_BODY'):
    from opendbc.car.hyundai import carstate, carcontroller
    for module in (carstate, carcontroller, __import__('opendbc.car.hyundai.interface', fromlist=['Params'])):
      module.Params = lambda *args: settings
  fingerprints = {bus: dict(entries) for bus, entries in identification['observed']}
  settings.put('FingerPrints', repr(fingerprints))
  cp = interface.get_params(candidate, fingerprints, [], False, True, False)
  cp.carVin = identification['vin']
  cp.carFw = []
  cp.fingerprintSource = 'fixed'
  cp.fuzzyFingerprint = False
  vehicle = interface(cp)
  assert vehicle.CC is not None
  case = dict(params=list(cp.to_bytes()), identification=identification,
      settings={key: list(value) for key, value in settings.values.items()}, controller=True, user_key=None)
  prepared = decoded(source_prepare(case))
  return dict(params=prepared['output']['params'], maximum=list(b'3'), frame=-1, error=False,
              name=prepared['settings']['CarName'], fingerprints=prepared['settings']['FingerPrints'])


def wait_line(process: subprocess.Popen, output: Path, expected: str) -> None:
  deadline = time.monotonic() + 10
  assert process.stdout is not None
  while time.monotonic() < deadline:
    assert select.select([process.stdout], [], [], max(0., deadline - time.monotonic()))[0], ('startup trace timed out', expected)
    line = process.stdout.readline().decode()
    with output.open('a') as stream:
      stream.write(line)
    assert line, ('startup process exited', process.poll())
    if line.strip() == expected:
      return
  raise TimeoutError(expected)


def exercise(args, candidate: str, bad_assets: bool) -> dict:
  output = args.evidence / (candidate + ('-bad-assets' if bad_assets else ''))
  output.mkdir(parents=True)
  prefix = 'card177_' + uuid.uuid4().hex
  os.environ['OPENPILOT_PREFIX'] = prefix
  params = output / 'params' / prefix
  params.mkdir(parents=True)
  values = {'OpenpilotEnabledToggle': b'1', 'NNFF': b'1', 'NNFFLite': b'1', 'CarParamsCache': b'',
            'CarParamsPersistent': b'previous route', 'IsMetric': b'1'}
  for key, value in values.items():
    (params / key).write_bytes(value)
  assets = output / 'missing-assets' if bad_assets else ROOT / 'opendbc_repo/opendbc/car/torque_data'
  command = [str(args.binary.resolve()), str((output / 'native.json').resolve()), str(ROOT / 'opendbc_repo/opendbc/dbc'), str(assets.resolve()), str(args.numerics)]
  with ExitStack() as stack:
    queue_root = Path('/dev/shm') / ('msgq_' + prefix)
    queue_root.mkdir()
    stack.callback(shutil.rmtree, queue_root)
    can = messaging.pub_sock('can')
    pandas = messaging.pub_sock('pandaStates')
    environment = dict(os.environ, PARAMS_ROOT=str(params.parent.resolve()), FINGERPRINT=candidate, OPENBLAS_NUM_THREADS='1')
    for flag in ('REPLAY', 'FAKESEND', 'SKIP_FW_QUERY', 'DISABLE_FW_CACHE'):
      environment.pop(flag, None)
    stderr = stack.enter_context((output / 'stderr.log').open('w'))
    process = stack.enter_context(subprocess.Popen(command, env=environment, stdin=subprocess.PIPE,
        stdout=subprocess.PIPE, stderr=stderr, bufsize=0))
    try:
      wait_line(process, output / 'stdout.log', 'CARD_STARTUP_READY')
      can.wait_for_readers(timeout=5)
      pandas.wait_for_readers(timeout=5)
      state = messaging.new_message('pandaStates', 1)
      pandas.send(state.to_bytes())
      event = messaging.new_message('can', 1)
      event.can[0].address = 0x123
      event.can[0].src = 0
      event.can[0].dat = b'\0' * 8
      started = time.monotonic()
      for index in range(240):
        event.logMonoTime = 1_000_000_000 + index * 10_000_000
        event.clear_write_flag()
        can.send(event.to_bytes())
        time.sleep(.004)
      wait_line(process, output / 'stdout.log', 'CARD_STARTUP_DONE')
      native = json.loads((output / 'native.json').read_text())
      (output / 'maps.txt').write_text(Path(f'/proc/{process.pid}/maps').read_text())
      assert 'libpython' not in (output / 'maps.txt').read_text()
      if bad_assets:
        assert native['error']
        assert not (params / 'FirmwareQueryDone').exists()
        assert not (params / 'CarParams').exists()
        observable = dict(error_before_firmware_done=True, identity_saved=(params / 'CarName').read_text())
      else:
        assert not native['error'], native
        assert native['identification']['candidate'] == candidate
        assert native['identification']['source'] == 'fixed'
        assert native['identification']['observed'] == [[bus, [[0x123, 8]] if bus == 0 else []] for bus in range(8)]
        with car.CarParams.from_bytes(bytes(native['params'])) as cp:
          cp_value = normalize(cp.to_dict())
        observed = dict(params=cp_value, maximum=native['maximum'], frame=native['frame'], error=False,
                        name=list((params / 'CarName').read_bytes()), fingerprints=list((params / 'FingerPrints').read_bytes()))
        source_log = io.StringIO()
        with redirect_stdout(source_log):
          expected = source(candidate, native['identification'], values)
        (output / 'source.log').write_text(source_log.getvalue())
        (output / 'source.json').write_text(json.dumps(expected) + '\n')
        (output / 'native-decoded.json').write_text(json.dumps(observed) + '\n')
        compare(expected, observed)
        assert (params / 'FirmwareQueryDone').read_bytes() == b'1'
        assert (params / 'CarParamsPrevRoute').read_bytes() == b'previous route'
        stored = (params / 'CarParams').read_bytes()
        deadline = time.monotonic() + 5
        while any(not (params / key).exists() or (params / key).read_bytes() != stored
                  for key in ('CarParamsCache', 'CarParamsPersistent')):
          assert time.monotonic() < deadline, 'asynchronous Params writes did not drain'
          time.sleep(.005)
        observable = dict(candidate=candidate, packets=native['identification']['packets'], initialized_source_frame=-1,
                          full_params_match=True, storage_route_handoff=True, model=native['model'])
      assert process.stdin is not None
      process.stdin.write(b'q')
      process.stdin.flush()
      assert process.wait(timeout=5) == 0
      return dict(observable=observable, elapsed_host_seconds=time.monotonic() - started, command=command)
    finally:
      if process.poll() is None:
        process.terminate()
        process.wait(timeout=5)


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--numerics', type=Path, required=True)
  parser.add_argument('--evidence', type=Path, required=True)
  args = parser.parse_args()
  args.evidence.mkdir(parents=True, exist_ok=True)
  rows = [exercise(args, candidate, False) for candidate in ('COMMA_BODY', 'MOCK', 'GENESIS_G70')]
  rows.append(exercise(args, 'COMMA_BODY', True))
  result = dict(status='pass', cases=len(rows), runtime_python=False, scenarios=rows,
      binary_sha256=hashlib.sha256(args.binary.read_bytes()).hexdigest(),
      observable='live CAN/panda startup, native brand and common construction, complete source CarParams and route storage, early asset failure',
      scope='owned original msgq/Cereal host peers; original brand construction and constructor segment; Cruise tail/continuous CLI remain separate')
  (args.evidence / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
  print(json.dumps(result))


if __name__ == '__main__':
  main()
