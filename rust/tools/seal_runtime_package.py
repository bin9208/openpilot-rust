"""Seal a staged native runtime only after every catalog requirement is present."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile


BOOT_BINARIES = (
  'openpilot-launch',
  'openpilot-manager',
  'openpilot-process-child',
  'openpilot-carrot-recovery',
  'openpilot-carrot-web-watchdog',
  'openpilot-carrot-server',
  'openpilot-agnos',
  'openpilot-agnos-updater',
  'openpilot-text-window',
  'openpilot-usbgpu-model',
  'openpilot-usbgpu-worker',
  'openpilot-usbgpu-probe',
  'openpilot-bootlog',
)
BOOT_COMPANIONS = ('usbgpu-assets',)
MACHINES = {'aarch64': 183, 'x86_64': 62}


class PackageError(ValueError):
  pass


def argument_path(value: object) -> Path:
  if isinstance(value, str):
    return Path(value)
  if isinstance(value, dict) and set(value) == {'Unix'}:
    raw = value['Unix']
    if isinstance(raw, list) and all(type(byte) is int and 0 <= byte <= 255 for byte in raw):
      return Path(os.fsdecode(bytes(raw)))
  raise PackageError('invalid manager argument encoding')


def contained(root: Path, path: Path) -> Path:
  path = path if path.is_absolute() else root / path
  if not path.resolve().is_relative_to(root):
    raise PackageError(f'artifact escapes repository: {path}')
  if not path.is_relative_to(root) or '..' in path.parts:
    raise PackageError(f'invalid artifact path: {path}')
  return path


def native(path: Path, architecture: str) -> None:
  with path.open('rb') as stream:
    header = stream.read(64)
  if len(header) != 64 or header[:6] != b'\x7fELF\x02\x01' or int.from_bytes(header[18:20], 'little') != MACHINES[architecture]:
    raise PackageError(f'expected {architecture} ELF64 artifact: {path}')


def seal(
  root: Path,
  inventory: list[dict],
  architecture: str,
  commit: str,
  *,
  required_boot: tuple[str, ...] = BOOT_BINARIES,
  required_companions: tuple[str, ...] = BOOT_COMPANIONS,
) -> dict:
  root = root.resolve(strict=True)
  if architecture not in MACHINES or len(commit) != 40 or any(char not in '0123456789abcdef' for char in commit):
    raise PackageError('invalid architecture or source commit')
  if not inventory:
    raise PackageError('empty manager inventory')
  binaries = root / 'rust/bin'
  executables = {contained(root, binaries / name) for name in required_boot}
  directories: set[Path] = set()
  required_files: set[Path] = set()
  for name in required_companions:
    companion = contained(root, binaries / name)
    if companion.is_symlink() or not companion.is_dir():
      raise PackageError(f'missing or unsupported bootstrap companion directory: {companion}')
    directories.add(companion)
  for entry in inventory:
    if entry.get('name') == 'webjoystick' and not (root / 'openpilot/tools/bodyteleop/web.py').exists() and not entry.get('enabled'):
      continue
    if not entry.get('ready') or not entry.get('argv'):
      raise PackageError(f"catalog implementation unavailable: {entry.get('name')}: {entry.get('error')}")
    executables.add(contained(root, argument_path(entry['argv'][0])))
    for requirement in entry.get('requirements', []):
      path = contained(root, Path(requirement['path']))
      if requirement.get('executable'):
        executables.add(path)
      elif requirement.get('file'):
        if not path.is_file() or path.stat().st_size == 0:
          raise PackageError(f'missing or empty required file: {path}')
        with path.open('rb') as stream:
          if stream.read(43).startswith(b'version https://git-lfs.github.com/spec/v1'):
            raise PackageError(f'Git LFS pointer instead of required file: {path}')
        required_files.add(path)
      elif requirement.get('directory'):
        if path.is_relative_to(binaries) and path.is_symlink():
          raise PackageError(f'unsupported directory symlink: {path}')
        if not path.is_dir():
          raise PackageError(f'missing companion or source directory: {path}')
        if path.is_relative_to(binaries):
          directories.add(path)
  for executable in executables:
    if not executable.is_file() or executable.stat().st_mode & 0o111 == 0:
      raise PackageError(f'missing native executable: {executable}')
    native(executable, architecture)
  for directory in directories:
    if not any(path.is_file() for path in directory.rglob('*')):
      raise PackageError(f'empty companion directory: {directory}')
  files = []
  for path in sorted(set(binaries.rglob('*')) | required_files):
    contained(root, path)
    if path.is_dir():
      if path.is_symlink():
        raise PackageError(f'unsupported directory symlink: {path}')
      continue
    if not path.is_file():
      raise PackageError(f'unsupported artifact type: {path}')
    with path.open('rb') as stream:
      magic = stream.read(4)
      stream.seek(0)
      digest = hashlib.file_digest(stream, 'sha256').hexdigest()
    if magic == b'\x7fELF':
      native(path, architecture)
    files.append({'path': str(path.relative_to(root)), 'sha256': digest, 'executable': path in executables})
  listed = {row['path'] for row in files}
  if any(str(path.relative_to(root)) not in listed for path in executables):
    raise PackageError('catalog executable is outside rust/bin')
  manifest = {'schema': 1, 'source_commit': commit, 'agnos_version': '19.8-carrot-bt1', 'files': files}
  destination = root / 'rust/runtime-manifest.json'
  with tempfile.NamedTemporaryFile(mode='w', dir=destination.parent, delete=False) as output:
    temporary = Path(output.name)
    try:
      json.dump(manifest, output, indent=2)
      output.write('\n')
      output.flush()
      os.fsync(output.fileno())
      output.close()
      temporary.replace(destination)
    finally:
      temporary.unlink(missing_ok=True)
  return manifest


def main() -> None:
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument('--root', type=Path, required=True)
  parser.add_argument('--manager', type=Path, required=True, help='Host manager built from the same source, used for catalog inspection')
  parser.add_argument('--architecture', choices=tuple(MACHINES), required=True)
  args = parser.parse_args()
  root = args.root.resolve(strict=True)
  status = subprocess.check_output(['git', 'status', '--porcelain', '--untracked-files=normal'], cwd=root, text=True)
  if status:
    raise PackageError('source checkout must be clean before sealing a runtime candidate')
  commit = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
  inventory = json.loads(subprocess.check_output([str(args.manager.resolve(strict=True)), '--root', str(root), '--inspect'], text=True))
  manifest = seal(root, inventory, args.architecture, commit)
  print(json.dumps({'source_commit': commit, 'architecture': args.architecture, 'artifacts': len(manifest['files'])}))


if __name__ == '__main__':
  main()
