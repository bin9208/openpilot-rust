import json
from dataclasses import dataclass
from pathlib import Path
import subprocess
import sys

from checkout_peer import Binaries, Peer


@dataclass(frozen=True, slots=True)
class Helpers:
  captured: Path
  handshake: Path


def helper_cases(binaries: Binaries, helpers: Helpers, output: Path):
  results = []
  with Peer(binaries, 'rust', output / 'handshake', True) as peer:
    peer.repo.joinpath('.git').mkdir()
    peer.fixture(stdout=list(b'a' * 40))

    def probe(name: str, request):
      command = [str(helpers.captured.resolve())]
      result = subprocess.run(command, input=json.dumps(request), env=peer.env, capture_output=True,
                              text=True, timeout=4, pass_fds=(peer.held.fileno(),))
      peer.record({'probe': name, 'command': command, 'request': request, 'returncode': result.returncode,
                    'stdout': result.stdout, 'stderr': result.stderr})
      assert result.returncode == 0, result
      response = json.loads(result.stdout)
      descriptors = list(peer.launches.iterdir()) if peer.launches.is_dir() else []
      assert not descriptors, descriptors
      peer.record({'probe_cleanup': name, 'launch_descriptors_after': [str(p) for p in descriptors]})
      results.append({'name': name, 'response': response})
      return response

    request = {'launcher': str(binaries.launcher.resolve()), 'cwd': str(peer.repo), 'argv': ['/bin/true']}
    response = probe('successful-eof', request)
    assert response['kind'] == 'child_exit' and response['code'] == 0 and response['stdout'] == []
    response = probe('failed-target-after-connect', {**request, 'argv': ['./missing-target']})
    assert response['kind'] == 'spawn_error' and response['errno'] == 2
    response = probe('failed-cwd-after-connect', {**request, 'cwd': str(peer.repo / 'absent')})
    assert response['kind'] == 'spawn_error' and response['errno'] == 2
    response = probe('missing-helper', {**request, 'launcher': str(peer.root / 'missing-helper')})
    assert response['kind'] == 'spawn_error'
    fake_request = {**request, 'launcher': str(helpers.handshake.resolve())}
    for mode, expected in [('exit_before', 'spawn_error'), ('corrupt_descriptor', 'spawn_error'),
                            ('exit_connected', 'child_exit'), ('signal_connected', 'child_exit'), ('invalid_marker', 'spawn_error')]:
      peer.control.joinpath('helper.json').write_text(json.dumps(mode))
      peer.record({'helper_mode': mode})
      response = probe(mode, fake_request)
      assert response['kind'] == expected, (mode, response)
      if mode == 'exit_connected':
        assert response['code'] == 7 and response['signal'] is None
      if mode == 'signal_connected':
        assert response['code'] is None and response['signal'] == 9
    peer.launches.rmdir()
    peer.launches.write_text('not a directory')
    try:
      peer.record({'fixture_fs': 'TMPDIR replaced with owned regular file'})
      response = probe('descriptor-storage-error', request)
      assert response['kind'] == 'spawn_error'
    finally:
      peer.launches.unlink()
      peer.launches.mkdir()

  delayed = Binaries(binaries.trace, binaries.fixture, helpers.handshake)
  with Peer(delayed, 'rust', output / 'deadline', True) as peer:
    peer.repo.joinpath('.git').mkdir()
    peer.control.joinpath('helper.json').write_text(json.dumps('delayed_launch'))
    peer.record({'helper_mode': 'delayed_launch'})
    peer.fixture(stdout=list(b'a' * 40), delay_ms=500)
    response = peer.op('read')
    assert response['commit'] == 'a' * 40 and response['elapsed'] > 1.1, response
    assert peer.helper_calls()[0]['mode'] == 'delayed_launch'
    results.append({'name': 'timeout-begins-after-exec', 'response': response})
  return results


def inherited_cases(binaries: Binaries, helpers: Helpers, output: Path):
  results = []
  source = """import json, os, pathlib, subprocess, sys
request = json.load(sys.stdin)
before = os.environ.get('PARAMS_COPY_PATH')
try:
  code = subprocess.call(request['argv'], cwd=request['cwd'], env=os.environ | dict(request.get('environment', [])))
  response = {'kind': 'child_exit', 'code': code}
except OSError as error:
  response = {'kind': 'spawn_error', 'errno': error.errno}
response.update(parent_environment_before=before, parent_environment_after=os.environ.get('PARAMS_COPY_PATH'))
pathlib.Path(request['report']).write_text(json.dumps(response))
"""
  with Peer(binaries, 'rust', output / 'inherited', True) as peer:
    peer.env['PARAMS_COPY_PATH'] = 'parent-copy-path'
    bad = peer.repo / 'bad-format'
    bad.write_text('exit 9\n')
    bad.chmod(0o755)
    denied = peer.repo / 'denied'
    denied.write_text('denied')
    for name, argv, expected in [('exit-zero', ['git'], ('child_exit', 0)),
                                  ('exit-one', ['git'], ('child_exit', 1)),
                                  ('environment-override', ['git'], ('child_exit', 0)),
                                  ('missing', ['./absent'], ('spawn_error', 2)),
                                  ('exec-format', ['./bad-format'], ('spawn_error', 8)),
                                  ('permission', ['./denied'], ('spawn_error', 13))]:
      peer.fixture(stdout=list(b'inherited stdout'), stderr=list(b'inherited stderr'), exit_code=1 if name == 'exit-one' else 0)
      pair = []
      for implementation in ['python', 'rust']:
        report = peer.output / f'{name}-{implementation}.json'
        request = {'launcher': str(binaries.launcher.resolve()), 'cwd': str(peer.repo), 'argv': argv,
                   'inherit': True, 'report': str(report),
                   'environment': [('PARAMS_COPY_PATH', 'child-copy-path')] if name == 'environment-override' else []}
        command = [sys.executable, '-c', source] if implementation == 'python' else [str(helpers.captured.resolve())]
        result = subprocess.run(command, input=json.dumps(request), env=peer.env, capture_output=True,
                                text=True, timeout=4, pass_fds=(peer.held.fileno(),))
        peer.record({'inherited': name, 'implementation': implementation, 'command': command, 'request': request,
                      'returncode': result.returncode, 'stdout': result.stdout, 'stderr': result.stderr})
        assert result.returncode == 0, result
        response = json.loads(report.read_text())
        assert response['parent_environment_before'] == response['parent_environment_after'] == 'parent-copy-path', response
        outcome = (response['kind'], response['code'] if response['kind'] == 'child_exit' else response['errno'])
        assert outcome == expected, (name, implementation, response)
        pair.append((outcome, result.stdout, result.stderr))
        assert not list(peer.launches.iterdir())
      assert pair[0] == pair[1], (name, pair)
      if expected[0] == 'child_exit':
        assert pair[0][1:] == ('inherited stdout', 'inherited stderr'), pair
        for call in peer.calls()[-2:]:
          assert call['inherited_fd_target'] is None and call['manager_daemon'] == 'checkout-parent', call
          assert call['params_copy_path'] == ('child-copy-path' if name == 'environment-override' else 'parent-copy-path'), call
      results.append({'name': name, 'pair': pair})
  return results
