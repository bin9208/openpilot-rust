from supervision_peer import wait_until


def ordered_ensure(peer):
  specs = [peer.native('a', 'delay'), peer.native('b'), peer.native('disabled', enabled=False), peer.native('excluded')]
  peer.launch(specs)
  peer.op('start', name='a')
  peer.ready('a')
  response = peer.op('ensure', allowed=['b', 'disabled', 'excluded'], not_run=['excluded'])
  assert response['error'] is None and response['result'] == {'running': ['b'], 'predicates': ['a', 'b']}
  snapshots = response['snapshots']
  assert snapshots[0]['shutting_down'] and not snapshots[0]['state']['shouldBeRunning']
  assert snapshots[1]['state']['running'] and snapshots[1]['state']['shouldBeRunning']
  assert not snapshots[2]['has_process'] and not snapshots[3]['has_process']
  peer.ready('b')
  peer.op('stop', name='a')
  peer.op('stop', name='b')
  peer.drain(100)
  messages = [r['msg'] for r in peer.records]
  assert messages.index('sending signal 2 to a') < messages.index('starting process b')
  return response['result']


def dead_first(peer, restart):
  peer.launch([peer.native(restart_if_crash=restart)])
  peer.op('start', name='child')
  old = peer.ready()['pid']
  temporary = peer.root / 'child' / 'exit.tmp'
  temporary.write_text('7')
  temporary.replace(peer.root / 'child' / 'exit')
  wait_until(lambda: peer.state()['exitCode'] == 7)
  response = peer.op('ensure', allowed=['child'])
  assert response['error'] is None and response['result']['running'] == ['child']
  assert peer.state()['pid'] != old
  wait_until(lambda: peer.ready()['pid'] != old)
  peer.op('stop', name='child')
  peer.drain(100)
  assert all(not r['msg'].startswith('Restarting ') for r in peer.records)
  return {'restart_policy': restart, 'dead_child_replaced_without_restart_branch': True}


def predicate_race(peer, restart):
  peer.launch([peer.native(restart_if_crash=restart)])
  peer.op('start', name='child')
  old = peer.ready()['pid']
  release = peer.root / 'child' / 'exit'
  response = peer.op('ensure', allowed=['child'], race={'name': 'child', 'release': str(release), 'pid': old})
  assert response['error'] is None and response['result']['predicates'] == ['child']
  assert response['result']['running'] == ['child']
  if restart:
    assert response['snapshots'][0]['state']['pid'] != old
  else:
    state = response['snapshots'][0]['state']
    assert state['pid'] == old and state['exitCode'] == 7 and not state['running'] and state['shouldBeRunning']
  if restart:
    wait_until(lambda: peer.ready()['pid'] != old)
  else:
    peer.op('ensure', allowed=['child'])
    assert peer.state()['pid'] != old
    wait_until(lambda: peer.ready()['pid'] != old)
  peer.op('stop', name='child')
  peer.drain(100)
  errors = [r['msg'] for r in peer.records if r['level'] == 'ERROR']
  assert errors == (['Restarting child (exitcode 7)'] if restart else [])
  return {'restart_policy': restart, 'race_restart_logs': errors}
