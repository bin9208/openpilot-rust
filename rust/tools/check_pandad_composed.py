import argparse
from hashlib import sha256
import json
import os
from pathlib import Path
import signal
import subprocess
import time

from pandad_runtime_peer import RuntimePeer, packet

ROOT = Path(__file__).resolve().parents[2]


class ComposedPeer(RuntimePeer):
  def __init__(self, args, output, *, onroad, count):
    super().__init__(args.core, args.collector, args.fixture, output, onroad=onroad, count=count)
    self.args = args
    self.config['supervisor'] = True
    self.store_config()
    self.root = output / 'root'
    (self.root / 'data/params').mkdir(parents=True)
    (self.root / 'data/params/d').symlink_to(self.params.resolve(), target_is_directory=True)
    self.basedir = output / 'basedir' if args.default_firmware else ROOT
    if args.default_firmware:
      (self.basedir / 'openpilot/selfdrive/pandad').mkdir(parents=True)
    self.firmware = self.basedir / 'panda/board/obj' if args.default_firmware else output / 'firmware'
    self.firmware.mkdir(parents=True)
    for name in ('panda.bin.signed', 'panda_h7.bin.signed'):
      (self.firmware / name).write_bytes(bytes([0x42]) * 128)
    self.children = []

  def launch(self, command, environment, cwd, stdout, stderr):
    del command, cwd
    environment['PANDA_FIRMWARE_USB_LIBRARY'] = str(self.fixture)
    descriptor = self.output / 'owned-descriptor'
    descriptor.write_bytes(b'owned non-CLOEXEC descriptor')
    environment['PANDA_OWNED_DESCRIPTOR'] = str(descriptor)
    command = [*self.args.runner, str(self.args.supervisor), str(self.root), str(self.basedir),
               str(self.firmware), str(self.args.core), str(self.args.launcher), '2']
    if self.args.cli:
      command = [*self.args.runner, str(self.args.supervisor), '--root', str(self.root),
                 '--basedir', str(self.basedir), '--child', str(self.args.core),
                 '--launcher', str(self.args.launcher), '--cycles', '2']
      if not self.args.default_firmware:
        command.extend(['--firmware', str(self.firmware)])
    self.process = subprocess.Popen(command, env=environment, stdout=stdout, stderr=stderr)
    (self.output / 'supervisor-command.json').write_text(json.dumps(command) + '\n')

  def child(self, previous=None):
    deadline = time.monotonic() + 10
    while time.monotonic() < deadline:
      self.poll()
      assert self.process.poll() is None, ('supervisor exited', self.process.returncode)
      path = Path(f'/proc/{self.process.pid}/task/{self.process.pid}/children')
      for value in path.read_text().split():
        pid = int(value)
        if pid == previous:
          continue
        try:
          maps = Path(f'/proc/{pid}/maps').read_text()
        except FileNotFoundError:
          continue
        if str(self.fixture) in maps:
          assert 'libpython' not in maps
          assert all(str(self.fixture) in line for line in maps.splitlines() if 'libusb-1.0.so' in line)
          (self.output / f'child-{pid}-maps.txt').write_text(maps)
          self.children.append(pid)
          return pid
      time.sleep(0.005)
    raise TimeoutError('supervisor did not launch a new native core')

  def ready(self):
    child = self.child()
    super().ready()
    assert (self.params / 'PandaSignatures').read_bytes() == b','.join([bytes([0x42]) * 128] * self.count)
    return child

  def close(self):
    if self.process is not None and self.process.poll() is None:
      self.process.send_signal(signal.SIGINT)
      try:
        self.process.wait(timeout=5)
      except subprocess.TimeoutExpired:
        for pid in self.children:
          try:
            os.kill(pid, signal.SIGKILL)
          except ProcessLookupError:
            pass
    super().close()


def exercise(args, name, onroad, count):
  peer = ComposedPeer(args, args.output / name, onroad=onroad, count=count)
  try:
    peer.start()
    first = peer.ready()
    for index in range(12):
      peer.inputs(index)
      peer.poll()
      time.sleep(0.05)
    peer.send(0x451)
    peer.until(lambda: any(row['op'] == 'write' for row in peer.trace()), 5)
    os.kill(first, signal.SIGINT)
    second = peer.child(previous=first)
    before = len(peer.messages['pandaStates'])
    peer.until(lambda: len(peer.messages['pandaStates']) > before, 5)
    for index in range(12):
      peer.inputs(index, enabled=False, camera=False)
      peer.poll()
      time.sleep(0.05)
    peer.send(0x452)
    peer.until(lambda: sum(row['op'] == 'write' for row in peer.trace()) >= 2, 5)
    assert peer.finish(signal.SIGINT) == 0
    assert not Path(f'/proc/{second}').exists(), 'child survived supervisor shutdown'
    for service in ('can', 'pandaStates', 'peripheralState'):
      assert peer.messages[service] and all(event['valid'] for event in peer.messages[service])
    trace = peer.trace()
    writes = [row for row in trace if row['op'] == 'write']
    assert [row['data'] for row in writes] == [list(packet(address, 0, [0x12, 0x34])) for address in (0x451, 0x452)]
    frames = [frame for event in peer.messages['can'] for frame in event['can']]
    assert {frame['src'] for frame in frames} == {index * 4 for index in range(count)}
    assert all(frame['address'] == 0x321 and frame['dat'] == b'\x04\x05' for frame in frames)
    if onroad:
      for index in range(count):
        safety = [row for row in trace if row['op'] == 'control' and row['kind'] == 0x40 and
                  row['request'] == 0xdc and row['device'] == index]
        assert any(row['index'] == 42 for row in safety) and safety[-1]['value'] == 19
    logs = [event['logMessage'] for event in peer.messages['logMessage']]
    starts = [row for row in logs if isinstance(row.get('msg'), dict) and row['msg'].get('event') == 'pandad.flash_and_connect']
    assert [row['msg']['count'] for row in starts] == [1, 2]
    assert any(row.get('msg') == 'Caught signal 2, exiting' for row in logs)
    assert all('libpython' not in line for line in (peer.output / 'maps.txt').read_text().splitlines())
    result = {'result': 'PASS', 'scenario': name, 'children': [first, second],
              'messages': {key: len(value) for key, value in peer.messages.items()}, 'usb_calls': len(trace)}
    (peer.output / 'report.json').write_text(json.dumps(result, indent=2) + '\n')
    return result
  finally:
    peer.close()


def main():
  parser = argparse.ArgumentParser()
  for key in ('supervisor', 'core', 'launcher', 'collector', 'fixture', 'output'):
    parser.add_argument('--' + key, type=Path, required=True)
  parser.add_argument('--runner', action='append', default=[])
  parser.add_argument('--cli', action='store_true')
  parser.add_argument('--default-firmware', action='store_true')
  args = parser.parse_args()
  if args.default_firmware and not args.cli:
    parser.error('--default-firmware requires --cli')
  for key in ('supervisor', 'core', 'launcher', 'collector', 'fixture', 'output'):
    setattr(args, key, getattr(args, key).resolve())
  args.output.mkdir(parents=True, exist_ok=False)
  results = [exercise(args, name, onroad, count) for name, onroad, count in
             (('offroad', False, 1), ('onroad', True, 1), ('two-pandas', True, 2))]
  paths = [args.supervisor, args.core, args.launcher, args.collector, args.fixture, Path(__file__)]
  report = {'result': 'PASS', 'scenarios': results, 'sha256': {str(path): sha256(path.read_bytes()).hexdigest() for path in paths},
            'runner': args.runner, 'cli': args.cli, 'default_firmware': args.default_firmware,
            'limits': 'Actual native supervisor, core, process launcher, log collector and original Cereal/msgq. ' +
            'Owned USB ABI and firmware files; core compile-time firmware check bypassed for owned signature. ' +
            'Exercises restart, CAN and state IPC, Params signatures and SIGINT cleanup; no physical transport or device acceptance.'}
  (args.output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
  print(json.dumps(report))


if __name__ == '__main__':
  main()
