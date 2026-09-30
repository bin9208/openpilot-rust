import json

from checkout_peer import Peer

A = 'a' * 40
B = 'BC09' * 16


def metadata_cases(peer: Peer):
  results = []

  def check(name: str, expected: str | None) -> None:
    response = peer.op('read')
    assert response['commit'] == expected, (name, response, expected)
    results.append({'name': name, 'commit': response['commit']})

  check('missing-build', None)
  values = [
    ('sha40', A, A), ('sha64-uppercase', B, B.lower()),
    ('empty', '', None), ('short39', 'a' * 39, None), ('long41', 'a' * 41, None),
    ('short63', 'b' * 63, None), ('long65', 'b' * 65, None), ('nonhex', 'g' * 40, None),
    ('leading-space', ' ' + A, None), ('trailing-newline', A + '\n', None),
    ('nul', A[:-1] + '\0', None), ('fullwidth', 'Ａ' * 40, None), ('arabic-digits', '١' * 40, None),
    ('surrogate', A[:-1] + '\ud800', None), ('null', None, None), ('bool', True, None),
    ('integer', 123, None), ('float', 1.5, None), ('array', [A], None), ('object', {'value': A}, None),
    ('nan-value', float('nan'), None), ('inf-value', float('inf'), None),
  ]
  for name, value, expected in values:
    peer.write(json.dumps({'openpilot': {'git_commit': value}}).encode())
    check(name, expected)
  raw = [
    ('unrelated-nonfinite-surrogate', '{"other":[NaN,Infinity,-Infinity,"\\ud800"],"openpilot":{"git_commit":"' + A + '"}}', A),
    ('unrelated-infinite-exponent', '{"other":1e9999,"openpilot":{"git_commit":"' + A + '"}}', A),
    ('unrelated-wide-integer', '{"other":' + '9' * 4300 + ',"openpilot":{"git_commit":"' + A + '"}}', A),
    ('integer-limit', '{"other":' + '9' * 4301 + ',"openpilot":{"git_commit":"' + A + '"}}', None),
    ('last-commit-wins', '{"openpilot":{"git_commit":null,"git_commit":"' + A + '"}}', A),
    ('last-invalid-wins', '{"openpilot":{"git_commit":"' + A + '","git_commit":null}}', None),
    ('last-openpilot-wins', '{"openpilot":null,"openpilot":{"git_commit":"' + A + '"}}', A),
    ('escaped-key-and-commit', '{"open\\u0070ilot":{"git_commit":"' + '\\u0061' * 40 + '"}}', A),
    ('surrogate-key', '{"\\ud800":true,"openpilot":{"git_commit":"' + A + '"}}', A),
    ('trailing-json', '{"openpilot":{"git_commit":"' + A + '"}} null', None),
    ('utf8-bom', '\ufeff{"openpilot":{"git_commit":"' + A + '"}}', None),
    ('missing-openpilot', '{}', None), ('missing-commit', '{"openpilot":{}}', None),
    ('null-openpilot', '{"openpilot":null}', None), ('list-openpilot', '{"openpilot":[]}', None),
    ('null-root', 'null', None), ('list-root', '[]', None), ('string-root', '"openpilot"', None),
    ('malformed-json', '{', None), ('empty-file', '', None),
  ]
  for name, value, expected in raw:
    peer.write(value.encode())
    check(name, expected)
  peer.write(b'{"other":"\xff","openpilot":{"git_commit":"' + A.encode() + b'"}}')
  check('invalid-utf8', None)
  peer.metadata(A)
  build = peer.repo / 'build.json'
  build.chmod(0)
  try:
    peer.record({'fixture_fs': 'build.json mode000'})
    check('unreadable-build', None)
  finally:
    build.chmod(0o600)
  build.unlink()
  build.mkdir()
  peer.record({'fixture_fs': 'build.json directory'})
  check('build-is-directory', None)
  build.rmdir()
  peer.metadata(A)
  git = peer.repo / '.git'
  git.symlink_to('.git')
  peer.record({'fixture_fs': '.git symlink loop'})
  check('git-loop-falls-back', A)
  git.unlink()
  git.symlink_to('absent')
  peer.record({'fixture_fs': '.git dangling symlink'})
  check('git-missing-target-falls-back', A)
  git.unlink()
  git.write_text('invalid git metadata\n')
  peer.record({'fixture_fs': '.git regular invalid file'})
  check('git-file-prevents-build-fallback', None)
  git.unlink()
  git.mkdir()
  peer.record({'fixture_fs': '.git empty directory'})
  check('git-directory-prevents-build-fallback', None)
  return results
