import signal
import os

from supervision_peer import wait_until


def launch_context(peer):
  spec = peer.native()
  peer.launch([spec])
  assert peer.op('prepare', name='child')['error'] is None
  assert peer.state() == {'name': 'child', 'pid': 0, 'running': False, 'shouldBeRunning': False, 'exitCode': 0}
  assert peer.op('start', name='child')['error'] is None
  ready = peer.ready()
  state = peer.state()
  assert state['running'] and state['shouldBeRunning'] and state['pid'] == ready['pid'] and state['exitCode'] == 0
  assert ready['manager_daemon'] == 'child' and ready['inherited'] == 'inherited-value'
  assert ready['cwd'] == str(peer.root / 'child')
  for number, key in enumerate(['stdin_target', 'stdout_target', 'stderr_target']):
    assert ready[key] == os.readlink(f'/proc/{peer.process.pid}/fd/{number}')
  assert ready['group'] == peer.process.pid
  peer.op('start', name='child')
  assert peer.state()['pid'] == ready['pid']
  stopped = peer.op('stop', name='child')
  assert stopped['result'] == 0 and stopped['error'] is None
  assert peer.signals() == [signal.SIGINT]
  assert peer.state() == {'name': 'child', 'pid': 0, 'running': False, 'shouldBeRunning': False, 'exitCode': 0}
  assert peer.op('stop', name='child')['result'] is None
  assert peer.op('signal', name='child', signal=signal.SIGTERM)['error'] is None
  return {'exit': stopped['result'], 'signals': peer.signals(), 'context': 'matched', 'idempotent': True}


def exec_failure(peer, kind):
  spec = peer.native()
  match kind:
    case 'target':
      spec['argv'][0] = './missing'
    case 'cwd':
      spec['cwd'] = 'absent'
    case 'empty':
      spec['argv'] = []
    case 'nul':
      spec['argv'][0] = 'bad\0command'
    case 'format':
      path = peer.root / 'child' / 'invalid-executable'
      path.write_text('exit 7\n')
      path.chmod(0o755)
      spec['argv'][0] = './invalid-executable'
    case 'permission':
      path = peer.root / 'child' / 'unexecutable'
      path.write_text('#!/bin/sh\nexit 7\n')
      path.chmod(0o644)
      spec['argv'][0] = './unexecutable'
    case 'directory':
      spec['argv'][0] = '.'
    case 'name-cwd':
      spec['name'] = 'child\0invalid'
      spec['cwd'] = 'absent'
    case 'nul-cwd':
      spec['argv'][0] = 'bad\0command'
      spec['cwd'] = 'absent'
    case other:
      raise ValueError(other)
  peer.launch([spec])
  name = spec['name']
  response = peer.op('start', name=name)
  assert response['error'] is None and response['snapshots'][0]['has_process']
  pid = response['snapshots'][0]['state']['pid']
  assert pid > 0
  state = wait_until(lambda: (state if not state['running'] else None) if (state := peer.state(name)) else None)
  assert state['exitCode'] == 1 and state['shouldBeRunning']
  peer.op('start', name=name)
  assert peer.state(name)['pid'] == pid
  assert peer.op('signal', name=name, signal=signal.SIGKILL)['error'] is None
  assert peer.op('stop', name=name, block=False)['result'] == 1
  assert not peer.state(name)['running'] and peer.state(name)['pid'] == 0
  if kind == 'name-cwd':
    assert 'No such file' not in (peer.output / 'stderr.log').read_text()
  if kind == 'nul-cwd':
    assert 'No such file' in (peer.output / 'stderr.log').read_text()
  return {'child_error': 1, 'retained_until_stop': True}


def nonblocking_once(peer):
  peer.launch([peer.native(mode='delay')])
  peer.op('start', name='child')
  peer.ready()
  first = peer.op('stop', name='child', block=False)
  assert first['result'] is None and first['elapsed'] < 0.1
  assert first['snapshots'][0]['shutting_down'] and not first['snapshots'][0]['state']['shouldBeRunning']
  second = peer.op('stop', name='child', block=False)
  assert second['result'] == 0 and 0.1 <= second['elapsed'] < 1.5, second
  assert peer.signals() == [signal.SIGINT]
  return {'first': None, 'second': 0, 'single_signal': peer.signals()}


def kill_timeout(peer, retry):
  peer.launch([peer.native(mode='ignore')])
  peer.op('start', name='child')
  ready = peer.ready()
  response = peer.op('stop', name='child', retry=retry)
  assert 4.9 <= response['elapsed'] < 7, response
  assert peer.signals() == [signal.SIGINT]
  if retry:
    assert response['result'] == -signal.SIGKILL
    assert not response['snapshots'][0]['has_process']
  else:
    assert response['result'] is None
    assert response['snapshots'][0]['shutting_down'] and response['snapshots'][0]['state']['running']
    assert response['snapshots'][0]['state']['pid'] == ready['pid']
    peer.op('signal', name='child', signal=signal.SIGKILL)
    wait_until(lambda: not peer.state()['running'])
    assert peer.op('stop', name='child')['result'] == -signal.SIGKILL
  return {'retry': retry, 'returned': response['result'], 'first_signal': peer.signals()}


def explicit_signal(peer, sigkill=False):
  peer.launch([peer.native(sigkill=sigkill)])
  peer.op('start', name='child')
  peer.ready()
  options = {} if sigkill else {'signal': signal.SIGUSR1}
  response = peer.op('stop', name='child', **options)
  assert response['result'] == (-signal.SIGKILL if sigkill else 0)
  assert peer.signals() == ([] if sigkill else [signal.SIGUSR1])
  return {'exit': response['result'], 'signals': peer.signals()}


def start_while_stopping(peer):
  peer.launch([peer.native(mode='delay')])
  peer.op('start', name='child')
  old = peer.ready()['pid']
  peer.op('stop', name='child', block=False)
  started = peer.op('start', name='child')
  assert started['elapsed'] >= 0.1 and started['error'] is None
  assert peer.state()['pid'] != old and not started['snapshots'][0]['shutting_down']
  wait_until(lambda: peer.ready()['pid'] != old)
  assert peer.op('stop', name='child')['result'] == 0
  return {'restarted_after_stop': True}


def restart(peer):
  peer.launch([peer.native()])
  peer.op('start', name='child')
  old = peer.ready()['pid']
  response = peer.op('restart', name='child')
  assert response['error'] is None and peer.state()['pid'] != old
  wait_until(lambda: peer.ready()['pid'] != old)
  assert peer.op('stop', name='child')['result'] == 0
  return {'new_pid': True, 'restart_exit': -signal.SIGKILL}
