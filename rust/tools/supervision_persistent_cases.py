import os
from pathlib import Path
import subprocess

from supervision_peer import wait_until


def reusable_daemon(peer):
  spec = peer.persistent()
  peer.launch([spec])
  peer.op('prepare', name='persistent')
  assert not (peer.root / 'params').exists()
  response = peer.op('start', name='persistent')
  assert response['error'] is None and not response['snapshots'][0]['has_process']
  ready = peer.ready('persistent')
  pid = peer.pid()
  assert ready['pid'] == pid and ready['group'] == pid and pid != peer.process.pid
  assert all(ready[key] == '/dev/null' for key in ['stdin_target', 'stdout_target', 'stderr_target'])
  assert ready['manager_daemon'] == 'parent-daemon'
  assert ready['cwd'] == str(Path.cwd())
  assert peer.state('persistent') == {'name': 'persistent', 'pid': 0, 'running': False, 'shouldBeRunning': False, 'exitCode': 0}
  assert peer.op('stop', name='persistent')['result'] is None
  assert peer.op('signal', name='persistent', signal=15)['error'] is None
  assert peer.op('restart', name='persistent')['error'] is None
  assert peer.pid() == pid
  os.kill(pid, 0)
  peer.close_supervisor()
  os.kill(pid, 0)
  peer.launch([spec])
  assert peer.op('start', name='persistent')['error'] is None
  assert peer.pid() == pid
  assert peer.op('ensure', allowed=['persistent'])['result']['running'] == ['persistent']
  assert peer.pid() == pid
  return {'independent_group': True, 'same_pid_after_manager_exit': True, 'state_has_no_child': True}


def identity_mismatch(peer, dead):
  specs = [peer.native('other'), peer.persistent()]
  peer.launch(specs)
  peer.op('start', name='other')
  old = peer.ready('other')['pid']
  if dead:
    peer.op('stop', name='other')
  peer.put_pid(str(old).encode())
  response = peer.op('start', name='persistent')
  assert response['error'] is None
  ready = peer.ready('persistent')
  assert peer.pid() == ready['pid'] and peer.pid() != old
  if not dead:
    assert peer.state('other')['running']
    peer.op('stop', name='other')
  return {'replaced_dead_pid': dead, 'replaced_identity_mismatch': not dead}


def invalid_pid(peer, value, expected_warning):
  peer.launch([peer.persistent()])
  if value is not None:
    peer.put_pid(value)
  response = peer.op('start', name='persistent')
  assert response['error'] is None
  ready = peer.ready('persistent')
  assert peer.pid() == ready['pid']
  peer.drain(100)
  warnings = [r['msg'] for r in peer.records if r['level'] == 'WARNING']
  assert bool(warnings) == expected_warning
  return {'started': True, 'warnings': warnings}


def formatted_pid(peer):
  peer.launch([peer.persistent()])
  peer.op('start', name='persistent')
  peer.ready('persistent')
  pid = peer.pid()
  text = ' \v\f\t\n+0_' + '_'.join(str(pid)) + '\r '
  peer.put_pid(text.encode())
  response = peer.op('start', name='persistent')
  assert response['error'] is None and peer.pid() == pid
  peer.drain(100)
  assert not [r for r in peer.records if r['level'] == 'WARNING']
  assert len([r for r in peer.records if r['msg'] == 'starting daemon persistent']) == 1
  return {'formatted_pid_reused': True}


def pid_overflow(peer, value):
  peer.launch([peer.persistent()])
  peer.put_pid(value)
  response = peer.op('start', name='persistent')
  assert response['error']['kind'] == 'OverflowError', response
  assert not (peer.root / 'persistent' / 'ready.json').exists()
  return {'error': response['error']['kind']}


def persistent_spawn_error(peer, bad_format=False):
  spec = peer.persistent()
  target = peer.root / 'missing-command'
  if bad_format:
    target.write_text('exit 7\n')
    target.chmod(0o755)
  spec['argv'][0] = str(target)
  peer.launch([spec])
  response = peer.op('start', name='persistent')
  assert response['error']['kind'] == ('OSError' if bad_format else 'FileNotFoundError') and not response['snapshots'][0]['has_process']
  assert not Path(f'/proc/{peer.process.pid}/task/{peer.process.pid}/children').read_text().strip()
  assert not (peer.root / 'params' / peer.prefix / 'AthenadPid').exists()
  return {'parent_error': response['error']['kind']}


def path_search(peer, persistent=False):
  spec = peer.persistent() if persistent else peer.native()
  first, second = peer.root / 'first', peer.root / 'second'
  first.mkdir()
  second.mkdir()
  bad = first / 'fixture'
  bad.write_text('exit 7\n')
  bad.chmod(0o755)
  (second / 'fixture').symlink_to(peer.args.fixture.resolve())
  spec['argv'][0] = 'fixture'
  peer.launch([spec], environment={'PATH': str(first) + ':' + str(second)})
  response = peer.op('start', name=spec['name'])
  assert response['error'] is None, response
  ready = peer.ready(spec['name'])
  assert ready['argv'][0] == 'fixture'
  assert peer.op('stop', name=spec['name'])['result'] == (None if persistent else 0)
  return {'searched_after_exec_format_error': True, 'argv_zero_preserved': True}


def inherited_descriptor(peer, persistent=False):
  spec = peer.persistent() if persistent else peer.native()
  path = peer.root / 'manager-owned-descriptor'
  with path.open('w') as descriptor:
    peer.launch([spec], environment={'PROCESS_FIXTURE_FD': str(descriptor.fileno())}, inherited=(descriptor.fileno(),))
    assert peer.op('start', name=spec['name'])['error'] is None
    ready = peer.ready(spec['name'])
    assert ready['inherited_fd_target'] == (None if persistent else str(path)), ready
    peer.op('stop', name=spec['name'])
  return {'closed_extra_descriptor': persistent}


def unreadable_pid_and_failed_write(peer):
  peer.launch([peer.persistent()])
  path = peer.root / 'params' / peer.prefix / 'AthenadPid'
  path.mkdir(parents=True)
  response = peer.op('start', name='persistent')
  assert response['error'] is None
  first = peer.ready('persistent')['pid']
  assert path.is_dir()
  response = peer.op('start', name='persistent')
  assert response['error'] is None
  wait_until(lambda: peer.ready('persistent')['pid'] != first)
  os.kill(first, 0)
  assert path.is_dir()
  return {'unreadable_treated_missing': True, 'source_write_error_ignored': True, 'both_daemons_owned': True}


def invalid_cmdline_utf8(peer):
  spec = peer.persistent()
  root = peer.root / 'invalid-utf8'
  root.mkdir()
  with subprocess.Popen([os.fsencode(peer.args.fixture.resolve()), os.fsencode(root), b'normal', b'\xff' + spec['identity'].encode()],
                        stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL) as child:
    try:
      peer.ready('invalid-utf8')
      peer.launch([spec])
      peer.put_pid(str(child.pid).encode())
      response = peer.op('start', name='persistent')
      assert response['error']['kind'] == 'UnicodeDecodeError', response
      assert child.poll() is None
    finally:
      child.kill()
      child.wait(timeout=3)
  return {'error': 'UnicodeDecodeError'}
