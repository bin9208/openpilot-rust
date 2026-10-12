#!/usr/bin/env python3
# /// script
# requires-python = ">=3.12"
# dependencies = ["scons==4.11.1", "pycryptodome==3.23.0"]
# ///
"""Build unchanged Panda firmware in an owned snapshot; never flash a device."""
from __future__ import annotations

import argparse
from dataclasses import asdict, dataclass
import hashlib
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess
import sys
from typing import Final

from Crypto.PublicKey import RSA

NAMES: Final = (
  'panda.bin.signed', 'panda_h7.bin.signed',
  'bootstub.panda.bin', 'bootstub.panda_h7.bin',
)


@dataclass(frozen=True, slots=True)
class Artifact:
  name: str
  sha256: str
  size: int


def digest(path: Path) -> str:
  return hashlib.sha256(path.read_bytes()).hexdigest()


def inputs(root: Path) -> dict[str, str]:
  return {str(p.relative_to(root)): digest(p)
          for directory in ('panda', 'opendbc/safety')
          for p in sorted((root / directory).rglob('*'))
          if p.is_file() and p.suffix not in ('.o', '.dblite') and not set(p.relative_to(root).parts).intersection(
            {'obj', 'tests', '__pycache__', '.git'})}


def guard(root: Path) -> int:
  free = shutil.disk_usage(root).free
  if free < 25 * 1024**3 + 512 * 1024**2:
    raise OSError('need 25 GiB plus 512 MiB; recover 35 GiB before resuming')
  return free


def verify(folder: Path, public_key: Path) -> list[Artifact]:
  """Check the source SETLEN/VERS/raw-SHA1 RSA format using the public key."""
  key = RSA.import_key(public_key.read_bytes())
  if key.size_in_bits() != 1024:
    raise RuntimeError('Panda source public key must be RSA1024')
  artifacts = []
  for name in NAMES:
    path = folder / name
    data = path.read_bytes()
    if path.is_symlink() or not path.is_file() or not data:
      raise RuntimeError(f'firmware must be a real nonempty file: {path}')
    if name.endswith('.signed'):
      raw = path.with_suffix('').read_bytes()
      body = raw + b'VERS' + struct.pack('<I', 2)
      expected = struct.pack('<I', len(body)) + body[4:]
      encoded = b'\x00\x01' + b'\xff' * 0x69 + b'\x00' + hashlib.sha1(body[4:]).digest()
      decoded = pow(int.from_bytes(data[-128:], 'big'), key.e, key.n).to_bytes(128, 'big')
      if data[:-128] != expected or decoded != encoded:
        raise RuntimeError(f'firmware source bytes or signature mismatch: {name}')
    else:
      stack, reset = struct.unpack_from('<II', data)
      if not 0x20000000 <= stack < 0x30000000 or not 0x08000000 < reset < 0x08020000 or reset % 2 != 1:
        raise RuntimeError(f'bootstub Cortex-M vectors invalid: {name}')
    artifacts.append(Artifact(name, digest(path), len(data)))
  return artifacts


def stage(root: Path, output: Path) -> None:
  """Admit only unchanged source-bound firmware, preserving existing artifacts."""
  guard(root)
  report = json.loads((output / 'receipt.json').read_text())
  if inputs(root) != report['source_inputs'] or inputs(output / 'source') != report['source_inputs']:
    raise RuntimeError('firmware source changed since the build')
  artifacts = verify(output / 'source/panda/board/obj', root / 'panda/certs/debug.pub')
  if [asdict(a) for a in artifacts] != report['artifacts']:
    raise RuntimeError('firmware artifacts changed since the build')
  destinations = [(output / 'source/panda/board/obj' / a.name,
                   root / 'panda/board/obj' / a.name) for a in artifacts]
  license_root = root / 'rust/bin/licenses/panda'
  destinations += [(root / 'panda/LICENSE', license_root / 'LICENSE'),
                   (output / 'receipt.json', license_root / 'provenance.json')]
  for source, destination in destinations:
    if any(p.is_symlink() for p in destination.parents):
      raise RuntimeError(f'package directory symlink: {destination}')
    if destination.is_symlink() or (destination.exists() and digest(destination) != digest(source)):
      raise RuntimeError(f'conflicting package artifact: {destination}')
  for source, destination in destinations:
    destination.parent.mkdir(parents=True, exist_ok=True)
    if not destination.exists():
      shutil.copyfile(source, destination)


def build(root: Path, output: Path, compiler: Path) -> None:
  free = guard(output.parent)
  if output.exists():
    raise FileExistsError(output)
  output.mkdir()
  source = output / 'source'
  before = inputs(root)
  for directory in ('panda', 'opendbc/safety'):
    shutil.copytree(root / directory, source / directory,
                    ignore=shutil.ignore_patterns('obj', 'tests', '__pycache__', '.git'))
  (source / 'panda/board/obj').mkdir()
  if inputs(source) != before:
    raise RuntimeError('firmware source snapshot mismatch')
  env = dict(os.environ)
  for name in ('RELEASE', 'CERT', 'DEBUG', 'ENABLE_SPI', 'FINAL_PROVISIONING'):
    env.pop(name, None)
  env['PATH'] = str(compiler / 'bin') + os.pathsep + str(Path(sys.executable).parent) + os.pathsep + env['PATH']
  version = subprocess.check_output([str(compiler / 'bin/arm-none-eabi-gcc'), '--version'], text=True)
  if '13.2.1 20231009' not in version:
    raise RuntimeError('require the uv.lock-pinned Arm GNU Toolchain 13.2.1')
  command = [sys.executable, '-m', 'SCons', '-C', str(source / 'panda'),
             '--minimal', '-j2', *['board/obj/' + n for n in NAMES]]
  with (output / 'build.log').open('w') as log:
    result = subprocess.run(command, env=env, stdout=log, stderr=subprocess.STDOUT, check=False)
  (output / 'command.json').write_text(json.dumps({'argv': command, 'exit': result.returncode,
                                                 'free_before': free, 'compiler': version}, indent=2) + '\n')
  result.check_returncode()
  artifacts = verify(source / 'panda/board/obj', root / 'panda/certs/debug.pub')
  if before != inputs(root) or before != inputs(source):
    raise RuntimeError('firmware source mutated during build')
  record = {'source_inputs': before, 'source_unchanged': True, 'artifacts': [asdict(a) for a in artifacts],
            'firmware_version': (source / 'panda/board/obj/version').read_text(),
            'compiler_version': version, 'command': command,
            'compiler_sha256': digest(compiler / 'bin/arm-none-eabi-gcc'),
            'signing': 'unchanged source default DEBUG/ALLOW_DEBUG with existing debug certificate; RSA1024 raw SHA1, SETLEN=1, VERS=2',
            'scope': 'unchanged independent safety firmware; built and verified only, never flashed or connected'}
  (output / 'receipt.json').write_text(json.dumps(record, indent=2) + '\n')


def main() -> None:
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument('--root', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--compiler', type=Path)
  parser.add_argument('--stage', action='store_true')
  args = parser.parse_args()
  if args.stage:
    stage(args.root.resolve(), args.output.resolve())
  else:
    if args.compiler is None:
      parser.error('--compiler is required for a build')
    build(args.root.resolve(), args.output.resolve(), args.compiler.resolve())


if __name__ == '__main__':
  main()
