# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Owned Git/Params/command providers for actual Tools source/native peers."""

from __future__ import annotations

from dataclasses import dataclass
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
from datetime import datetime, UTC
from typing import assert_never

import anyio
from anyio.streams.buffered import BufferedByteReceiveStream
from carrot_server_dashcam_sync_probe import Json
from carrot_server_dashcam_upload import request, save


@dataclass(frozen=True, slots=True)
class Options:
  output: Path
  native: Path
  launcher: Path
  binding: Path
  application: bool = False
  force: bool = False
  legacy: bool = False


def repository(root: Path) -> None:
  seed = root / 'seed'
  seed.mkdir()
  environment = os.environ | {
    'GIT_AUTHOR_NAME': 'Owned Tools',
    'GIT_AUTHOR_EMAIL': 'owned@invalid',
    'GIT_COMMITTER_NAME': 'Owned Tools',
    'GIT_COMMITTER_EMAIL': 'owned@invalid',
    'GIT_AUTHOR_DATE': '2000-01-01T00:00:00Z',
    'GIT_COMMITTER_DATE': '2000-01-01T00:00:00Z',
  }
  for args in [['init', '-b', 'dev'], ['config', 'user.name', 'Owned Tools'], ['config', 'user.email', 'owned@invalid']]:
    subprocess.run(['git', *args], cwd=seed, env=environment, check=True, capture_output=True)
  (seed / 'owned.txt').write_text('first\n')
  for args in [['add', '.'], ['commit', '-m', 'Owned initial'], ['branch', 'topic'], ['clone', '--bare', '.', str(root / 'remote.git')]]:
    subprocess.run(['git', *args], cwd=seed, env=environment, check=True, capture_output=True)
  subprocess.run(['git', 'remote', 'add', 'origin', str(root / 'remote.git')], cwd=seed, check=True, capture_output=True)


def providers(root: Path) -> None:
  binary = root / 'bin'
  binary.mkdir()
  for name in ['git', 'bash', 'sudo', 'scons', 'tmux']:
    script = [
      f'#!{sys.executable}',
      'import os, sys, json',
      'from pathlib import Path',
      f'root=Path({str(root)!r})',
      'args=sys.argv[1:]',
      'fds={}',
      'for entry in Path("/proc/self/fd").iterdir():',
      '  try: fds[entry.name]=os.readlink(entry)',
      '  except OSError: pass',
      'with (root/"providers.jsonl").open("a") as stream:',
      f'  stream.write(json.dumps({{"name":{name!r},"argv":args,"cwd":os.getcwd(),"pid":os.getpid(),"sid":os.getsid(0),"fds":fds}})+"\\n")',
    ]
    if name == 'git':
      script.append('os.execv("/usr/bin/git", ["git", *args])')
    elif name == 'bash':
      script.extend(['if args[:1]==["-lc"]: args=["--noprofile", "--norc", "-c", *args[1:]]', 'os.execv("/bin/bash", ["bash", *args])'])
    elif name == 'tmux':
      script.extend(
        [
          'if (root/"tmux-fail").exists():',
          '  sys.stderr.write("owned tmux unavailable\\n"); sys.exit(7)',
          'sys.stdout.write("owned pane\\r\\nsecond line\\n")',
        ]
      )
    else:
      script.append('sys.stdout.write("owned provider complete\\n")')
    (binary / name).write_text('\n'.join(script) + '\n')
    (binary / name).chmod(0o755)


class Peer:
  def __init__(self, name: str, root: Path, options: Options) -> None:
    self.name, self.root, self.options = name, root, options
    self.process: anyio.abc.Process | None = None
    self.log: anyio.AsyncFile[bytes] | None = None
    self.port = 0
    self.prefix = 'd'

  async def start(self) -> None:
    for path in ['state', 'media/0/videos', 'media/0/realdata', 'owned-params/d', 'owned-params/d_tmp', 'web']:
      await anyio.Path(self.root / path).mkdir(parents=True, exist_ok=True)
    if self.options.legacy:
      await anyio.Path(self.root / 'legacy-state').mkdir()
      await anyio.Path(self.root / 'legacy-state/tool_jobs.json').write_text(
        json.dumps({'jobs': [{'id': 'legacy-owned', 'action': 'shell_cmd', 'status': 'running', 'created_at': datetime.now(UTC).timestamp()}]})
      )
    await anyio.Path(self.root / 'web/index.html').write_text('owned Tools application')
    await anyio.to_thread.run_sync(shutil.copyfile, Path('openpilot/selfdrive/carrot_settings.json'), self.root / 'settings.json')
    await anyio.to_thread.run_sync(providers, self.root)
    await anyio.to_thread.run_sync(shutil.copytree, self.options.output / 'seed', self.root / 'repository')
    await anyio.run_process(['git', 'remote', 'set-url', 'origin', str(self.options.output / 'remote.git')], cwd=self.root / 'repository')
    await anyio.run_process(['git', 'fetch', 'origin'], cwd=self.root / 'repository')
    await anyio.run_process(['git', 'branch', '--set-upstream-to=origin/dev', 'dev'], cwd=self.root / 'repository')
    if self.name == 'native':
      helper = self.root / 'repository/rust/bin/openpilot-runtime-install'
      await anyio.Path(helper.parent).mkdir(parents=True)
      await anyio.Path(helper).write_text('#!/bin/sh\nprintf \'%s\\n\' "$@" > package-attempt\necho "Owned matching release unavailable"\nexit 17\n')
      helper.chmod(0o755)
      with (self.root / 'repository/.git/info/exclude').open('a') as excluded:
        excluded.write('\n/rust/\n/package-attempt\n')
    for name, value in {
      'DongleId': 'owned-dongle',
      'GitBranch': 'owned/dev',
      'GitCommit': 'owned-commit',
      'HardwareSerial': 'owned-serial',
      'CustomSR': '12',
    }.items():
      await anyio.Path(self.root / 'owned-params/d' / name).write_text(value)
    environment = os.environ | {
      'PARAMS_ROOT': str(self.root / 'owned-params'),
      'OPENPILOT_PREFIX': self.prefix,
      'CARROT_DATA_DIR': str(self.root),
      'CARROT_SETTINGS_PATH': str(self.root / 'settings.json'),
      'ORIGINAL_PARAMS_BINDING': str(self.options.binding),
      'PATH': str(self.root / 'bin') + os.pathsep + os.environ['PATH'],
      'GIT_CONFIG_NOSYSTEM': '1',
      'GIT_CONFIG_GLOBAL': '/dev/null',
      'GIT_TERMINAL_PROMPT': '0',
      'GIT_ALLOW_PROTOCOL': 'file',
      'GIT_CONFIG_COUNT': '2',
      'GIT_CONFIG_KEY_1': f'url.{self.options.output / "remote.git"}.insteadOf',
      'GIT_CONFIG_VALUE_1': 'https://github.com/bin9208/openpilot-rust.git',
      'GIT_CONFIG_KEY_0': f'url.{self.options.output / "remote.git"}.insteadOf',
      'GIT_CONFIG_VALUE_0': 'https://github.com/ajouatom/openpilot.git',
    }
    argv = [sys.executable, '-P', str(Path(__file__).with_name('carrot_server_tools_source.py'))] if self.name == 'source' else [str(self.options.native)]
    config = {
      'owned_root': str(self.root),
      'params_root': str(self.root / 'owned-params'),
      'prefix': self.prefix,
      'launcher': str(self.options.launcher),
      'application': self.options.application,
      'force': self.options.force,
    }
    self.log = await anyio.Path(self.root / 'process.log').open('wb')
    self.process = await anyio.open_process(argv, env=environment, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=self.log.wrapped)
    assert self.process.stdin and self.process.stdout
    await self.process.stdin.send((json.dumps(config) + '\n').encode())
    with anyio.fail_after(8):
      ready = json.loads(await BufferedByteReceiveStream(self.process.stdout).receive_until(b'\n', 65536))
    self.port = ready['port']
    await anyio.to_thread.run_sync(
      save,
      self.root / 'invocation.json',
      {
        'argv': argv,
        'input': config,
        'ready': ready,
        'environment': {
          key: environment[key]
          for key in environment
          if key.startswith(('GIT_', 'PARAMS_', 'OPENPILOT_', 'CARROT_')) or key in {'PATH', 'ORIGINAL_PARAMS_BINDING'}
        },
      },
    )

  async def close(self) -> list[str]:
    errors = []
    with anyio.CancelScope(shield=True):
      if self.process:
        try:
          if self.process.returncode is None:
            assert self.process.stdin
            await self.process.stdin.send(b'\n')
            await self.process.stdin.aclose()
          with anyio.fail_after(5):
            await self.process.wait()
          if self.process.returncode != 0:
            errors.append(f'{self.name} exit {self.process.returncode}')
        except (OSError, TimeoutError, anyio.BrokenResourceError, anyio.ClosedResourceError) as error:
          errors.append(f'{self.name}: {error}')
          if self.process.returncode is None:
            self.process.kill()
          await self.process.wait()
        finally:
          await self.process.aclose()
      if self.log:
        await self.log.aclose()
    return errors


class Fixture:
  def __init__(self, options: Options) -> None:
    self.options = options
    self.peers = [Peer(name, options.output / name, options) for name in ['source', 'native']]
    self.cases: list[str] = []
    self.preserved_factory = False

  async def start(self) -> None:
    await anyio.Path(self.options.output).mkdir(parents=True)
    await anyio.to_thread.run_sync(repository, self.options.output)
    for peer in self.peers:
      await peer.start()

  def normalized(self, value: Json, peer: Peer) -> Json:
    match value:
      case str():
        return value.replace(str(peer.root), '<owned-root>')
      case list():
        return [self.normalized(item, peer) for item in value]
      case dict():
        return {key: self.normalized(item, peer) for key, item in value.items() if key not in {'id', 'created_at', 'updated_at'}}
      case None | bool() | int() | float():
        return value
      case _:
        assert_never(value)

  async def pair(self, label: str, command: dict[str, Json], job: bool = False) -> None:
    if command['action'] == 'rebuild_all':
      # Earlier calibration/reboot actions intentionally launch delayed children.
      # Let those owned children finish before attributing calls to this repair.
      await anyio.sleep(1.1)
    responses = []
    native_repo = self.peers[1].root / 'repository'
    native_head = subprocess.check_output(['/usr/bin/git', 'rev-parse', 'HEAD'], cwd=native_repo)
    native_source = (native_repo / 'owned.txt').read_bytes()
    provider_log = self.peers[1].root / 'providers.jsonl'
    provider_before = len(provider_log.read_text().splitlines()) if provider_log.exists() else 0
    for peer in self.peers:
      body = json.dumps(command).replace('<owned-root>', str(peer.root)).replace('<remote>', str(self.options.output / 'remote.git')).encode()
      response = await request(peer.port, '/api/tools/start' if job else '/api/tools', 'POST', body)
      if job:
        assert response['status'] == 200 and response['payload']['status'] == 'running'
        job_id = response['payload']['job_id']
        with anyio.fail_after(13):
          while True:
            response = await request(peer.port, '/api/tools/job?id=' + job_id)
            if response['payload']['done']:
              break
            await anyio.sleep(0.01)
      responses.append(response)
    await anyio.to_thread.run_sync(
      save, self.options.output / f'{label}.json', {'command': command, 'job': job, 'source': responses[0], 'native': responses[1]}
    )
    expected = self.normalized(responses[0]['payload'], self.peers[0])
    actual = self.normalized(responses[1]['payload'], self.peers[1])
    action = command['action']
    corrected = action in {'rebuild_all', 'git_reset', 'git_checkout', 'git_sync', 'git_pull', 'git_reset_repo_fetch', 'git_reset_repo_checkout'}
    native_result = actual.get('result', {}) if job else actual
    # Invalid-input and held-lock checks never reach the changed maintenance
    # transaction and must keep their exact original comparison.
    if responses[1]['status'] == 409 or native_result.get('error_code') in {'GIT_BUSY', 'GIT_PRECHECK_FAILED'}:
      corrected = False
    if action == 'git_reset' and command.get('mode', 'hard') not in {'hard', 'soft', 'mixed'}:
      corrected = False
    if action == 'git_checkout' and not command.get('branch') and not command.get('name'):
      corrected = False
    if action == 'git_reset_repo_checkout' and not command.get('branch'):
      corrected = False
    if corrected:
      result = actual['result'] if job else actual
      assert subprocess.check_output(['/usr/bin/git', 'rev-parse', 'HEAD'], cwd=native_repo) == native_head
      assert (native_repo / 'owned.txt').read_bytes() == native_source
      calls = [json.loads(line) for line in provider_log.read_text().splitlines()[provider_before:]]
      assert not any(call['name'] == 'scons' or call['name'] == 'sudo' and call['argv'] == ['reboot'] for call in calls), calls
      if action == 'git_reset' and command.get('mode', 'hard') != 'hard':
        assert result['ok'] is False and result['error'] == 'Soft and mixed reset cannot preserve a verified native source/package pair', result
        assert responses[1]['status'] == (200 if job else 400)
      elif action in {'git_checkout', 'git_sync', 'git_pull'}:
        assert result['ok'] is False and result['rc'] == 1 and result['out'].strip() == 'Native runtime origin must be bin9208/openpilot-rust', result
      elif action == 'git_reset_repo_fetch':
        self.preserved_factory = True
        assert result['ok'] is True and result['branches'] == ['dev', 'topic'], result
        assert subprocess.check_output(['/usr/bin/git', 'config', '--get', 'remote.origin.url'], cwd=native_repo).strip() == b'https://github.com/bin9208/openpilot-rust.git'
        assert subprocess.check_output(['/usr/bin/git', 'remote'], cwd=native_repo).splitlines() == [b'origin', b'second']
      else:
        assert result['ok'] is False and result['rc'] == 17, result
        if not job:
          assert 'Owned matching release unavailable' in result['out']
        arguments = (native_repo / 'package-attempt').read_text().splitlines()
        assert arguments[:3] == ['--stage-update', '--root', str(native_repo)]
        if action == 'git_reset_repo_checkout':
          assert result['staged'] is False and result['reboot_required'] is False
      # The original mutating/rebuild response must not be accepted as the new
      # staged failure contract. Unchanged actions below retain exact comparison.
      assert expected != actual, (label, 'original incorrectly accepted as staged native')
    elif action == 'git_remote_add' and job and self.preserved_factory and command.get('name') == 'second' and command.get('url'):
      # Factory preserved the native second remote/ref objects. The original
      # deleted them and fetches exactly these two known fixture refs again.
      refetch = f"From {self.options.output / 'remote'}\n * [new branch]      dev        -> second/dev\n * [new branch]      topic      -> second/topic\n"
      staged_expected = json.loads(json.dumps(expected))
      assert staged_expected['log'].count(refetch) == 1
      assert staged_expected['result']['out'].count(refetch) == 1
      staged_expected['log'] = staged_expected['log'].replace(refetch, '', 1)
      staged_expected['result']['out'] = staged_expected['result']['out'].replace(refetch, '', 1)
      assert staged_expected == actual
    elif action == 'git_branch_list' and job and self.preserved_factory:
      # The earlier factory/sync actions intentionally diverge: the original
      # deletes local topic and the second remote; native preserves both.
      result, original = actual['result'], expected['result']
      assert result['current_branch'] == original['current_branch'] == 'dev'
      assert result['branches'] == ['dev', 'origin/dev', 'origin/topic', 'second/dev', 'second/topic', 'topic']
      assert original['branches'] == ['dev', 'origin/dev', 'origin/topic']
      assert result['remotes'] == ['origin', 'second'] and original['remotes'] == ['origin']
      assert result['remote_urls'] == {'origin': str(self.options.output / 'remote.git'), 'second': str(self.options.output / 'remote.git')}
      assert result['summary_vars'] == {'count': 6} and len(result['branch_items']) == 6
      assert result['device_type'] == original['device_type'] == 'pc'
      assert result['branch_prefix'] == original['branch_prefix'] == 'c3'
      assert subprocess.check_output(['/usr/bin/git', 'config', '--get', 'remote.origin.url'], cwd=native_repo).strip() == b'https://github.com/bin9208/openpilot-rust.git'
    else:
      assert responses[0]['status'] == responses[1]['status'] and expected == actual, (label, expected, actual)
    self.cases.append(label)

  async def close(self) -> None:
    errors = []
    for peer in self.peers:
      errors.extend(await peer.close())
    await anyio.to_thread.run_sync(save, self.options.output / 'cleanup.json', {'errors': errors})
    assert not errors, errors

  async def result(self) -> None:
    await anyio.to_thread.run_sync(
      save,
      self.options.output / 'result.json',
      {
        'cases': self.cases,
        'pairs': len(self.cases),
        'native_sha256': hashlib.sha256(await anyio.Path(self.options.native).read_bytes()).hexdigest(),
        'providers': 'actual Git with owned file-only URL rewrite; harmless owned sudo/scons/tmux; actual bash without login PATH reset',
      },
    )
