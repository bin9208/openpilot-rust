from __future__ import annotations

import hashlib
import gzip
import io
import json
from pathlib import Path
import struct
import subprocess
import sys
import tarfile

import pytest

from seal_runtime_package import BOOT_BINARIES


TOOLS = Path(__file__).resolve().parents[1]


def elf_fixture(*, machine: int = 183, interpreter: bool = False, needed: bool = False) -> bytes:
  """An ELF parser fixture, never an executable runtime substitute."""
  header = bytearray(64)
  header[:7] = b'\x7fELF\x02\x01\x01'
  struct.pack_into('<HHI', header, 16, 2, machine, 1)
  struct.pack_into('<Q', header, 32, 64)
  count = 2 if interpreter or needed else 1
  struct.pack_into('<HHH', header, 52, 64, 56, count)
  offset = 64 + count * 56
  load = struct.pack('<IIQQQQQQ', 1, 5, offset, 0, 0, 16, 16, 8)
  program = struct.pack('<IIQQQQQQ', 3 if interpreter else 2, 5, offset, 0, 0, 16, 16, 8) if count == 2 else b''
  return bytes(header) + load + program + struct.pack('<QQ', 1 if needed else 0, 0)


def git(root: Path, *args: str) -> str:
  return subprocess.check_output(['git', *args], cwd=root, text=True).strip()


@pytest.fixture
def release_source(tmp_path: Path) -> tuple[Path, Path, Path, Path]:
  root = tmp_path / 'source'
  root.mkdir()
  git(root, 'init', '-q')
  git(root, 'config', 'user.name', 'Owned fixture')
  git(root, 'config', 'user.email', 'fixture@example.invalid')
  git(root, 'remote', 'add', 'origin', 'https://github.com/bin9208/openpilot-rust.git')
  (root / '.gitignore').write_text('rust/bin/\nrust/runtime-manifest.json\n')
  (root / 'source.txt').write_text('owned release test input\n')
  git(root, 'add', '.')
  git(root, 'commit', '-qm', 'owned release fixture')
  binary_dir = root / 'rust/bin'
  binary_dir.mkdir(parents=True)
  for name in BOOT_BINARIES:
    binary = binary_dir / name
    binary.write_bytes(elf_fixture())
    binary.chmod(0o755)
  asset = binary_dir / 'usbgpu-assets/kernel'
  asset.parent.mkdir()
  asset.write_bytes(b'owned parser fixture data')
  (binary_dir / 'kernel-alias').symlink_to('usbgpu-assets/kernel')
  rows = [{'path': str(path.relative_to(root)), 'sha256': hashlib.sha256(path.read_bytes()).hexdigest(),
           'executable': path.name in BOOT_BINARIES}
          for path in sorted(binary_dir.rglob('*')) if path.is_file()]
  manifest = {'schema': 1, 'source_commit': git(root, 'rev-parse', 'HEAD'), 'agnos_version': '19.8-carrot-bt1', 'files': rows}
  (root / 'rust/runtime-manifest.json').write_text(json.dumps(manifest) + '\n')
  installer = tmp_path / 'installer'
  installer.write_bytes(elf_fixture())
  installer.chmod(0o755)
  receipt = tmp_path / 'receipt.json'
  receipt.write_text(json.dumps({'schema': 1, 'source_commit': manifest['source_commit'], 'architecture': 'aarch64',
                               'target': 'aarch64-unknown-linux-musl', 'installer_sha256': hashlib.sha256(installer.read_bytes()).hexdigest(),
                               'build_command': 'owned ELF parser fixture; not a production build'}) + '\n')
  return root, installer, receipt, tmp_path / 'release'


def package_cli(source: tuple[Path, Path, Path, Path], *extra: str) -> subprocess.CompletedProcess[str]:
  root, installer, receipt, output = source
  return subprocess.run([sys.executable, str(TOOLS / 'package_runtime_release.py'), '--root', str(root), '--installer', str(installer),
                         '--installer-build-receipt', str(receipt), '--output', str(output), *extra], capture_output=True, text=True, check=False)


def edit_manifest(root: Path, mutate) -> None:
  path = root / 'rust/runtime-manifest.json'
  value = json.loads(path.read_text())
  mutate(value)
  path.write_text(json.dumps(value) + '\n')


def test_cli_packages_deterministically_and_preserves_layout(release_source: tuple[Path, Path, Path, Path]) -> None:
  root, _, _, output = release_source
  result = package_cli(release_source)
  assert result.returncode == 0, result.stderr
  descriptor = json.loads((output / 'runtime-release.json').read_text())
  assert descriptor['source_commit'] == git(root, 'rev-parse', 'HEAD')
  assert descriptor['architecture'] == 'aarch64'
  archive = output / descriptor['archive']['name']
  assert descriptor['archive']['sha256'] == hashlib.sha256(archive.read_bytes()).hexdigest()
  assert descriptor['archive']['bytes'] == archive.stat().st_size
  with tarfile.open(archive) as bundle:
    assert bundle.getmember('rust/bin/openpilot-launch').mode == 0o755
    assert bundle.getmember('rust/bin/kernel-alias').linkname == 'usbgpu-assets/kernel'
    assert bundle.getmember('rust/runtime-manifest.json').mode == 0o644
    assert all(member.uid == member.gid == 0 for member in bundle)
  second = output.with_name('repeat')
  again = package_cli((root, release_source[1], release_source[2], second))
  assert again.returncode == 0, again.stderr
  assert archive.read_bytes() == (second / archive.name).read_bytes()
  assert (output / 'runtime-release.json').read_bytes() == (second / 'runtime-release.json').read_bytes()
  verified = package_cli(release_source, '--verify-only')
  assert verified.returncode == 0, verified.stderr


@pytest.mark.parametrize('fault', ['dirty', 'commit', 'hash', 'mode', 'unexpected', 'escape', 'duplicate', 'boot', 'wrong-repo', 'installer-arch',
                                  'installer-interpreter', 'installer-needed', 'receipt-commit', 'receipt-hash', 'oversized'])
def test_cli_rejects_invalid_input_before_creating_release(release_source: tuple[Path, Path, Path, Path], fault: str) -> None:
  root, installer, receipt, output = release_source
  binary = root / 'rust/bin/openpilot-launch'
  if fault == 'dirty':
    (root / 'source.txt').write_text('dirty source')
  elif fault == 'commit':
    edit_manifest(root, lambda value: value.update(source_commit='f' * 40))
  elif fault == 'hash':
    binary.write_bytes(elf_fixture() + b'tamper')
  elif fault == 'mode':
    binary.chmod(0o644)
  elif fault == 'unexpected':
    (root / 'rust/bin/unsealed').write_bytes(b'unsealed')
  elif fault == 'escape':
    binary.unlink()
    binary.symlink_to('/etc/passwd')
  elif fault == 'duplicate':
    edit_manifest(root, lambda value: value['files'].append(value['files'][0]))
  elif fault == 'boot':
    edit_manifest(root, lambda value: value.update(files=[row for row in value['files'] if row['path'] != 'rust/bin/openpilot-launch']))
    binary.unlink()
  elif fault == 'wrong-repo':
    git(root, 'remote', 'set-url', 'origin', 'https://github.com/bin9208/openpilot.git')
  elif fault.startswith('installer-'):
    installer.write_bytes(elf_fixture(machine=62 if fault == 'installer-arch' else 183,
                                      interpreter=fault == 'installer-interpreter', needed=fault == 'installer-needed'))
  elif fault.startswith('receipt-'):
    value = json.loads(receipt.read_text())
    value['source_commit' if fault == 'receipt-commit' else 'installer_sha256'] = 'f' * (40 if fault == 'receipt-commit' else 64)
    receipt.write_text(json.dumps(value))
  elif fault == 'oversized':
    with binary.open('wb') as stream:
      stream.truncate(2 * 1024**3)
  result = package_cli(release_source)
  assert result.returncode != 0
  assert not output.exists(), result.stdout


@pytest.mark.parametrize('fault', ['descriptor', 'archive', 'helper', 'sidecar', 'extra', 'archive-mode', 'archive-escape', 'archive-duplicate',
                                  'archive-footer', 'archive-hidden'])
def test_verifier_rejects_tampered_release(release_source: tuple[Path, Path, Path, Path], fault: str) -> None:
  assert package_cli(release_source).returncode == 0
  output = release_source[3]
  descriptor_path = output / 'runtime-release.json'
  descriptor = json.loads(descriptor_path.read_text())
  archive = output / descriptor['archive']['name']
  if fault == 'descriptor':
    descriptor['archive']['unpacked_bytes'] += 1
    descriptor_path.write_text(json.dumps(descriptor))
  elif fault == 'archive':
    archive.write_bytes(archive.read_bytes()[:-4])
  elif fault == 'helper':
    (output / 'openpilot-runtime-install-aarch64').write_bytes(b'changed')
  elif fault == 'sidecar':
    (output / (archive.name + '.sha256')).write_text('f' * 64 + '  ' + archive.name + '\n')
  elif fault == 'extra':
    (output / 'unreviewed-asset').write_bytes(b'extra')
  elif fault in ('archive-footer', 'archive-hidden'):
    if fault == 'archive-footer':
      archive.write_bytes(archive.read_bytes()[:-4])
    else:
      archive.write_bytes(archive.read_bytes() + gzip.compress(b'hidden unmanifested bytes'))
    descriptor['archive']['sha256'] = hashlib.sha256(archive.read_bytes()).hexdigest()
    descriptor['archive']['bytes'] = archive.stat().st_size
    descriptor_path.write_text(json.dumps(descriptor))
    (output / (archive.name + '.sha256')).write_text(descriptor['archive']['sha256'] + '  ' + archive.name + '\n')
  else:
    original = archive.read_bytes()
    timestamp = int(git(release_source[0], 'show', '-s', '--format=%ct', 'HEAD'))
    with archive.open('wb') as raw, gzip.GzipFile(fileobj=raw, mode='wb', filename='', mtime=timestamp, compresslevel=6) as compressed, \
        tarfile.open(fileobj=io.BytesIO(original)) as source, tarfile.open(fileobj=compressed, mode='w', format=tarfile.USTAR_FORMAT) as destination:
      for entry in source:
        data = source.extractfile(entry).read() if entry.isfile() else None
        if entry.name == 'rust/bin/openpilot-launch' and fault == 'archive-mode':
          entry.mode = 0o644
        destination.addfile(entry, io.BytesIO(data) if data is not None else None)
      if fault != 'archive-mode':
        added = tarfile.TarInfo('../escape' if fault == 'archive-escape' else 'rust/runtime-manifest.json')
        destination.addfile(added)
    descriptor['archive']['sha256'] = hashlib.sha256(archive.read_bytes()).hexdigest()
    descriptor['archive']['bytes'] = archive.stat().st_size
    descriptor_path.write_text(json.dumps(descriptor))
    (output / (archive.name + '.sha256')).write_text(descriptor['archive']['sha256'] + '  ' + archive.name + '\n')
  result = package_cli(release_source, '--verify-only')
  assert result.returncode != 0
  assert archive.exists()


def test_disk_floor_preserves_source_and_creates_no_release(release_source, monkeypatch) -> None:
  import shutil
  from package_runtime_release import package
  from runtime_release_support import ReleaseError
  root, installer, receipt, output = release_source
  original = (root / 'rust/runtime-manifest.json').read_bytes()
  usage = shutil.disk_usage(root)
  monkeypatch.setattr(shutil, 'disk_usage', lambda _: type(usage)(usage.total, usage.used, 1))
  with pytest.raises(ReleaseError, match='25 GiB'):
    package(root, installer, receipt, output)
  assert not output.exists()
  assert (root / 'rust/runtime-manifest.json').read_bytes() == original
