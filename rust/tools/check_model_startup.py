# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy==2.5.3", "pycapnp==2.1.0", "zstandard==0.25.0"]
# ///
# PYTHONPATH=native-python:.:tinygrad_repo:rust/tools python rust/tools/check_model_startup.py --help
"""Compare original and native model initialization with all camera frames withheld."""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time

from msgq.visionipc import VisionIpcClient, VisionIpcServer, VisionStreamType
from openpilot.cereal import car
from openpilot.system.camerad.cameras.nv12_info import get_nv12_info
from check_driver_daemon import Lines, stop


def check(args, component: str, resolution: tuple[int, int], raw: bool) -> dict:
  destination = args.output / f'{component}-{resolution[0]}-raw-{int(raw)}'
  destination.mkdir()
  prefix = f'startup-{os.getpid()}-{resolution[0]}-{int(raw)}-{component}'
  shm = Path('/dev/shm') / f'msgq_{prefix}'
  shm.mkdir()
  params = destination / 'params' / prefix
  params.mkdir(parents=True)
  for key, value in {'VEgoStopping': b'5', 'CameraYawTrimDeg': b'35', 'UseWideCamera': b'1', 'SteerActuatorDelay': b'0',
                     'LatSmoothSec': b'0', 'LongActuatorDelay': b'30'}.items():
    (params / key).write_bytes(value)
  environment = dict(os.environ, OPENPILOT_PREFIX=prefix, PARAMS_ROOT=str(params.parent), LOGPRINT='info')
  environment.pop('SEND_RAW_PRED', None)
  if raw:
    environment['SEND_RAW_PRED'] = '0'
  stride, y_height, _, size = get_nv12_info(*resolution)
  previous = os.environ.get('OPENPILOT_PREFIX')
  os.environ['OPENPILOT_PREFIX'] = prefix
  server = VisionIpcServer('camerad')
  streams = [VisionStreamType.VISION_STREAM_DRIVER] if component == 'dmonitoringmodeld' else [
    VisionStreamType.VISION_STREAM_ROAD, VisionStreamType.VISION_STREAM_WIDE_ROAD]
  for stream in streams:
    server.create_buffers_with_sizes(stream, 4, *resolution, size, stride, stride * y_height)
  server.start_listener()
  deadline = time.monotonic() + 10
  while set(VisionIpcClient.available_streams('camerad', False)) != set(streams):
    assert time.monotonic() < deadline
    time.sleep(.01)
  processes = []
  try:
    source_command = [sys.executable, str(Path(__file__).with_name('model_startup_reference.py')), '--component', component,
                      '--models', str(args.models)]
    source = subprocess.Popen(source_command, env=environment | {'DEV': 'CPU'}, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                              stderr=subprocess.STDOUT, bufsize=0)
    processes.append(source)
    source_lines = Lines(source.stdout, destination / 'source.log')
    initialized = json.loads(source_lines.until(('"event": "initialized"',)))
    assert (initialized['width'], initialized['height'], initialized['buffer_len']) == (*resolution, size)
    assert not (params / 'CarParams').exists()
    source.stdin.write(b'receive\n')
    source.stdin.flush()
    for stream in streams:
      server.send(stream, bytes(size), 1, 50_000_000, 50_001_000)
    received = json.loads(source_lines.until(('"event": "first_frame"',)))
    assert received['frame_id'] == 1 and source.wait(timeout=5) == 0
    del server
    server = VisionIpcServer('camerad')
    for stream in streams:
      server.create_buffers_with_sizes(stream, 4, *resolution, size, stride, stride * y_height)
    server.start_listener()
    binary = args.driving if component == 'modeld' else args.driver
    command = [str(binary), '--trusted-catalog', str(args.catalog)]
    native = subprocess.Popen(command, env=environment, stderr=subprocess.PIPE, bufsize=0)
    processes.append(native)
    lines = Lines(native.stderr, destination / 'native.log')
    lines.until(('connected extra cam' if component == 'modeld' else 'connected with buffer size',))
    try:
      loaded = lines.until(('models loaded',), timeout=10)
    except TimeoutError:
      assert native.poll() is None
      loaded = None
    before_params = loaded is not None
    if component == 'modeld':
      (params / 'CarParams').write_bytes(car.CarParams.new_message(longitudinalActuatorDelay=0.).to_bytes())
      lines.until(('modeld got CarParams',))
    if loaded is None:
      try:
        loaded = lines.until(('models loaded',), timeout=1)
      except TimeoutError:
        assert native.poll() is None
    report = {'component': component, 'resolution': resolution, 'raw': raw, 'source': initialized,
              'source_first_frame': received, 'native_loaded_before_car_params': before_params,
              'native_loaded_without_frame': loaded is not None, 'commands': {'source': source_command, 'native': command},
              'expected': args.expect, 'native_exit_before_stop': native.poll()}
    (destination / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    expected = args.expect == 'pre-frame'
    assert before_params == expected and (loaded is not None) == expected, report
    stop(native)
    assert native.returncode == 0
    print(json.dumps(report), flush=True)
    return report
  finally:
    for process in reversed(processes):
      stop(process)
    del server
    if previous is None:
      del os.environ['OPENPILOT_PREFIX']
    else:
      os.environ['OPENPILOT_PREFIX'] = previous
    shutil.rmtree(shm)
    for path in Path('/tmp').glob(f'{prefix}*'):
      if path.is_socket():
        path.unlink()


if __name__ == '__main__':
  parser = argparse.ArgumentParser(description=__doc__)
  for name in ['driving', 'driver', 'models', 'catalog', 'output']:
    parser.add_argument('--' + name, type=Path, required=True)
  parser.add_argument('--expect', choices=['lazy', 'pre-frame'], default='pre-frame')
  args = parser.parse_args()
  for name in ['driving', 'driver', 'models', 'catalog', 'output']:
    setattr(args, name, getattr(args, name).resolve())
  args.output.mkdir(parents=True)
  reports = [check(args, component, resolution, raw) for component in ['modeld', 'dmonitoringmodeld']
             for resolution in [(1344, 760), (1928, 1208)] for raw in [False, True]]
  (args.output / 'report.json').write_text(json.dumps({'runs': reports, 'device_validation': False}, indent=2) + '\n')
