from __future__ import annotations

import argparse
import fcntl
from http.server import ThreadingHTTPServer
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time
import uuid

from openpilot.cereal import messaging
from carrot_server_auto_update_pull_cases import assert_repository, git, template
from carrot_server_auto_update_service import ROOT, TOOLS
from carrot_server_auto_update_service_http import Peer
from carrot_server_auto_update_service_probe import Probe, exited, wait_file
import threading


def locked(path: Path) -> bool:
  with path.open('a+b') as file:
    try:
      fcntl.flock(file, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except BlockingIOError:
      return True
  return False


def publish(pm, valid: bool = True, engaged: bool = False) -> None:
  for service in ['managerState', 'carState', 'selfdriveState', 'deviceState']:
    event = messaging.new_message(service)
    event.valid = valid
    if service == 'carState':
      event.carState.gearShifter = 'park'
    elif service == 'selfdriveState':
      event.selfdriveState.enabled = engaged
    elif service == 'deviceState':
      event.deviceState.started = True
    pm.send(service, event)


def pump(pm, seconds: float, *, valid: bool = True, engaged: bool = False) -> None:
  deadline = time.monotonic() + seconds
  while time.monotonic() < deadline:
    publish(pm, valid, engaged)
    time.sleep(.05)


def execute(name: str, kind: str, root: Path, base: Path, heads: dict[str, str], binary: Path, address: str) -> dict:
  root.mkdir(parents=True)
  repository, state, params = root / 'repository', root / 'state', root / 'params'
  shutil.copytree(base, repository)
  git(repository, 'reset', '--hard', heads['base'])
  remote = root / 'origin.git'
  subprocess.run(['/usr/bin/git', 'clone', '--bare', '-q', str(base), str(remote)], check=True)
  subprocess.run(['/usr/bin/git', '-C', str(remote), 'update-ref', 'refs/heads/owned-update', heads['target']], check=True)
  git(repository, 'remote', 'add', 'origin', remote.as_uri())
  git(repository, 'config', 'branch.owned-update.remote', 'origin')
  git(repository, 'config', 'branch.owned-update.merge', 'refs/heads/owned-update')
  state.mkdir()
  (state / 'git.json').write_text('{"unrelated":"preserved"}')
  selected = {'auto_update_git_pull': name != 'disabled', 'auto_update_reboot': 'park' if name == 'updated-reboot' else 'off'}
  (state / 'web_settings.json').write_text(json.dumps(selected))
  (params / 'd').mkdir(parents=True)
  (params / 'd/DongleId').write_text('owned-runtime')
  wrappers = root / 'bin'
  wrappers.mkdir()
  wrapper = TOOLS / 'carrot_server_auto_update_service_git.py'
  wrapper.chmod(0o755)
  (wrappers / 'git').symlink_to(wrapper)
  prefix = 'rust-probe-auto-' + uuid.uuid4().hex[:12]
  namespace = Path('/dev/shm/msgq_' + prefix)
  namespace.mkdir()
  os.environ['OPENPILOT_PREFIX'] = prefix
  pm = messaging.PubMaster(['managerState', 'carState', 'selfdriveState', 'deviceState'])
  config = {'mode': 'app' if name == 'app-config' else 'manager' if name == 'manager-retry' else 'runtime',
    'repository': str(repository), 'source': str(ROOT), 'state': str(state), 'params': str(params), 'lock': str(root / 'repository.lock'),
    'launcher': str(ROOT / 'rust/target/debug/openpilot-process-child'), 'failures': 1 if name == 'manager-retry' else 0}
  ready, release, log = root / 'ready.json', root / 'release', root / 'git.jsonl'
  environment = os.environ | {'PATH': str(wrappers) + ':' + os.environ['PATH'], 'CARROT_REPO_LOCK_PATH': config['lock'],
    'CWEB_PUSH_NOTIFY_URL': address + '/notify', 'CWEB_PUSH_REPORT_TOKEN': '',
    'OWNED_AUTO_REPOSITORY': str(repository), 'OWNED_AUTO_LOG': str(log), 'OWNED_AUTO_MODE': name,
    'OWNED_AUTO_READY': str(ready), 'OWNED_AUTO_RELEASE': str(release), 'GIT_CONFIG_GLOBAL': '/dev/null',
    'GIT_CONFIG_SYSTEM': '/dev/null', 'GIT_ALLOW_PROTOCOL': 'file', 'GIT_TERMINAL_PROMPT': '0'}
  command = [str(binary)] if kind == 'native' else [sys.executable, '-P', str(TOOLS / 'carrot_server_auto_update_service_source.py')]
  assert_repository(repository)
  before = len(Peer.records)
  probe = Probe(command, config, environment, root / 'invocation.json')
  observations = []
  pidfd = None
  try:
    if name == 'manager-retry':
      observations.append(probe.command({'now': 0.}, 'sample'))
      observations.append(probe.command({'now': 0.}, 'sample'))
      for now in [0., 2., 4., 6., 8., 10.]:
        publish(pm)
        observations.append(probe.command({'now': now}, 'sample'))
      publish(pm, valid=False)
      observations.append(probe.command({'now': 11.}, 'sample'))
      assert observations[0] == {'sample': False, 'creates': 1} and observations[-2]['sample'] and not observations[-1]['sample'], observations
    else:
      for now in [0., 2., 4., 6., 8., 10.]:
        probe.command({'now': now})
        pump(pm, 1.05)
      if name == 'disabled':
        assert not log.exists()
      elif name in {'hold-fetch', 'stop-fetch', 'hold-config', 'app-config', 'stop-notify'}:
        notice = wait_file(ready)
        pidfd = os.pidfd_open(notice['pid'])
        assert locked(Path(config['lock'])) and notice['held'] and notice['inherited'], notice
        if name == 'hold-fetch':
          pump(pm, 1.3, valid=False)
          release.touch()
          pump(pm, .5, valid=False)
        else:
          probe.stop(wait=False)
          if name in {'hold-config', 'app-config'}:
            time.sleep(.15)
            observations.append({'pending_after_stop': probe.process.poll() is None, 'lock_held_after_stop': locked(Path(config['lock']))})
            assert all(observations[-1].values())
            release.touch()
          probe.finish()
          assert exited(pidfd)
      else:
        deadline = time.monotonic() + 4
        while not (params / 'd/DoReboot').exists():
          assert time.monotonic() < deadline, 'owned verified update/reboot absent'
          pump(pm, .1)
        assert (params / 'd/DoReboot').read_bytes() == b'1'
    if probe.process.poll() is None:
      probe.stop()
    commands = [json.loads(line) for line in log.read_text().splitlines()] if log.exists() else []
    result = json.loads(probe.stdout[-1])
    row = {'result': result, 'observations': observations, 'head': git(repository, 'rev-parse', 'HEAD'), 'commands': [record['argv'] for record in commands],
      'notify_requests': Peer.records[before:], 'lock_released': not locked(Path(config['lock'])), 'owned_wait_exited': pidfd is None or exited(pidfd)}
    if name in {'disabled', 'hold-fetch', 'stop-fetch', 'hold-config', 'app-config'}:
      assert row['head'] == heads['base'] and row['result']['state'] == {'unrelated': 'preserved'} and not row['notify_requests'], row
    if name == 'stop-notify':
      assert row['head'] == heads['target'] and row['result']['state']['auto_update']['status'] == 'updated' and not row['notify_requests'], row
    if name == 'updated-reboot':
      assert row['result']['state']['auto_update']['status'] == 'reboot_requested' and len(row['notify_requests']) == 1, row
    assert row['lock_released'] and row['owned_wait_exited'], row
    (root / 'result.json').write_text(json.dumps(row, indent=2, ensure_ascii=False))
    return row
  finally:
    release.touch()
    probe.cleanup()
    if pidfd is not None:
      os.close(pidfd)
    del pm
    shutil.rmtree(namespace)


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('output', type=Path)
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--case', action='append', default=[])
  args = parser.parse_args()
  args.output = args.output.resolve()
  args.output.mkdir(parents=True)
  base, heads = template(args.output, os.environ | {'GIT_AUTHOR_DATE': '2026-09-01T00:00:00+00:00', 'GIT_COMMITTER_DATE': '2026-09-01T00:00:00+00:00'})
  peer = ThreadingHTTPServer(('127.0.0.1', 0), Peer)
  worker = threading.Thread(target=peer.serve_forever)
  worker.start()
  pairs = []
  try:
    for name in args.case or ['manager-retry', 'disabled', 'updated-reboot', 'hold-fetch', 'stop-fetch', 'hold-config', 'app-config', 'stop-notify']:
      outcomes = [execute(name, kind, args.output / name / kind, base, heads, args.binary, f'http://127.0.0.1:{peer.server_port}')
        for kind in ['source', 'native']]
      def comparable(row: dict) -> dict:
        return {key: value for key, value in row.items() if key != 'commands'}
      equal = comparable(outcomes[0]) == comparable(outcomes[1])
      pairs.append({'name': name, 'source': outcomes[0], 'native': outcomes[1], 'equal': equal})
      print(json.dumps({'name': name, 'equal': equal}), flush=True)
  finally:
    peer.shutdown()
    peer.server_close()
    worker.join()
  result = {'pairs': pairs, 'differences': [row['name'] for row in pairs if not row['equal']], 'peer_stopped': not worker.is_alive()}
  (args.output / 'result.json').write_text(json.dumps(result, indent=2, ensure_ascii=False))
  assert not result['differences'], result['differences']


if __name__ == '__main__':
  main()
