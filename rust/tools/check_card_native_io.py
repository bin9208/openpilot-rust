import argparse
import ast
from contextlib import ExitStack
import hashlib
import json
import os
from pathlib import Path
import select
import shutil
import subprocess
import time
import uuid

import zmq
from openpilot.cereal import log, messaging
from card_vehicle_source import normalize
from can_source import ROOT
from check_card_vehicle import compare

SERVICES = ('pandaStates', 'carControl', 'onroadEvents', 'carrotMan', 'longitudinalPlan',
            'radarState', 'modelV2', 'drivingModelData', 'customReservedRawData0')


def receive(process: subprocess.Popen, stream: Path) -> dict:
  deadline = time.monotonic() + 5
  while time.monotonic() < deadline:
    assert process.stdout is not None
    assert select.select([process.stdout], [], [], max(0., deadline - time.monotonic()))[0], 'native card IO timed out'
    line = process.stdout.readline()
    with stream.open('a') as output:
      output.write(line)
    assert line, ('native card IO exited', process.poll())
    if line.startswith('CARD_TRACE '):
      return json.loads(line.removeprefix('CARD_TRACE '))
  raise TimeoutError('native card IO trace missing')


def command(process: subprocess.Popen, request: dict, stream: Path) -> dict:
  assert process.stdin is not None
  process.stdin.write(json.dumps(request) + '\n')
  process.stdin.flush()
  return receive(process, stream)


def packet(frames: list[dict], timestamp: int) -> bytes:
  event = messaging.new_message('can', len(frames))
  event.logMonoTime = timestamp
  event.valid = False
  for message, frame in zip(event.can, frames, strict=True):
    message.address = frame['address']
    message.dat = bytes(frame['data'])
    message.src = frame['bus']
  return event.to_bytes()


def source_callbacks():
  from openpilot.selfdrive.pandad import can_list_to_can_capnp
  path = ROOT / 'openpilot/selfdrive/car/card.py'
  tree = ast.parse(path.read_text())
  functions = [item for item in tree.body if isinstance(item, ast.FunctionDef) and item.name == 'can_comm_callbacks']
  assert len(functions) == 1
  functions[0].returns = None
  for argument in functions[0].args.args:
    argument.annotation = None
  scope = dict(messaging=messaging, CanData=__import__('opendbc.car.can_definitions', fromlist=['CanData']).CanData,
               can_list_to_can_capnp=can_list_to_can_capnp)
  exec(compile(ast.Module(body=functions, type_ignores=[]), str(path), 'exec'), scope)
  return scope['can_comm_callbacks']


def source_snapshot(sm) -> dict:
  topics = []
  for name in SERVICES:
    if name == 'customReservedRawData0':
      payload = list(bytes(sm[name]))
    elif name in ('pandaStates', 'onroadEvents'):
      payload = [value.to_dict() for value in sm[name]]
    else:
      payload = sm[name].to_dict()
    topics.append(dict(name=name, seen=sm.seen[name], updated=sm.updated[name], alive=sm.alive[name],
        valid=sm.valid[name], frame=sm.recv_frame[name], mono=sm.logMonoTime[name], data=normalize(payload)))
  return dict(frame=sm.frame, topics=topics)


def native_snapshot(value: dict) -> dict:
  for topic in value['topics']:
    with log.Event.from_bytes(bytes(topic.pop('event'))) as event:
      payload = event.to_dict()[topic['name']]
      topic['data'] = list(payload) if isinstance(payload, bytes) else normalize(payload)
  return value


def exercise(args) -> dict:
  output = args.evidence
  output.mkdir(parents=True, exist_ok=True)
  prefix = 'card177_' + uuid.uuid4().hex
  params_root = output / 'params'
  params = params_root / prefix
  params.mkdir(parents=True)
  os.environ['OPENPILOT_PREFIX'] = prefix
  stderr_path = output / 'stderr.log'
  stdout_path = output / 'stdout.log'
  transcript = []
  with ExitStack() as stack:
    queue_root = Path('/dev/shm') / ('msgq_' + prefix)
    queue_root.mkdir()
    stack.callback(shutil.rmtree, queue_root)
    context = stack.enter_context(zmq.Context())
    logs = stack.enter_context(context.socket(zmq.PULL))
    logs.setsockopt(zmq.LINGER, 0)
    logs.bind('ipc:///tmp/logmessage' + prefix)
    publishers = {name: messaging.pub_sock(name) for name in ('can', *SERVICES)}
    source_sm = messaging.SubMaster(list(SERVICES))
    source_can = messaging.sub_sock('can', timeout=20)
    sendcan = messaging.sub_sock('sendcan', timeout=100)
    captured = []
    source_receive, source_send = source_callbacks()(source_can, type('Send', (), {'send': lambda self, data: captured.append(data)})())
    environment = dict(os.environ, PARAMS_ROOT=str(params_root.resolve()), SIMULATION='0')
    for flag in ('REPLAY', 'FAKESEND', 'FINGERPRINT', 'SKIP_FW_QUERY', 'DISABLE_FW_CACHE'):
      environment.pop(flag, None)
    stderr = stack.enter_context(stderr_path.open('w'))
    process = stack.enter_context(subprocess.Popen([str(args.binary.resolve())], env=environment,
        stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=stderr, text=True, bufsize=1))
    try:
      assert receive(process, stdout_path) == {'ready': True}
      assert sendcan.receive(non_blocking=True) is None
      maps = Path(f'/proc/{process.pid}/maps').read_text()
      (output / 'maps.txt').write_text(maps)
      assert 'libpython' not in maps
      for publisher in publishers.values():
        publisher.wait_for_readers(timeout=5)
      assert process.stdin is not None
      process.stdin.write('{"op":"startup"}\n')
      process.stdin.flush()
      publishers['can'].send(packet([], 1000))
      publishers['can'].send(packet([dict(address=123, data=[1], bus=4)], 2000))
      pandas = messaging.new_message('pandaStates', 2)
      publishers['pandaStates'].send(pandas.to_bytes())
      while not messaging.recv_one_retry(source_can).can:
        pass
      count = len(messaging.recv_one_retry(source_sm.sock['pandaStates']).pandaStates)
      expected = dict(pandas=count, frame=source_sm.frame)
      actual = receive(process, stdout_path)
      compare(expected, actual)
      transcript.append(dict(operation='initial_nonempty_can_discard_and_panda_read', source=expected, native=actual))
      batches = [[], [dict(address=321, data=list(range(64)), bus=0)], [dict(address=456, data=[9, 8], bus=7)]]
      for index, frames in enumerate(batches):
        publishers['can'].send(packet(frames, 3000 + index))
      expected = {'packets': [[dict(address=frame.address, data=list(frame.dat), bus=frame.src) for frame in batch] for batch in source_receive(False)]}
      actual = command(process, {'op': 'receive', 'wait': False}, stdout_path)
      compare(expected, actual)
      transcript.append(dict(operation='nonconflated_can_batches_with_empty_packet_and_fd', source=expected, native=actual))
      for index in range(4):
        for name in SERVICES:
          event = messaging.new_message(name, 0 if name in ('pandaStates', 'onroadEvents', 'customReservedRawData0') else None)
          event.logMonoTime = 100_000 + index
          event.valid = name != 'customReservedRawData0'
          if name == 'carControl':
            event.carControl.enabled = index % 2 == 0
            publishers[name].send(event.to_bytes())
            event.carControl.enabled = index % 2 != 0
            event.clear_write_flag()
          publishers[name].send(event.to_bytes())
        actual = native_snapshot(command(process, {'op': 'update'}, stdout_path))
        source_sm.update(0)
        expected = source_snapshot(source_sm)
        compare(expected, actual)
        transcript.append(dict(operation='conflated_source_submaster_update', source=expected, native=actual))
      frames = [dict(address=0x7d0, data=[2, 0x10, 3, 0, 0, 0, 0, 0], bus=4)]
      assert command(process, dict(op='send', frames=frames), stdout_path) == {'sent': 1}
      wire = sendcan.receive()
      assert wire is not None
      source_send([__import__('opendbc.car.can_definitions', fromlist=['CanData']).CanData(frame['address'], bytes(frame['data']), frame['bus']) for frame in frames])
      with log.Event.from_bytes(wire) as native, log.Event.from_bytes(captured[0]) as source:
        left = source.to_dict()
        right = native.to_dict()
        left.pop('logMonoTime')
        right.pop('logMonoTime')
        for value in (left, right):
          for frame in value['sendcan']:
            frame['dat'] = list(frame['dat'])
        compare(left, right)
        assert native.logMonoTime > 0
      (output / 'sendcan.bin').write_bytes(wire)
      transcript.append(dict(operation='diagnostic_sendcan', source=normalize(left), native=normalize(right)))
      for enabled in (True, False):
        (params / 'ObdMultiplexingChanged').write_bytes(b'1')
        process.stdin.write(json.dumps(dict(op='obd', enabled=enabled)) + '\n')
        process.stdin.flush()
        deadline = time.monotonic() + 5
        while time.monotonic() < deadline:
          value = (params / 'ObdMultiplexingEnabled').read_bytes() if (params / 'ObdMultiplexingEnabled').exists() else b'0'
          if value == (b'1' if enabled else b'0') and not (params / 'ObdMultiplexingChanged').exists():
            break
          time.sleep(.002)
        else:
          raise TimeoutError('OBD handshake write missing')
        (params / 'ObdMultiplexingChanged').write_bytes(b'')
        assert not select.select([process.stdout], [], [], .05)[0], 'empty OBD acknowledgment incorrectly unblocked'
        (params / 'ObdMultiplexingChanged').write_bytes(b'0')
        actual = receive(process, stdout_path)
        compare(dict(obd=enabled), actual)
        transcript.append(dict(operation='obd_ack_nonempty_even_false', enabled=enabled, changed_missing_before_ack=True, empty_ack_blocked=True, native=actual))
      warnings = []
      deadline = time.monotonic() + 5
      while len(warnings) < 4 and time.monotonic() < deadline:
        if logs.poll(100):
          wire = logs.recv()
          assert wire[0] == 30
          warnings.append(json.loads(wire[1:])['msg'])
      compare(['Setting OBD multiplexing to True', 'OBD multiplexing set successfully',
               'Setting OBD multiplexing to False', 'OBD multiplexing set successfully'], warnings)
      (output / 'warnings.json').write_text(json.dumps(warnings) + '\n')
      process.stdin.write('{"op":"quit"}\n')
      process.stdin.flush()
      assert process.wait(timeout=5) == 0
    finally:
      (output / 'transcript.json').write_text(json.dumps(transcript, indent=2) + '\n')
      if process.poll() is None:
        process.terminate()
        process.wait(timeout=5)
  (output / 'transcript.json').write_text(json.dumps(transcript, indent=2) + '\n')
  return dict(status='pass', operations=len(transcript), runtime_python=False,
      binary_sha256=hashlib.sha256(args.binary.read_bytes()).hexdigest(),
      observable='original msgq/Cereal startup read, nonconflated CAN packets, conflated subscriber state, diagnostic CAN and nonempty OBD acknowledgment/logs',
      scope='real host IPC adapter; logMonoTime clocks are observed positive and excluded from cross-process byte comparison; no vehicle transport',
      source_sha256={str(path.relative_to(ROOT)): hashlib.sha256(path.read_bytes()).hexdigest() for path in
          (ROOT / 'openpilot/selfdrive/car/card.py', ROOT / 'openpilot/cereal/messaging/__init__.py')})


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--evidence', type=Path, required=True)
  args = parser.parse_args()
  result = exercise(args)
  (args.evidence / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
  print(json.dumps(result))


if __name__ == '__main__':
  main()
