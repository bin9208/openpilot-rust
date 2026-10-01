#!/usr/bin/env python3
"""Real native processes with owned serial PTYs, private GPIO and msgq, loopback HTTP."""
import argparse
import json
import os
from pathlib import Path
import select
import shutil
import signal
import subprocess
import tempfile
import time
import uuid

from openpilot.cereal import log
from qcomgps_peer import Peer, wait
from qcomgps_reference import Source, packet, payload
from qcomgps_setup_reference import compare


def receive(process):
  assert select.select([process.stdout], [], [], 5)[0], 'publication timeout'
  row = json.loads(process.stdout.readline())
  with log.Event.from_bytes(bytes(row['data'])) as message:
    value = message.to_dict()
  assert value['valid']
  assert 0 <= time.monotonic() - value['logMonoTime'] / 1e9 < 2
  return row, value


def scenario(binary, listener, output, failure):
  output.mkdir(parents=True, exist_ok=True)
  prefix = 'qcomgps_' + uuid.uuid4().hex
  shm = Path('/dev/shm/msgq_' + prefix)
  shm.mkdir()
  env = {key: value for key, value in os.environ.items() if key not in ['ZMQ', 'CEREAL_FAKE', 'DEBUG']}
  env.update(OPENPILOT_PREFIX=prefix, LOGPRINT='debug')
  process = observer = None
  with tempfile.TemporaryDirectory(prefix='qcomgps-private-') as temporary:
    root = Path(temporary)
    peer = Peer(root)
    if failure == 'at-retry':
      peer.drop_commands = 1
    if failure == 'injection-failure':
      executable = Path(peer.config['mmcli'])
      executable.write_text(executable.read_text().replace('exit 0', 'exit 7'))
    config = root / 'config.json'
    config.write_text(json.dumps(peer.config))
    try:
      with (output / 'daemon.log').open('w') as log_file, (output / 'listener.log').open('w') as listener_error:
        observer = subprocess.Popen([listener], env=env, stdout=subprocess.PIPE, stderr=listener_error, text=True, bufsize=1, start_new_session=True)
        assert observer.stdout.readline().strip() == 'ready'
        process = subprocess.Popen([binary, '--fixture', config], env=env, stdout=log_file, stderr=log_file, start_new_session=True)
        wait(lambda: peer.setups == 1 and (root / 'sys/class/gpio/gpio34/value').read_text() == '1', process, timeout=10)
        peer.assert_exclusive()
        native_executable = str(Path(f'/proc/{process.pid}/exe').resolve())
        assert native_executable == str(binary.resolve())
        children = Path(f'/proc/{process.pid}/task/{process.pid}/children').read_text().split()
        child_executables = [str(Path(f'/proc/{pid}/exe').resolve()) for pid in children]
        assert child_executables and all(value == native_executable for value in child_executables)
        time.sleep(.1)
        records = []
        source = Source()
        cases = [(0x1477, payload('gps_measurement_report', {'version': 0, 'sv_count': 0})),
                 (0x1480, payload('glonass_measurement_report', {'version': 0, 'sv_count': 0})),
                 (0x14de, payload('oemdre_measurement_report', {'version': 2, 'sv_count': 0, 'source': 0})),
                 (0x14e1, payload('oemdre_svpoly_report', {'version': 2})),
                 (0x1476, payload('position_report', {'u_PosSource': 2, 'w_GpsWeekNumber': 2440, 'q_FltVdop': 500.}))]
        for kind, body in cases:
          data = packet(kind, body)
          peer.send(16, data, fragmented=True)
          raw, value = receive(observer)
          (output / f'{kind:x}.capnp').write_bytes(bytes(raw['data']))
          value['logMonoTime'] = 123456789
          expected = source.publication(16, data)
          assert expected == {'topic': raw['topic'], 'event': value}
          records.append({'topic': raw['topic'], 'event': value})
        peer.send(99, b'unhandled opcode')
        if failure == 'disconnect':
          peer.stop.set()
          os.close(peer.diag_master)
          peer.diag_master = -1
          assert process.wait(timeout=5) == 1
          assert 'disconnected' in (output / 'daemon.log').read_text()
        elif failure == 'crc':
          os.write(peer.diag_master, b'\x10\0\0\x7e')
          assert process.wait(timeout=5) == 1
          assert 'checksum mismatch' in (output / 'daemon.log').read_text()
        elif failure == 'length':
          peer.send(16, b'\0' * 15)
          assert process.wait(timeout=5) == 1
          assert 'log extent' in (output / 'daemon.log').read_text()
        else:
          peer.assist.set()
          wait(lambda: Path(peer.config['assistance']).exists(), process)
          peer.send(16, packet(0xffff, b''))
          wait(lambda: peer.setups == 2, process)
          assert (root / 'injections').read_text().splitlines() == ['-m', 'any', '--timeout', '30',
                   '--location-inject-assistance-data=' + peer.config['assistance']] * (5 if failure == 'injection-failure' else 1)
          fix = packet(0x1476, payload('position_report', {'u_PosSource': 2, 'w_GpsWeekNumber': 2440, 'q_FltVdop': 1.}))
          peer.send(16, fix)
          raw, value = receive(observer)
          assert value['gpsLocation']['hasFix']
          (output / 'fix.capnp').write_bytes(bytes(raw['data']))
          process.send_signal(signal.SIGTERM)
          assert process.wait(timeout=5) == 0
          assert (root / 'sys/class/gpio/gpio34/value').read_text() == '0'
          assert peer.at[-3:] == ['AT+QGPSCFG="outport","none"', 'AT+QGPS?', 'AT+QGPSEND']
          for command in peer.diag[-2:]:
            assert command[0] == 115 and not any(command[16:]), command
        if failure == 'at-retry':
          assert peer.at[:2] == ['AT+QGPS?', 'AT+QGPS?']
          assert 'at_cmd failed, trying again' in (output / 'daemon.log').read_text()
        assert not peer.errors, peer.errors
        assert all(command[0] in [38, 39, 75, 115] for command in peer.diag)
        (output / 'commands.json').write_text(json.dumps({'at': peer.at, 'diagnostic': peer.diag}, indent=2) + '\n')
        (output / 'publications.json').write_text(json.dumps(records, indent=2) + '\n')
        (output / 'result.json').write_text(json.dumps({'scenario': failure, 'returncode': process.returncode,
          'publications': len(records), 'exclusive_serial': True, 'setups': peer.setups, 'executable': native_executable, 'child_executables': child_executables, 'pass': True}) + '\n')
    finally:
      for child in [process, observer]:
        if child is not None:
          if child.poll() is None:
            os.killpg(child.pid, signal.SIGKILL)
          child.wait(timeout=5)
      peer.close()
      shutil.rmtree(shm)
      Path('/tmp/logmessage' + prefix).unlink(missing_ok=True)


def main():
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument('binary', type=Path)
  parser.add_argument('listener', type=Path)
  parser.add_argument('output', type=Path)
  args = parser.parse_args()
  for scenario_name in ['late-assistance-signal', 'injection-failure', 'crc', 'length', 'disconnect', 'at-retry']:
    scenario(args.binary, args.listener, args.output / scenario_name, scenario_name)
    if scenario_name == 'late-assistance-signal':
      compare(args.output / scenario_name / 'commands.json', args.output / 'setup-source.json')
    print('PASS:', scenario_name, flush=True)


if __name__ == '__main__':
  main()
