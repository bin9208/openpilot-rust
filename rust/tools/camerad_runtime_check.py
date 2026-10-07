import argparse
from collections import Counter
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import struct
import subprocess
import time


SERVICES = ['wideRoadCameraState', 'roadCameraState', 'driverCameraState']
CASES = {
  'term': ({}, signal.SIGTERM, [0, 1, 2]),
  'int': ({}, signal.SIGINT, [0, 1, 2]),
  'pwr': ({}, signal.SIGPWR, [0, 1, 2]),
  'disabled-road': ({'DISABLE_ROAD': ''}, signal.SIGTERM, [0, 2]),
  'missing-road': ({'CK_MISSING_PORT': '1'}, signal.SIGTERM, [0, 2]),
  'wait-failure': ({'CK_FAIL_OP': 'sync:6', 'CK_FAIL_SKIP': '8'}, signal.SIGTERM, [0, 1, 2]),
  'poll-error': ({'CK_FAIL_OP': 'poll', 'CK_FAIL_SKIP': '6'}, None, [0, 1, 2]),
  'publication-order': ({}, signal.SIGTERM, [0, 1, 2]),
}


def main() -> None:
  parser = argparse.ArgumentParser()
  for name in ['binary', 'fixture', 'oracle', 'output']:
    parser.add_argument('--' + name, type=Path, required=True)
  parser.add_argument('--case', choices=CASES, required=True)
  parser.add_argument('--verify-only', action='store_true')
  parser.add_argument('--qemu', type=Path)
  parser.add_argument('--sysroot', type=Path)
  args = parser.parse_args()
  if args.verify_only:
    verify(args)
    return
  args.output.mkdir(parents=True, exist_ok=False)
  prefix = 'camera_runtime_' + str(os.getpid())
  os.environ['OPENPILOT_PREFIX'] = prefix
  namespace = Path('/dev/shm') / ('msgq_' + prefix)
  namespace.mkdir()
  from openpilot.cereal import log
  from openpilot.cereal.services import SERVICE_LIST
  import msgq
  from msgq.visionipc import VisionIpcClient, VisionStreamType
  import zmq

  overrides, stop_signal, ports = CASES[args.case]
  environment = {
    key: value
    for key, value in os.environ.items()
    if not key.startswith(('CK_', 'SPECTRA_', 'DISABLE_', 'DEBUG_FRAMES', 'LOG_RAW_FRAMES', 'CTRL_EXP_FROM_PARAMS'))
  }
  asan = '/usr/lib/llvm-18/lib/clang/18/lib/linux'
  environment.update(
    overrides, CK_SENSOR=str(0x5304), CK_TRACE=str(args.output / 'trace.jsonl'), CK_START_GATE=str(args.output / 'go'), PARAMS_ROOT=str(args.output / 'params')
  )
  if args.case == 'publication-order':
    environment['CK_REGISTER_GATE'] = str(args.output / 'register-go')
  if args.qemu:
    assert args.sysroot
    environment.pop('LD_PRELOAD', None)
    environment.pop('LD_LIBRARY_PATH', None)
  else:
    environment.update(
      LD_PRELOAD=f'{asan}/libclang_rt.asan-x86_64.so:{args.fixture}',
      LD_LIBRARY_PATH=asan,
      ASAN_OPTIONS='detect_leaks=0:abort_on_error=1',
      UBSAN_OPTIONS='halt_on_error=1:print_stacktrace=1',
    )
  params = args.output / 'params' / prefix
  params.mkdir(parents=True)
  (params / 'IsOffroad').write_text('1')
  subscribers = [msgq.sub_sock(service, conflate=False, segment_size=SERVICE_LIST[service].queue_size) for service in SERVICES]
  streams = [VisionStreamType.VISION_STREAM_WIDE_ROAD, VisionStreamType.VISION_STREAM_ROAD, VisionStreamType.VISION_STREAM_DRIVER]
  vision = {port: VisionIpcClient('camerad', streams[port], False) for port in ports}
  context = zmq.Context()
  logger = context.socket(zmq.PULL)
  logger.setsockopt(zmq.LINGER, 0)
  endpoint = 'ipc:///tmp/logmessage' + prefix
  logger.bind(endpoint)
  messages = [[] for _ in SERVICES]
  images = [[] for _ in SERVICES]
  logs = []
  command = [str(args.binary)]
  if args.qemu:
    command = [str(args.qemu), '-L', str(args.sysroot), '-E', f'LD_PRELOAD={args.fixture}', *command]
  process = None
  sent_signal = None
  failure = None

  def receive() -> None:
    for port, subscriber in enumerate(subscribers):
      while (raw := subscriber.receive(non_blocking=True)) is not None:
        with log.Event.from_bytes(raw) as message:
          assert message.which() == SERVICES[port], (port, message.which())
          messages[port].append({'wire': raw.hex(), 'event': message.to_dict()})
    for port, client in vision.items():
      if client.is_connected():
        while (frame := client.recv(0)) is not None:
          images[port].append(
            {
              'frame_id': client.frame_id,
              'buffer_frame_id': frame.frame_id,
              'sof': client.timestamp_sof,
              'eof': client.timestamp_eof,
              'slot': frame.idx,
              'width': frame.width,
              'height': frame.height,
              'stride': frame.stride,
              'length': len(frame.data),
              'samples': [frame.data[index] for index in (128, 1024, 8192)],
            }
          )
    while logger.poll(0, zmq.POLLIN):
      packet = logger.recv()
      record = json.loads(packet[1:])
      assert packet[0] == record['levelnum']
      logs.append(record)

  try:
    with (args.output / 'stdout').open('wb') as stdout, (args.output / 'stderr').open('wb') as stderr:
      process = subprocess.Popen(command, env=environment, stdout=stdout, stderr=stderr)
      deadline = time.monotonic() + 15
      while True:
        receive()
        trace = args.output / 'trace.jsonl'
        if trace.exists() and '"runtime-ready"' in trace.read_text():
          break
        assert process.poll() is None, ('startup exit', process.returncode)
        assert time.monotonic() < deadline, 'startup timeout'
        time.sleep(0.01)
      for port, client in vision.items():
        while not client.connect(False):
          assert time.monotonic() < deadline, ('VisionIPC connect timeout', port)
          time.sleep(0.01)
      (args.output / 'go').write_text('go')
      deadline = time.monotonic() + 10
      while process.poll() is None:
        receive()
        barrier = args.output / 'register-go'
        if args.case == 'publication-order' and not barrier.exists() and barrier.with_suffix('.ready').exists():
          receive()
          observation = {'messages': [len(values) for values in messages], 'vision': [len(values) for values in images]}
          (args.output / 'publication-order.json').write_text(json.dumps(observation, indent=2) + '\n')
          assert observation == {'messages': [0, 0, 0], 'vision': [1, 0, 0]}, observation
          barrier.write_text('release')
        if stop_signal is not None and sent_signal is None and all(len(messages[port]) >= 5 and len(images[port]) >= 5 for port in ports):
          process.send_signal(stop_signal)
          sent_signal = int(stop_signal)
        assert time.monotonic() < deadline, 'runtime timeout'
        time.sleep(0.002)
      receive()
      time.sleep(0.03)
      receive()
      assert process.returncode == 0, ('runtime exit', process.returncode)
  except BaseException as error:
    failure = repr(error)
    raise
  finally:
    if process is not None and process.poll() is None:
      process.kill()
      process.wait()
    for name, value in [('messages', messages), ('vision', images), ('logs', logs)]:
      (args.output / (name + '.json')).write_text(json.dumps(value, indent=2) + '\n')
    (args.output / 'run.json').write_text(
      json.dumps(
        {
          'command': command,
          'environment': {
            key: value for key, value in environment.items() if key.startswith(('CK_', 'LD_', 'ASAN_', 'UBSAN_', 'OPENPILOT_', 'PARAMS_', 'DISABLE_'))
          },
          'signal': sent_signal,
          'exit': process.returncode if process else None,
          'failure': failure,
        },
        indent=2,
      )
      + '\n'
    )
    logger.close()
    context.term()
    shutil.rmtree(namespace)
    Path('/tmp/logmessage' + prefix).unlink(missing_ok=True)
  verify(args)


def verify(args) -> None:
  from openpilot.cereal import log
  from camerad_camera_lifecycle_cleanup import audit

  _, _, ports = CASES[args.case]
  messages = json.loads((args.output / 'messages.json').read_text())
  images = json.loads((args.output / 'vision.json').read_text())
  logs = json.loads((args.output / 'logs.json').read_text())
  sent_signal = json.loads((args.output / 'run.json').read_text())['signal']

  records = [json.loads(line) for line in (args.output / 'trace.jsonl').read_text().splitlines()]
  cleanup = audit(records)
  (args.output / 'cleanup-audit.json').write_text(json.dumps(cleanup, indent=2) + '\n')
  stderr = (args.output / 'stderr').read_text()
  assert not any(error in stderr for error in ['ERROR: AddressSanitizer', 'runtime error:', 'Sanitizer CHECK failed'])
  assert all(not messages[port] and not images[port] for port in set(range(3)) - set(ports))
  events = [record for record in records if record['op'] == 'runtime-event']
  assert {row['session'] for row in events} == {10001 + port for port in ports}
  assert len([row for row in records if row['op'] == 'query']) == 2
  assert len([row for row in records if row['op'] == 'subscribe']) == 1
  assert any(record['msg'] == '-- Starting devices' and record['levelnum'] == 20 for record in logs)
  assert any(record['msg'] == '-- Dequeueing Video events' and record['levelnum'] == 20 for record in logs)
  if args.case not in ['missing-road']:
    for port in ports:
      assert any(record['msg'].startswith(f'camera {port} synced on frame_id_offset ') and record['levelnum'] == 30 for record in logs)
  if args.case == 'missing-road':
    assert any('first frame sync timed out:' in record['msg'] and record['levelnum'] == 40 for record in logs)
    assert min(row['round'] for row in records if row['op'] == 'runtime-registers') > 40
  elif args.case == 'wait-failure':
    assert any('sync failed after' in record['msg'] and record['levelnum'] == 40 for record in logs)
    assert any('clearing and requeuing' in record['msg'] and record['levelnum'] == 30 for record in logs)
  elif args.case == 'poll-error':
    assert any('poll failed' in record['msg'] and record['levelnum'] == 40 for record in logs)
  elif args.case == 'publication-order':
    assert json.loads((args.output / 'publication-order.json').read_text()) == {'messages': [0, 0, 0], 'vision': [1, 0, 0]}
    assert len([record for record in records if record['op'] == 'runtime-register-barrier-released']) == 1

  register_records = {(row['port'], row['round']): row for row in records if row['op'] == 'runtime-registers'}
  for port in ports:
    if not messages[port]:
      assert args.case == 'poll-error'
      continue
    assert len(messages[port]) == len(images[port]), (port, len(messages[port]), len(images[port]))
    assert [image['frame_id'] for image in images[port]] == [row['event'][SERVICES[port]]['frameId'] for row in messages[port]]
    layout = images[port][0]
    lines = [f'reset 3 {port} {layout["width"]} {layout["height"]} {8.0 if port == 1 else 1.71}']
    for index, row in enumerate(messages[port]):
      event = row['event']
      data = event[SERVICES[port]]
      image = images[port][index]
      assert event['valid'] is True
      assert image['buffer_frame_id'] == image['frame_id'] == data['frameId']
      assert image['sof'] == data['timestampSof'] and image['eof'] == data['timestampEof']
      assert image['samples'] == [64 + port * 32] * 3, (port, image['samples'])
      lines.append(
        'step {frameId} {requestId} {timestampSof} {timestampEof} {processingTime} {log_time} {seed} 0 0 1 "" ""'.format(
          **data, log_time=event['logMonoTime'], seed=64 + port * 32
        )
      )
    input_text = '\n'.join(lines) + '\n'
    (args.output / f'oracle-{port}.input').write_text(input_text)
    oracle_command = [str(args.oracle)]
    if args.qemu:
      oracle_command = [str(args.qemu), '-L', str(args.sysroot), *oracle_command]
    oracle = subprocess.run(
      oracle_command,
      input=input_text,
      text=True,
      capture_output=True,
      timeout=10,
      env={**os.environ, 'LD_LIBRARY_PATH': '/usr/lib/llvm-18/lib/clang/18/lib/linux'},
    )
    (args.output / f'oracle-{port}.stdout').write_text(oracle.stdout)
    (args.output / f'oracle-{port}.stderr').write_text(oracle.stderr)
    assert oracle.returncode == 0, oracle.stderr
    expected = [json.loads(line) for line in oracle.stdout.splitlines()]
    assert len(expected) == len(messages[port])
    for actual, wanted in zip(messages[port], expected, strict=True):
      with log.Event.from_bytes(bytes(wanted['wire'])) as reference:
        assert actual['event'] == reference.to_dict(), (port, actual['event'], reference.to_dict())
      data = actual['event'][SERVICES[port]]
      record = register_records[(port, data['requestId'])]
      payload = bytes.fromhex(record['payload'])
      writes = [list(values) for values in struct.iter_unpack('<II', payload)]
      assert writes == wanted['state']['writes'], (port, data['requestId'], writes, wanted['state']['writes'])
      assert wanted['actions'] == ['vision', 'registers', 'publish']
  close_counts = Counter(row['fd'] for row in records if row['op'] == 'close')
  for fd in [501, 503, 504, 505, 510, 511, 512]:
    assert close_counts[fd] == 1, ('device cleanup', fd, close_counts[fd])
  assert not any('panicked at' in line for line in stderr.splitlines())
  report = {
    'status': 'PASS',
    'case': args.case,
    'messages': [len(values) for values in messages],
    'vision': [len(values) for values in images],
    'logs': len(logs),
    'signal': sent_signal,
    'cleanup': cleanup,
    'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest(),
    'fixture_sha256': hashlib.sha256(args.fixture.read_bytes()).hexdigest(),
    'oracle_sha256': hashlib.sha256(args.oracle.read_bytes()).hexdigest(),
  }
  (args.output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
  print(json.dumps(report))


if __name__ == '__main__':
  main()
