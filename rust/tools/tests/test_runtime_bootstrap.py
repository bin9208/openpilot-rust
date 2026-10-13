import hashlib
import os
from pathlib import Path
import shutil
import subprocess

import pytest


SCRIPT = Path(__file__).resolve().parents[1] / 'ensure_runtime.sh'


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
cp "$FIXTURE_REMOTE/${url##*/}" "$output"
''')
  environment = {**os.environ, 'PATH': f'{commands}:{os.environ["PATH"]}', 'FIXTURE_ROOT': str(root), 'FIXTURE_REMOTE': str(remote)}
  return root, remote, environment


def run_bootstrap(checkout):
  root, _, environment = checkout
  return subprocess.run(['bash', str(SCRIPT), '--root', str(root)], env=environment, capture_output=True, text=True, timeout=20)


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
