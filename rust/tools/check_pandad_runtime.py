import argparse
from hashlib import sha256
import json
from pathlib import Path
import resource
import signal
import subprocess
import time

from pandad_runtime_peer import RuntimePeer, packet


def control(trace, request):
  return [row for row in trace if row['op'] == 'control' and row['kind'] == 0x40 and row['request'] == request]


def exercise(args, name, *, onroad=False, count=1, flags=(), action=None):
  peer = RuntimePeer(args.binary, args.collector, args.fixture, args.output / name,
                     onroad=onroad, count=count, flags=flags, runner=args.runner)
  try:
    peer.start()
    peer.ready()
    if action == 'hotplug':
      peer.config['count'] = count + 1
      peer.store_config()
      assert peer.finish(None) == 0
    elif action == 'disconnect':
      peer.config['disconnect_after'] = 20
      peer.store_config()
      assert peer.finish(None) == 0
    else:
      for index in range(12):
        peer.inputs(index)
        peer.poll()
        time.sleep(.05)
      peer.send(0x401)
      peer.send(0x402, timestamp=time.monotonic_ns() + 10_000_000_000)
      peer.send(0x403, timestamp=1)
      for address in (0x411, 0x412, 0x413):
        peer.send(address)
      for index in range(25):
        peer.inputs(12 + index, enabled=False, camera=False)
        peer.poll()
        time.sleep(.05)
      assert peer.finish(signal.SIGTERM if name == 'no-fan' else signal.SIGINT) == 0
    trace = peer.trace()
    for service in ('can', 'pandaStates', 'peripheralState'):
      assert peer.messages[service], (name, service)
      assert all(event['valid'] for event in peer.messages[service]), (name, service)
    received = [frame for event in peer.messages['can'] for frame in event['can']]
    assert any(frame['address'] == 0x321 and frame['dat'] == b'\x04\x05' for frame in received)
    assert {frame['src'] for frame in received} == {index * 4 for index in range(count)}
    if action is None:
      writes = [row for row in trace if row['op'] == 'write']
      expected = [list(packet(address, 0, [0x12, 0x34])) for address in (0x401, 0x402, 0x411, 0x412, 0x413)]
      assert [row['data'] for row in writes] == ([] if 'FAKESEND' in flags else expected), (name, writes)
      assert all(row['device'] == 0 and row['endpoint'] == 3 and row['timeout'] == 5 for row in writes)
      fan = control(trace, 0xb1)
      assert (not fan) if 'NO_FAN_CONTROL' in flags else any(row['value'] == 51 for row in fan)
      ir = control(trace, 0xb0)
      assert any(row['value'] == 50 for row in ir) and ir[-1]['value'] == 0
      heartbeat = control(trace, 0xf3)
      assert any(row['value'] == 1 for row in heartbeat) and heartbeat[-1]['value'] == 0
      if onroad:
        safety = control(trace, 0xdc)
        assert any(row['index'] == 42 for row in safety)
        for index in range(count):
          assert [row for row in safety if row['device'] == index][-1]['value'] == 19
        assert all(row['value'] == 7 for row in control(trace, 0xdf))
    snapshots = [event['pandaStates'] for event in peer.messages['pandaStates']]
    if count == 2:
      assert all(not rows[0]['ignitionLine'] and rows[1]['ignitionLine'] for rows in snapshots)
    assert all(row['peripheralState']['fanSpeedRpm'] == 1234 for row in peer.messages['peripheralState'])
    logs = [event['logMessage'] for event in peer.messages['logMessage']]
    assert any(row['filename'] == 'panda[0]' and row['msg'] == 'SPI: fixture serial' for row in logs)
    timestamps = [row for row in logs if isinstance(row.get('msg'), dict) and 'timestamp' in row['msg']]
    if 'LOG_TIMESTAMPS' in flags:
      assert len(timestamps) == 10, (name, timestamps)
    else:
      assert not timestamps
    if action == 'hotplug':
      assert any(str(row['msg']).startswith('Reconnecting to new panda:') for row in logs)
    assert all('libpython' not in line for line in (peer.output / 'maps.txt').read_text().splitlines())
    report = {'result': 'PASS', 'scenario': name, 'messages': {key: len(values) for key, values in peer.messages.items()},
              'usb_calls': len(trace), 'source_frames': len(received), 'native_only': True,
              'implementation': 'original-cpp' if args.original else 'rust'}
    (peer.output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report), flush=True)
    return report
  finally:
    peer.close()


def firmware_failure(args):
  peer = RuntimePeer(args.binary, args.collector, args.fixture, args.output / 'firmware-mismatch', skip_firmware=False, runner=args.runner)
  try:
    peer.start()
    exit_code = peer.finish(None)
    assert exit_code != 0 if args.original else exit_code == 1
    assert 'Panda firmware out of date' in (peer.output / 'stderr.log').read_text()
    trace = peer.trace()
    assert [row['request'] for row in trace if row['op'] == 'control'] == [0xc1, 0xc0, 0xe8, 0xe8, 0xe8, 0xd3, 0xd4]
    assert not any(row['op'] in ('read', 'write') for row in trace)
    return {'result': 'PASS', 'scenario': 'firmware-mismatch', 'exit': exit_code}
  finally:
    peer.close()


def malformed_params(args):
  peer = RuntimePeer(args.binary, args.collector, args.fixture, args.output / 'malformed-params', onroad=True, runner=args.runner)
  (peer.params / 'CarParams').write_bytes(b'invalid capnp')
  try:
    peer.start()
    exit_code = peer.finish(None)
    assert exit_code != 0 if args.original else exit_code == 1
    trace = peer.trace()
    safety = control(trace, 0xdc)
    assert [row['value'] for row in safety] == [19, 3], safety
    assert not any(row['op'] == 'write' for row in trace)
    return {'result': 'PASS', 'scenario': 'malformed-params', 'exit': exit_code,
            'safety_controls': safety}
  finally:
    peer.close()


def main():
  parser = argparse.ArgumentParser(description='Exercise native pandad with owned USB ABI, original msgq/Cereal and native log collector.')
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--collector', type=Path, required=True)
  parser.add_argument('--fixture', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--case')
  parser.add_argument('--original', action='store_true')
  parser.add_argument('--runner', action='append', default=[])
  args = parser.parse_args()
  for key in ('binary', 'collector', 'fixture', 'output'):
    setattr(args, key, getattr(args, key).resolve())
  args.output.mkdir(parents=True, exist_ok=False)
  resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
  cases = [
    ('offroad', {}), ('onroad', {'onroad': True, 'flags': ('LOG_TIMESTAMPS',)}),
    ('c3-pair', {'onroad': True, 'count': 2}), ('fake-send', {'flags': ('FAKESEND',)}),
    ('no-fan', {'flags': ('NO_FAN_CONTROL',)}), ('hotplug', {'action': 'hotplug'}),
    ('disconnect', {'action': 'disconnect'}),
  ]
  results = []
  for name, options in cases:
    if args.case is None or args.case == name:
      results.append(exercise(args, name, **options))
  if args.case is None or args.case == 'firmware-mismatch':
    results.append(firmware_failure(args))
  if args.case is None or args.case == 'malformed-params':
    results.append(malformed_params(args))
  for options, expected in [(['--help'], 0), (['--frames', '0'], 1), (['--unknown'], 1)]:
    if args.original:
      break
    process = subprocess.run([*args.runner, str(args.binary), *options], capture_output=True, text=True, timeout=5)
    assert process.returncode == expected
  manifest = {'result': 'PASS', 'scenarios': results,
              'binary_sha256': sha256(args.binary.read_bytes()).hexdigest(),
              'fixture_sha256': sha256(args.fixture.read_bytes()).hexdigest(),
              'collector_sha256': sha256(args.collector.read_bytes()).hexdigest(),
              'implementation': 'original-cpp' if args.original else 'rust',
              'runner': args.runner,
              'limits': 'host integration with owned transport; scenario assertions do not establish physical device behavior or CPU savings'}
  (args.output / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
  print(json.dumps(manifest))


if __name__ == '__main__':
  main()
