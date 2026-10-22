from __future__ import annotations

import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import time

from qa_space_policy import free_for_growth


ROLES = (
  'openpilot-runtime-install',
  'openpilot-manager',
  'openpilot-process-child',
  'openpilot-carrot-recovery',
  'openpilot-carrot-web-watchdog',
  'openpilot-carrot-server',
  'openpilot-agnos',
  'openpilot-agnos-updater',
  'openpilot-text-window',
  'openpilot-usbgpu-model',
  'sudo',
  'tmux',
  'taskset',
)
COMMIT = '0123456789abcdef0123456789abcdef01234567'


def records(path: Path) -> list[dict]:
  if not path.exists():
    return []
  return [json.loads(line) for line in path.read_text().splitlines(keepends=True) if line.endswith('\n')]


def run(binary: Path, provider: Path, output: Path, entry: Path | None = None) -> None:
  output.mkdir(parents=True, exist_ok=False)
  if free_for_growth(output, 512 * 1024**2) < 512 * 1024**2:
    raise RuntimeError('insufficient space for owned bootstrap package')
  rows = []
  for scenario in ('ready', 'package-corrupt', 'metadata-mismatch', 'installer-unlisted', 'installer-missing',
                   'agnos-current', 'agnos-activate', 'agnos-ui-failed'):
    root = output / scenario
    repo, data, temporary, system = [root / name for name in ('repo', 'data', 'tmp', 'system')]
    for path in (repo / 'rust/bin', data, temporary, system / 'usr/comma', system / 'sys/firmware/devicetree/base'):
      path.mkdir(parents=True)
    (system / 'usr/comma/setup_keys').write_text('owned fixture keys\n')
    (system / 'sys/firmware/devicetree/base/model').write_bytes(b'comma tici\0')
    if scenario.startswith('agnos'):
      (system / 'AGNOS').touch()
      (system / 'VERSION').write_text('19.8-carrot-bt1\n' if scenario == 'agnos-current' else 'old\n')
    for role in (*ROLES, 'openpilot-launch'):
      os.link(binary if role == 'openpilot-launch' else provider, repo / 'rust/bin' / role)
    (repo / 'build.json').write_text(json.dumps({'openpilot': {'git_commit': COMMIT if scenario != 'metadata-mismatch' else 'stale'}}))
    manifest = {
      'schema': 1,
      'source_commit': COMMIT,
      'agnos_version': '19.8-carrot-bt1',
      'files': [
        {'path': str(path.relative_to(repo)), 'sha256': hashlib.sha256(path.read_bytes()).hexdigest(), 'executable': True}
        for path in (repo / 'rust/bin').iterdir()
        if scenario != 'installer-unlisted' or path.name != 'openpilot-runtime-install'
      ],
    }
    (repo / 'rust/runtime-manifest.json').write_text(json.dumps(manifest))
    if scenario == 'installer-missing':
      (repo / 'rust/bin/openpilot-runtime-install').unlink()
    if scenario == 'package-corrupt':
      replacement = repo / 'rust/bin/corrupt'
      replacement.write_bytes(b'wrong generation')
      replacement.chmod(0o755)
      replacement.replace(repo / 'rust/bin/openpilot-manager')
    paths_file = root / 'paths.json'
    paths_file.write_text(json.dumps({'repository': str(repo), 'data': str(data), 'temporary': str(temporary), 'system': str(system)}))
    event_file = root / 'events.jsonl'
    environment = os.environ.copy()
    environment.pop('LD_LIBRARY_PATH', None)
    inherited_libraries = [str(root / 'explicit-libraries')] if scenario == 'agnos-current' else []
    if inherited_libraries:
      environment['LD_LIBRARY_PATH'] = os.pathsep.join(inherited_libraries)
    elif scenario == 'agnos-activate':
      environment['LD_LIBRARY_PATH'] = ''
    for key in ('CARROT_BOOT_LOCK_SOCKET', 'CARROT_BOOT_LOCK_FD', 'CARROT_REPO_LOCK_PATH', 'CARROT_WEB_PID_FILE', 'CARROT_WEB_LOCK_FILE', 'CARROT_DATA_DIR'):
      environment.pop(key, None)
    environment.update(
      {'OWNED_BOOTSTRAP_RECORD': str(event_file), 'OWNED_VERIFY': 'ready' if scenario == 'agnos-activate' else 'missing', 'CARROT_WEB_EXTERNAL': '1'}
    )
    if entry is not None:
      installed_entry = repo / 'launch_openpilot.sh'
      shutil.copy2(entry, installed_entry)
      command = ['bash', str(installed_entry), '--paths', str(paths_file)]
    else:
      command = [str(binary), '--paths', str(paths_file)]
    with (root / 'launcher.log').open('wb') as log:
      child = subprocess.Popen(command, env=environment, cwd=repo if entry is not None else None, stdout=log, stderr=log)
      try:
        deadline = time.monotonic() + 20
        while True:
          observed = records(event_file)
          roles = [row['role'] for row in observed]
          if scenario in ('ready', 'agnos-current'):
            complete = 'manager-lock-released' in roles and any(
              row.get('args', [None])[0] == '--ensure-if-egpu' for row in observed if row['role'] == 'openpilot-usbgpu-model'
            )
          elif scenario == 'agnos-activate':
            complete = any(row['role'] == 'sudo' and row['args'] == ['reboot'] for row in observed)
          else:
            complete = 'openpilot-text-window' in roles
          if complete and 'openpilot-carrot-recovery' in roles:
            break
          assert child.poll() is None, (scenario, (root / 'launcher.log').read_text())
          assert time.monotonic() < deadline, (scenario, observed, (root / 'launcher.log').read_text())
          time.sleep(0.02)
        assert all(not row.get('inherited_lock', False) for row in observed), observed
        assert 'openpilot-runtime-install' not in roles, observed
        expected_libraries = [str(repo / 'rust/bin/lib'), *inherited_libraries]
        assert all(row['library_path'] == expected_libraries for row in observed if 'library_path' in row), (scenario, observed)
        if scenario in ('ready', 'agnos-current'):
          assert roles.index('openpilot-usbgpu-model') < roles.index('openpilot-manager'), observed
          assert (data / 'params/d/SshEnabled').read_bytes() == b'1'
          with (temporary / 'carrot_repo_update.lock').open('r') as lock:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        else:
          assert 'openpilot-manager' not in roles, observed
          assert 'openpilot-usbgpu-model' not in roles, observed
        if scenario in ('installer-unlisted', 'installer-missing'):
          message = next(row for row in observed if row['role'] == 'openpilot-text-window')['args'][-1]
          if scenario == 'installer-unlisted':
            assert 'required executable absent from runtime manifest: rust/bin/openpilot-runtime-install' in message, message
            assert (repo / 'rust/bin/openpilot-runtime-install').is_file()
          else:
            assert 'No such file or directory' in message, message
            assert not (repo / 'rust/bin/openpilot-runtime-install').exists()
        if scenario == 'agnos-ui-failed':
          assert roles.count('openpilot-agnos-updater') == 3, observed
          assert next(row for row in observed if row['role'] == 'openpilot-agnos')['args'][-1].endswith('agnos-tici.json')
        if scenario == 'agnos-current':
          assert 'openpilot-agnos' not in roles, observed
        child.terminate()
        assert child.wait(timeout=5) == 0
        rows.append({'scenario': scenario, 'pass': True, 'events': len(observed), 'process_reaped': True})
        (root / 'invocation.json').write_text(json.dumps({'argv': command, 'pid': child.pid, 'exit': child.returncode}) + '\n')
      finally:
        if child.poll() is None:
          child.kill()
          child.wait(timeout=5)
  (output / 'result.json').write_text(
    json.dumps(
      {
        'scenarios': rows,
        'launcher_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
        'provider_sha256': hashlib.sha256(provider.read_bytes()).hexdigest(),
        'entry_sha256': hashlib.sha256(entry.read_bytes()).hexdigest() if entry is not None else None,
        'scope': 'real launcher with owned command providers; not full runtime or hardware acceptance',
      },
      indent=2,
    )
    + '\n'
  )
  print(f'PASS: {len(rows)} owned bootstrap scenarios')


if __name__ == '__main__':
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--provider', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--entry', type=Path)
  args = parser.parse_args()
  run(args.binary.resolve(), args.provider.resolve(), args.output.resolve(), args.entry.resolve() if args.entry is not None else None)
