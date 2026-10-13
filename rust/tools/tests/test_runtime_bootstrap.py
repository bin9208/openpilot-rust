import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import time

import pytest


SCRIPT = Path(__file__).resolve().parents[1] / 'ensure_runtime.sh'
Checkout = tuple[Path, Path, dict[str, str]]


def executable(path: Path, contents: str) -> None:
  path.parent.mkdir(parents=True, exist_ok=True)
  path.write_text(contents)
  path.chmod(0o755)


@pytest.fixture
def checkout(tmp_path):
  root = tmp_path / 'checkout with spaces'
  root.mkdir()
  subprocess.run(['git', 'init', '-q', str(root)], check=True)
  subprocess.run(
    ['git', '-C', str(root), '-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid', 'commit', '--allow-empty', '-qm', 'fixture'],
    check=True,
  )
  remote = tmp_path / 'remote'
  remote.mkdir()
  helper = remote / 'openpilot-runtime-install-aarch64'
  executable(helper, '#!/bin/bash\nset -eu\n[[ "$1" == --root && "$2" == "$FIXTURE_ROOT" ]]\nprintf "installed\\n" > "$2/installed"\n')
  digest = hashlib.sha256(helper.read_bytes()).hexdigest()
  (remote / f'{helper.name}.sha256').write_text(f'{digest}  {helper.name}\n')
  commands = tmp_path / 'commands'
  commands.mkdir()
  executable(commands / 'curl', '''#!/bin/bash
set -eu
output=""
for ((i=1; i<=$#; i++)); do
  if [[ "${!i}" == --output ]]; then j=$((i+1)); output="${!j}"; fi
done
url="${!#}"
expected="https://github.com/bin9208/openpilot-rust/releases/download/runtime-$(git -C "$FIXTURE_ROOT" rev-parse HEAD)/"
[[ "$url" == "$expected"* ]]
printf '%s\n' "$url" >> "$FIXTURE_ROOT/requests"
if [[ -n ${FIXTURE_FAILURES:-} ]]; then
  count=$(wc -l < "$FIXTURE_ROOT/requests")
  IFS=, read -ra failures <<< "$FIXTURE_FAILURES"
  if (( count <= ${#failures[@]} )); then
    printf partial > "$output"
    printf '%s' "${FIXTURE_HTTP:-000}"
    exit "${failures[count-1]}"
  fi
fi
if [[ ! -f "$FIXTURE_REMOTE/${url##*/}" ]]; then exit 22; fi
cp "$FIXTURE_REMOTE/${url##*/}" "$output"
''')
  environment = {**os.environ, 'PATH': f'{commands}:{os.environ["PATH"]}', 'FIXTURE_ROOT': str(root), 'FIXTURE_REMOTE': str(remote)}
  return root, remote, environment


def run_bootstrap(checkout):
  root, _, environment = checkout
  return subprocess.run(['bash', str(SCRIPT), '--root', str(root)], env=environment, capture_output=True, text=True, timeout=35)


def test_fresh_checkout_downloads_verified_helper_for_exact_source(checkout):
  root, _, _ = checkout
  result = run_bootstrap(checkout)
  assert result.returncode == 0, result.stderr
  assert (root / 'installed').read_text() == 'installed\n'
  assert len((root / 'requests').read_text().splitlines()) == 2


def test_bad_helper_hash_prevents_execution(checkout):
  root, remote, _ = checkout
  (remote / 'openpilot-runtime-install-aarch64').write_text('#!/bin/bash\ntouch "$FIXTURE_ROOT/installed"\n')
  result = run_bootstrap(checkout)
  assert result.returncode != 0
  assert not (root / 'installed').exists()
  assert len((root / 'requests').read_text().splitlines()) == 2


def test_valid_installed_package_needs_no_network(checkout):
  root, _, _ = checkout
  executable(root / 'rust/bin/openpilot-launch', '#!/bin/bash\n[[ "$1" == --verify-package && "$2" == --root && "$3" == "$FIXTURE_ROOT" ]]\n')
  result = run_bootstrap(checkout)
  assert result.returncode == 0, result.stderr
  assert not (root / 'requests').exists()


def test_missing_release_keeps_existing_runtime(checkout):
  root, remote, _ = checkout
  binary = root / 'rust/bin/openpilot-launch'
  executable(binary, '#!/bin/bash\nexit 1\n')
  before = binary.read_bytes()
  (remote / 'openpilot-runtime-install-aarch64.sha256').unlink()
  result = run_bootstrap(checkout)
  assert result.returncode != 0
  assert binary.read_bytes() == before
  assert not (root / 'installed').exists()
  assert len((root / 'requests').read_text().splitlines()) == 1
  assert 'curl_exit=22' in result.stderr


@pytest.mark.parametrize('residue', ['.runtime-install-journal.json', '.runtime-install-rollback'])
def test_interrupted_install_enters_cached_installer_even_when_runtime_verifies(checkout, residue):
  root, remote, _ = checkout
  executable(root / 'rust/bin/openpilot-launch', '#!/bin/bash\nexit 0\n')
  (root / 'rust' / residue).touch()
  commit = subprocess.check_output(['git', '-C', str(root), 'rev-parse', 'HEAD'], text=True).strip()
  cache = root / 'rust/.runtime-bootstrap' / commit
  cache.mkdir(parents=True)
  for name in ['openpilot-runtime-install-aarch64', 'openpilot-runtime-install-aarch64.sha256']:
    shutil.copy2(remote / name, cache / name)
  result = run_bootstrap(checkout)
  assert result.returncode == 0, result.stderr
  assert (root / 'installed').is_file()
  assert not (root / 'requests').exists()


@pytest.mark.parametrize('failures,http_status', [('6,7,60,18', '000'), ('22,22,22,22', '503')])
def test_network_and_clock_recovery_outlasts_old_retry_budget(checkout: Checkout, failures: str, http_status: str) -> None:
  root, _, environment = checkout
  environment['FIXTURE_FAILURES'] = failures
  environment['FIXTURE_HTTP'] = http_status
  started = time.monotonic()
  result = run_bootstrap(checkout)
  assert result.returncode == 0, result.stderr
  assert time.monotonic() - started >= 20
  assert (root / 'installed').read_text() == 'installed\n'
  assert len((root / 'requests').read_text().splitlines()) == 6
  assert 'retry' in result.stdout.lower()
  assert not list((root / 'rust/.runtime-bootstrap').glob('*/download.*'))


@pytest.mark.parametrize('phase', ['download', 'backoff'])
def test_cancelling_download_reaps_child_and_never_executes_partial(checkout: Checkout, phase: str) -> None:
  root, _, environment = checkout
  commands = Path(environment['PATH'].split(':')[0])
  executable(commands / 'curl', """#!/bin/bash
set -eu
printf '%s' "$$" > "$FIXTURE_ROOT/download-pid"
trap 'exit 143' TERM
for ((i=1; i<=$#; i++)); do
  if [[ "${!i}" == --output ]]; then j=$((i+1)); printf partial > "${!j}"; fi
done
if [[ "$FIXTURE_PHASE" == backoff ]]; then exit 6; fi
while :; do sleep 0.05; done
""")
  environment['FIXTURE_PHASE'] = phase
  process = subprocess.Popen(['bash', str(SCRIPT), '--root', str(root)], env=environment, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
  try:
    deadline = time.monotonic() + 5
    while not (root / 'download-pid').exists() and time.monotonic() < deadline:
      time.sleep(0.01)
    assert (root / 'download-pid').exists()
    child = int((root / 'download-pid').read_text())
    if phase == 'backoff':
      assert 'retry_seconds=' in process.stdout.readline()
    process.terminate()
    process.communicate(timeout=5)
    assert process.returncode == 143
    assert not Path(f'/proc/{child}').exists()
    assert not (root / 'installed').exists()
    assert not list((root / 'rust/.runtime-bootstrap').glob('*/download.*'))
  finally:
    if process.poll() is None:
      process.kill()
      process.communicate()
    if (root / 'download-pid').exists():
      child = int((root / 'download-pid').read_text())
      if Path(f'/proc/{child}').exists():
        os.kill(child, 15)


def test_cached_verified_helper_runs_without_network(checkout: Checkout) -> None:
  root, remote, _ = checkout
  commit = subprocess.check_output(['git', '-C', str(root), 'rev-parse', 'HEAD'], text=True).strip()
  cache = root / 'rust/.runtime-bootstrap' / commit
  cache.mkdir(parents=True)
  for name in ['openpilot-runtime-install-aarch64', 'openpilot-runtime-install-aarch64.sha256']:
    shutil.copy2(remote / name, cache / name)
  result = run_bootstrap(checkout)
  assert result.returncode == 0, result.stderr
  assert (root / 'installed').read_text() == 'installed\n'
  assert not (root / 'requests').exists()
