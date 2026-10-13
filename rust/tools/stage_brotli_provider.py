#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# python -P rust/tools/stage_brotli_provider.py SDK_LIB_DIR OUTPUT TARGET 1.1.0
"""Stage the three trusted SDK Brotli libraries for the native QR provider."""

from __future__ import annotations

from dataclasses import dataclass
import hashlib
import json
import os
from pathlib import Path
import shutil
import sys
import tempfile
from typing import Final, TypedDict

LIBRARIES: Final = ('libbrotlienc.so.1', 'libbrotlidec.so.1', 'libbrotlicommon.so.1')
MACHINES: Final = {'x86_64-unknown-linux-gnu': 62, 'aarch64-unknown-linux-gnu': 183}


@dataclass(frozen=True, slots=True)
class InvalidBundle(ValueError):
  reason: str

  def __str__(self) -> str:
    return self.reason


class Manifest(TypedDict):
  abi_version: int
  target: str
  brotli_version: int
  files: dict[str, str]


def stage(source: Path, output: Path, target: str, version: str) -> None:
  """Copy one same-target SDK bundle; runtime rechecks hashes, ABI and codec calls."""
  machine = MACHINES[target]
  major, minor, patch = (int(part) for part in version.split('.'))
  if major != 1 or not 0 <= minor < 4096 or not 0 <= patch < 4096:
    raise InvalidBundle('unsupported Brotli version')
  source = source.resolve(strict=True)
  files: dict[str, bytes] = {}
  for name in LIBRARIES:
    path = (source / name).resolve(strict=True)
    if not path.is_file() or not path.is_relative_to(source):
      raise InvalidBundle(f'SDK library escapes the source directory: {name}')
    raw = path.read_bytes()
    valid_elf = raw[:6] == b'\x7fELF\x02\x01' and int.from_bytes(raw[16:18], 'little') == 3
    if len(raw) > 16 * 1024 * 1024 or not valid_elf or int.from_bytes(raw[18:20], 'little') != machine:
      raise InvalidBundle(f'ELF target mismatch: {name}')
    files[name] = raw
  growth = sum(len(raw) for raw in files.values()) + 1024 * 1024
  output.parent.mkdir(parents=True, exist_ok=True)
  if shutil.disk_usage(output.parent).free < 25 * 1024**3 + growth:
    raise InvalidBundle('insufficient free space for Brotli bundle staging')
  manifest: Manifest = {
    'abi_version': 1,
    'target': target,
    'brotli_version': (major << 24) | (minor << 12) | patch,
    'files': {name: hashlib.sha256(raw).hexdigest() for name, raw in files.items()},
  }
  with tempfile.TemporaryDirectory(prefix='.brotli-', dir=output.parent) as temporary:
    root = Path(temporary)
    for name, raw in files.items():
      (root / name).write_bytes(raw)
    (root / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    if output.exists() or output.is_symlink():
      raise InvalidBundle('Brotli bundle destination already exists')
    os.rename(root, output)
  print(json.dumps({'bundle': str(output.resolve()), 'manifest': manifest, 'bytes': growth - 1024 * 1024}))


if __name__ == '__main__':
  stage(Path(sys.argv[1]), Path(sys.argv[2]), sys.argv[3], sys.argv[4])
