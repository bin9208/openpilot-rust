#!/usr/bin/env python3
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Create deterministic exact-source assets for the GitHub runtime release.

Run: python3 rust/tools/package_runtime_release.py --root ROOT --installer ELF
     --installer-build-receipt RECEIPT --output NEW_DIRECTORY
Verify without network: --root ROOT --output DIRECTORY --verify-only
"""

from __future__ import annotations

import argparse
import gzip
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile

from runtime_release_support import (
  AGNOS, INSTALLER, MAX_ASSET, MAX_METADATA, MAX_SOURCE_ASSETS, MAX_UNPACKED, RECEIPT, SOURCE_ASSETS, JSON, Member, ReleaseError,
  build_receipt, clean_source, disk_guard, hex_digest, integer, json_file, mapping, native, require, safe_name, sequence, sha256, sidecar, string,
)
from seal_runtime_package import BOOT_BINARIES


def sealed_members(root: Path, commit: str) -> dict[str, Member]:
  """Verify every sealed source file before copying any bytes."""
  manifest_path = root / 'rust/runtime-manifest.json'
  manifest = json_file(manifest_path)
  require(type(manifest.get('schema')) is int and manifest.get('schema') == 1 and manifest.get('source_commit') == commit
          and manifest.get('agnos_version') == AGNOS,
          'sealed manifest identity mismatch')
  rows = sequence(manifest.get('files'))
  require(0 < len(rows) <= 100000, 'invalid manifest file count')
  members: dict[str, Member] = {}
  for raw in rows:
    row = mapping(raw)
    name = safe_name(string(row.get('path')))
    require(name.startswith('rust/bin/') or name in SOURCE_ASSETS, f'unexpected manifest source asset: {name}')
    require(name not in members and type(row.get('executable')) is bool, f'duplicate or invalid manifest member: {name}')
    path = root / name
    require(path.resolve(strict=True).is_relative_to(root) and path.is_file(), f'artifact escapes repository or is not a file: {name}')
    require(not any(parent.is_symlink() for parent in path.parents if parent != root and parent.is_relative_to(root)), f'directory alias: {name}')
    size = path.stat().st_size
    require(0 < size < MAX_ASSET, f'asset exceeds 2 GiB limit or is empty: {name}')
    mode = path.stat().st_mode & 0o7777
    require(mode <= 0o777 and (not row['executable'] or bool(mode & 0o111)), f'unsafe or missing executable mode: {name}')
    digest = hex_digest(row.get('sha256'))
    require(sha256(path) == digest, f'sealed artifact hash mismatch: {name}')
    with path.open('rb') as stream:
      magic = stream.read(4)
    if row['executable'] or magic == b'\x7fELF':
      native(path)
    link = os.path.relpath(path.resolve(strict=True), path.parent) if path.is_symlink() else None
    if link is not None:
      require(not Path(link).is_absolute() and '\\' not in link and '\x00' not in link, f'invalid archive symlink: {name}')
    members[name] = Member(name, digest, size, mode, link)
  require(sum(member.size for name, member in members.items() if name in SOURCE_ASSETS) <= MAX_SOURCE_ASSETS,
          'source assets exceed 128 MiB activation limit')
  for member in members.values():
    if member.link is not None:
      target = str((root / member.name).resolve(strict=True).relative_to(root))
      require(target in members and members[target].link is None and members[target].sha256 == member.sha256,
              f'symlink target not a sealed regular member: {member.name}')
  for boot in BOOT_BINARIES:
    name = f'rust/bin/{boot}'
    require(name in members and bool(members[name].mode & 0o111), f'missing sealed boot executable: {boot}')
  require(any(name.startswith('rust/bin/usbgpu-assets/') for name in members), 'missing sealed GPU bootstrap assets')
  actual = {str(path.relative_to(root)) for path in (root / 'rust/bin').rglob('*') if not path.is_dir()}
  require(actual == {name for name in members if name.startswith('rust/bin/')}, 'unsealed or unsupported rust/bin artifact')
  members['rust/runtime-manifest.json'] = Member('rust/runtime-manifest.json', sha256(manifest_path), manifest_path.stat().st_size, 0o644)
  require(sum(member.size for member in members.values() if member.link is None) <= MAX_UNPACKED, 'runtime exceeds 8 GiB unpacked limit')
  return members


def verify_archive(archive: Path, members: dict[str, Member], timestamp: int) -> int:
  """Stream the complete archive and reject unexpected members and metadata."""
  seen: set[str] = set()
  unpacked = 0
  with archive.open('rb') as raw:
    require(raw.read(10) == b'\x1f\x8b\x08\x00' + timestamp.to_bytes(4, 'little') + b'\x00\xff', 'noncanonical gzip header')
  with gzip.open(archive, 'rb') as compressed, tarfile.open(fileobj=compressed, mode='r|') as bundle:
    for entry in bundle:
      name = safe_name(entry.name)
      require(name in members and name not in seen, f'unexpected or duplicate archive member: {name}')
      seen.add(name)
      expected = members[name]
      require(entry.mode == expected.mode and entry.uid == entry.gid == 0 and entry.mtime == timestamp
              and not entry.uname and not entry.gname, f'archive mode/identity mismatch: {name}')
      require(not entry.pax_headers, f'unexpected extended archive metadata: {name}')
      if expected.link is not None:
        require(entry.issym() and entry.linkname == expected.link and entry.size == 0, f'archive symlink mismatch: {name}')
      else:
        require(entry.isfile() and entry.size == expected.size and 0 < entry.size < MAX_ASSET, f'archive member type/size mismatch: {name}')
        stream = bundle.extractfile(entry)
        require(stream is not None, f'unreadable archive member: {name}')
        if stream is not None:
          with stream:
            require(hashlib.file_digest(stream, 'sha256').hexdigest() == expected.sha256, f'archive member hash mismatch: {name}')
        unpacked += entry.size
    padding = 0
    while tail := bundle.fileobj.read(10240):
      padding += len(tail)
      require(padding <= 10240 and not tail.strip(b'\x00'), 'unexpected trailing archive content')
  require(seen == members.keys(), 'missing archive members')
  require(unpacked <= MAX_UNPACKED, 'runtime exceeds 8 GiB unpacked limit')
  return unpacked


def validate_assets(root: Path, assets: Path) -> dict[str, JSON]:
  """Validate a release directory against its clean sealed source, without network."""
  root = root.resolve(strict=True)
  commit, timestamp = clean_source(root)
  descriptor = json_file(assets / 'runtime-release.json')
  require(set(descriptor) == {'schema', 'source_commit', 'architecture', 'agnos_version', 'manifest_sha256', 'archive'}, 'invalid descriptor fields')
  require(type(descriptor.get('schema')) is int and descriptor.get('schema') == 1 and descriptor.get('source_commit') == commit
          and descriptor.get('architecture') == 'aarch64'
          and descriptor.get('agnos_version') == AGNOS, 'release descriptor identity mismatch')
  members = sealed_members(root, commit)
  require(descriptor.get('manifest_sha256') == members['rust/runtime-manifest.json'].sha256, 'release manifest digest mismatch')
  archive_info = mapping(descriptor.get('archive'))
  name = f'openpilot-rust-{commit}-aarch64.tar.gz'
  require(set(archive_info) == {'name', 'sha256', 'bytes', 'unpacked_bytes'} and archive_info.get('name') == name, 'invalid archive descriptor')
  expected_assets = {name, name + '.sha256', 'runtime-release.json', INSTALLER, INSTALLER + '.sha256', RECEIPT}
  require({path.name for path in assets.iterdir()} == expected_assets, 'unexpected or missing release assets')
  for asset in assets.iterdir():
    require(asset.is_file() and not asset.is_symlink() and 0 < asset.stat().st_size < MAX_ASSET, f'invalid release asset: {asset.name}')
  archive = assets / name
  require(integer(archive_info.get('bytes')) == archive.stat().st_size and hex_digest(archive_info.get('sha256')) == sha256(archive),
          'archive size/hash mismatch')
  require(integer(archive_info.get('unpacked_bytes')) == verify_archive(archive, members, timestamp), 'archive unpacked size mismatch')
  installer = assets / INSTALLER
  require(bool(installer.stat().st_mode & 0o111), 'installer executable mode missing')
  native(installer, static=True)
  build_receipt(assets / RECEIPT, installer, commit)
  for file in (archive, installer):
    checksum = assets / (file.name + '.sha256')
    require(checksum.stat().st_size <= MAX_METADATA and checksum.read_text() == sidecar(file), f'checksum sidecar mismatch: {file.name}')
  require(clean_source(root)[0] == commit, 'source identity changed during validation')
  return descriptor


def package(root: Path, installer: Path, receipt_path: Path, output: Path) -> dict[str, JSON]:
  root = root.resolve(strict=True)
  commit, timestamp = clean_source(root)
  members = sealed_members(root, commit)
  require(installer.is_file() and not installer.is_symlink() and bool(installer.stat().st_mode & 0o111), 'invalid installer file/mode')
  native(installer, static=True)
  receipt = build_receipt(receipt_path, installer, commit)
  require(not output.exists(), 'release output already exists')
  unpacked = sum(member.size for member in members.values() if member.link is None)
  disk_guard(output, unpacked * 21 // 10 + installer.stat().st_size + 64 * 1024**2)
  output.parent.mkdir(parents=True, exist_ok=True)
  with tempfile.TemporaryDirectory(dir=output.parent, prefix='.runtime-release-') as temporary:
    staged = Path(temporary) / 'assets'
    staged.mkdir()
    archive = staged / f'openpilot-rust-{commit}-aarch64.tar.gz'
    with archive.open('wb') as raw:
      with gzip.GzipFile(fileobj=raw, mode='wb', filename='', mtime=timestamp, compresslevel=6) as compressed:
        with tarfile.open(fileobj=compressed, mode='w|', format=tarfile.USTAR_FORMAT) as bundle:
          for member in sorted(members.values(), key=lambda member: member.name):
            entry = tarfile.TarInfo(member.name)
            entry.mode, entry.mtime = member.mode, timestamp
            if member.link is not None:
              entry.type, entry.linkname = tarfile.SYMTYPE, member.link
              bundle.addfile(entry)
            else:
              entry.size = member.size
              with (root / member.name).open('rb') as stream:
                bundle.addfile(entry, stream)
      raw.flush()
      os.fsync(raw.fileno())
    require(archive.stat().st_size < MAX_ASSET, 'archive exceeds 2 GiB asset limit')
    helper = staged / INSTALLER
    shutil.copyfile(installer, helper)
    helper.chmod(0o755)
    (staged / RECEIPT).write_text(json.dumps(receipt, sort_keys=True, indent=2) + '\n')
    for file in (archive, helper):
      (staged / (file.name + '.sha256')).write_text(sidecar(file))
    descriptor: dict[str, JSON] = {'schema': 1, 'source_commit': commit, 'architecture': 'aarch64', 'agnos_version': AGNOS,
                                'manifest_sha256': members['rust/runtime-manifest.json'].sha256,
                                'archive': {'name': archive.name, 'sha256': sha256(archive), 'bytes': archive.stat().st_size, 'unpacked_bytes': unpacked}}
    (staged / 'runtime-release.json').write_text(json.dumps(descriptor, sort_keys=True, indent=2) + '\n')
    validate_assets(root, staged)
    staged.rename(output)
  return descriptor


def main() -> None:
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument('--root', type=Path, required=True)
  parser.add_argument('--installer', type=Path)
  parser.add_argument('--installer-build-receipt', type=Path)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--verify-only', action='store_true')
  args = parser.parse_args()
  try:
    if args.verify_only:
      descriptor = validate_assets(args.root, args.output)
    else:
      require(args.installer is not None and args.installer_build_receipt is not None, 'installer and build receipt are required')
      descriptor = package(args.root, args.installer, args.installer_build_receipt, args.output)
  except (ReleaseError, OSError, EOFError, tarfile.TarError, json.JSONDecodeError, subprocess.CalledProcessError) as error:
    parser.exit(1, f'runtime release: {error}\n')
  print(json.dumps(descriptor, sort_keys=True))


if __name__ == '__main__':
  main()
