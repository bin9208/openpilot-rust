import argparse
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import time

from bluetooth_daemon_fixture import Fixture, MAC, Program, wait_for


def scenario(root: Path, binary: Path | None, shim: Path) -> dict:
  fixture = Fixture(root, Program(binary, shim))
  result = {}
  try:
    wait_for(lambda: fixture.status().get('grabbed') == [MAC] and fixture.status().get('stationary'))
    assert fixture.events() == []
    result['offroad'] = fixture.click(blocked=True)
    fixture.update(started=True)
    wait_for(lambda: fixture.status().get('started') and fixture.status().get('stationary'))
    result['onroad'] = fixture.click()
    assert result['onroad']['emitted']
    wait_for(lambda: fixture.events() == [])
    for field in ['car_valid', 'can_valid', 'car_publish']:
      fixture.update(**{field: False})
      wait_for(lambda: not fixture.status().get('stationary'))
      result[field] = fixture.click(blocked=True)
      fixture.update(**{field: True})
      wait_for(lambda: fixture.status().get('stationary'))
    fixture.update(enabled=True)
    wait_for(lambda: not fixture.status().get('stationary'))
    fixture.input(115, 1)
    wait_for(lambda: any(event.get('hold') for event in fixture.events()))
    fixture.update(brake=True)
    wait_for(lambda: not any(event.get('hold') for event in fixture.events()), 0.35)
    result['brake_cancels_hold'] = True
    fixture.input(115, 0)
    fixture.update(brake=False, enabled=False)
    wait_for(lambda: fixture.status().get('stationary'))
    fixture.settings['devices'][MAC]['enabled'] = False
    Fixture.write(fixture.config, fixture.settings)
    wait_for(lambda: fixture.status().get('grabbed') == [])
    Fixture.write(fixture.runtime / 'learn.json', {'address': MAC, 'until': time.monotonic() + 10})
    wait_for(lambda: fixture.status().get('grabbed') == [MAC])
    result['learning'] = fixture.click(114)
    assert result['learning']['reason'] == 'test' and not result['learning']['emitted']
    Fixture.write(fixture.runtime / 'learn.json', {'address': MAC, 'until': 0})
    wait_for(lambda: fixture.status().get('grabbed') == [])
    fixture.settings['devices'][MAC]['enabled'] = True
    Fixture.write(fixture.config, fixture.settings)
    wait_for(lambda: fixture.status().get('grabbed') == [MAC])
    calls = fixture.root / 'calls.log'
    before = calls.read_text().count('open ')
    assert os.write(fixture.keeper, b'partial-event') == 13
    wait_for(lambda: calls.read_text().count('open ') > before)
    wait_for(lambda: fixture.status().get('grabbed') == [MAC])
    result['partial_read_reopened'] = True
    (fixture.node / 'uniq').unlink()
    wait_for(lambda: fixture.status().get('grabbed') == [])
    (fixture.node / 'uniq').write_text(MAC)
    wait_for(lambda: fixture.status().get('grabbed') == [MAC])
    result['discovery_reopened'] = True
    duplicate = subprocess.run(fixture.command, env=fixture.env, capture_output=True, text=True, timeout=3)
    assert duplicate.returncode != 0 and fixture.child.poll() is None
    result['exclusive_reader'] = True
    fixture.child.send_signal(signal.SIGINT)
    assert fixture.child.wait(timeout=2) == -signal.SIGINT
    stopped = fixture.status()
    result['sigint'] = {key: value for key, value in stopped.items() if key != 'time'}
    assert result['sigint'] == {'stationary': False, 'grabbed': [], 'stopped': True}
    assert calls.read_text().splitlines()[-1] == 'close 0 0'
    result['syscalls'] = calls.read_text().splitlines()
  finally:
    fixture.close()
    (root / 'result.json').write_text(json.dumps(result, indent=2))
  return result


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', required=True, type=Path)
  parser.add_argument('--shim', required=True, type=Path)
  parser.add_argument('--output', required=True, type=Path)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  source = scenario(args.output / 'source', None, args.shim)
  native = scenario(args.output / 'native', args.binary, args.shim)
  assert source == native, (source, native)
  (args.output / 'result.json').write_text(json.dumps({'source': source, 'native': native,
    'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest()}, indent=2))
  print('PASS: original/native real IPC and FIFO lifecycle, validity, freshness, held cancellation, learning, discovery, lock and SIGINT')


if __name__ == '__main__':
  main()
