from checkout_peer import Peer

A = 'A' * 40


def real_git(peer: Peer, object_format: str = 'sha1'):
  peer.git('init', '--quiet', '--initial-branch=fixture', '--object-format=' + object_format)
  assert peer.op('read')['commit'] is None
  peer.git('commit', '--quiet', '--allow-empty', '-m', 'original')
  first = peer.git('rev-parse', 'HEAD')
  assert len(first) == (40 if object_format == 'sha1' else 64)
  peer.metadata('f' * 40)
  assert peer.op('read')['commit'] == first
  initial = peer.op('capture')
  assert initial['running_commit'] == first and not initial['reboot_required']
  peer.repo.joinpath('dirty').write_text('untracked change\n')
  assert peer.op('read')['commit'] == first
  peer.git('commit', '--quiet', '--allow-empty', '-m', 'changed')
  second = peer.git('rev-parse', 'HEAD')
  assert first != second
  states = [peer.op('update', 0)['returned'], peer.op('update', 5)['returned']]
  assert states == [False, True]
  peer.git('checkout', '--quiet', '--detach', first)
  reverted = peer.op('update', 10)
  assert not reverted['returned'] and reverted['running_commit'] == first
  return {'first': first, 'second': second, 'changed_reads': states, 'reverted': reverted['returned']}


def git_file(peer: Peer):
  peer.git('init', '--quiet', '--initial-branch=fixture', '--separate-git-dir=' + str(peer.root / 'gitdir'))
  peer.git('commit', '--quiet', '--allow-empty', '-m', 'separate git directory')
  expected = peer.git('rev-parse', 'HEAD')
  assert peer.repo.joinpath('.git').is_file()
  response = peer.op('read')
  assert response['commit'] == expected
  return {'commit': response['commit'], 'git_is_file': True}


def git_fixture(peer: Peer):
  peer.repo.joinpath('.git').mkdir()
  results = []
  cases = [
    ('valid', {'stdout': list(A.encode())}, A.lower()),
    ('unicode-trim', {'stdout': list(('\x1c\x1d\x1e\x1f\u0085\u2003\r\n' + A + '\u3000\n').encode())}, A.lower()),
    ('valid64', {'stdout': list(('BC09' * 16).encode())}, 'bc09' * 16),
    ('bad-status', {'stdout': list(A.encode()), 'exit_code': 7}, None),
    ('bad-stdout-utf8', {'stdout': [255]}, None),
    ('bad-stderr-utf8', {'stdout': list(A.encode()), 'stderr': [255]}, None),
    ('multiline', {'stdout': list((A + '\n' + A).encode())}, None),
    ('nonhex', {'stdout': list(('g' * 40).encode())}, None),
    ('nul', {'stdout': list(A.encode()) + [0]}, None),
    ('signal', {'stdout': list(A.encode()), 'mode': 'signal'}, None),
    ('large-pipes', {'stdout': list(A.encode()), 'padding': 1024 * 1024}, A.lower()),
    ('timeout', {'mode': 'sleep'}, None),
    ('closed-pipes-timeout', {'mode': 'close_sleep'}, None),
  ]
  for name, behavior, expected in cases:
    peer.fixture(**behavior)
    response = peer.op('read')
    assert response['commit'] == expected, (name, response)
    if name.endswith('timeout'):
      assert 0.9 < response['elapsed'] < 3.0, (name, response)
    results.append({'name': name, 'commit': response['commit']})
  calls = peer.calls()
  assert len(calls) == len(cases)
  assert all(call['argv'] == ['git', '--no-optional-locks', 'rev-parse', '--verify', 'HEAD^{commit}'] for call in calls), calls
  assert all(call['cwd'] == str(peer.repo) for call in calls), calls
  command = peer.bin / 'git'
  command.unlink()
  response = peer.op('read')
  assert response['commit'] is None and len(peer.calls()) == len(cases)
  results.append({'name': 'missing-executable', 'commit': None})
  command.write_text("printf '%s' '" + 'f' * 40 + "'\n")
  command.chmod(0o700)
  response = peer.op('read')
  assert response['commit'] is None and len(peer.calls()) == len(cases)
  results.append({'name': 'executable-format', 'commit': None})
  command.chmod(0)
  response = peer.op('read')
  assert response['commit'] is None and len(peer.calls()) == len(cases)
  results.append({'name': 'executable-permission', 'commit': None})
  command.chmod(0o700)
  peer.fallback_bin.joinpath('git').symlink_to(peer.binaries.fixture.resolve())
  peer.fixture(stdout=list(A.encode()))
  response = peer.op('read')
  assert response['commit'] == A.lower(), ('PATH search after ENOEXEC', response)
  results.append({'name': 'path-after-exec-format', 'commit': response['commit']})
  return results
