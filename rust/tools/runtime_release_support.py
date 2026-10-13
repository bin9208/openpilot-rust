"""Shared fail-closed validation for exact-source runtime release assets."""

from __future__ import annotations

from dataclasses import dataclass
import hashlib
import json
from pathlib import Path, PurePosixPath
import shutil
import struct
import subprocess
from typing import Final, TypeAlias

JSON: TypeAlias = None | bool | int | float | str | list['JSON'] | dict[str, 'JSON']
REPOSITORY: Final = 'bin9208/openpilot-rust'
AGNOS: Final = '19.8-carrot-bt1'
MAX_ASSET: Final = 2 * 1024**3
MAX_UNPACKED: Final = 8 * 1024**3
MAX_METADATA: Final = 4 * 1024**2
INSTALLER: Final = 'openpilot-runtime-install-aarch64'
RECEIPT: Final = 'runtime-installer-build.json'
SOURCE_ASSETS: Final = frozenset({
  'openpilot/selfdrive/assets/fonts/KaiGenGothicKR-Bold.ttf',
  'openpilot/selfdrive/assets/icons_mici/carrot_cruse_gap_trimmed.png',
  'openpilot/selfdrive/assets/icons_mici/carrot_wheel_lane.png',
  'openpilot/selfdrive/assets/icons_mici/carrot_wheel_org.png',
  'openpilot/selfdrive/assets/icons_mici/settings/network/wifi_strength_full.png',
  'openpilot/selfdrive/assets/images/speed_bg.png',
  'openpilot/selfdrive/assets/images/traffic_green.png',
  'openpilot/selfdrive/assets/images/traffic_red.png',
  'openpilot/selfdrive/carrot/cluster/assets/images/tpms_toy_car.png',
  'openpilot/selfdrive/carrot/web/assets/alert_camera.svg',
  'openpilot/selfdrive/carrot/web/assets/alert_police.svg',
  'openpilot/selfdrive/carrot/xiaoge/assets/lane.onnx',
  'openpilot/selfdrive/carrot/xiaoge/assets/v_asm_model.onnx',
  'panda/board/obj/bootstub.panda.bin', 'panda/board/obj/bootstub.panda_h7.bin',
  'panda/board/obj/panda.bin.signed', 'panda/board/obj/panda_h7.bin.signed',
})


class ReleaseError(ValueError):
  """A release input or remote identity failed verification."""


@dataclass(frozen=True, slots=True)
class Member:
  name: str
  sha256: str
  size: int
  mode: int
  link: str | None = None


def require(condition: bool, message: str) -> None:
  if not condition:
    raise ReleaseError(message)


def mapping(value: JSON) -> dict[str, JSON]:
  require(isinstance(value, dict), 'expected JSON object')
  if isinstance(value, dict):
    return value
  raise ReleaseError('expected JSON object')


def sequence(value: JSON) -> list[JSON]:
  if isinstance(value, list):
    return value
  raise ReleaseError('expected JSON array')


def string(value: JSON) -> str:
  if isinstance(value, str):
    return value
  raise ReleaseError('expected JSON string')


def integer(value: JSON) -> int:
  if type(value) is int:
    return value
  raise ReleaseError('expected JSON integer')


def hex_digest(value: JSON, length: int = 64) -> str:
  digest = string(value)
  require(len(digest) == length and all(char in '0123456789abcdef' for char in digest), 'invalid SHA identity')
  return digest


def json_file(path: Path) -> dict[str, JSON]:
  require(path.is_file() and not path.is_symlink() and 0 < path.stat().st_size <= MAX_METADATA, f'invalid metadata file: {path}')
  return mapping(json.loads(path.read_bytes(), object_pairs_hook=unique_object))


def unique_object(pairs: list[tuple[str, JSON]]) -> dict[str, JSON]:
  value = dict(pairs)
  require(len(value) == len(pairs), 'duplicate JSON field')
  return value


def sha256(path: Path) -> str:
  with path.open('rb') as stream:
    return hashlib.file_digest(stream, 'sha256').hexdigest()


def clean_source(root: Path) -> tuple[str, int]:
  """Verify repository origin, clean tracked/untracked source and full HEAD."""
  remote = subprocess.check_output(['git', 'remote', 'get-url', 'origin'], cwd=root, text=True).strip()
  require(remote in (f'https://github.com/{REPOSITORY}', f'https://github.com/{REPOSITORY}.git',
                     f'git@github.com:{REPOSITORY}.git', f'ssh://git@github.com/{REPOSITORY}.git'), 'wrong source repository origin')
  status = subprocess.check_output(['git', 'status', '--porcelain', '--untracked-files=normal'], cwd=root, text=True)
  require(not status, 'source checkout must be clean')
  commit = hex_digest(subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip(), 40)
  timestamp = int(subprocess.check_output(['git', 'show', '-s', '--format=%ct', commit], cwd=root, text=True))
  require(0 <= timestamp < 2**32, 'invalid source commit timestamp')
  return commit, timestamp


def safe_name(name: str) -> str:
  path = PurePosixPath(name)
  require(bool(name) and not path.is_absolute() and '..' not in path.parts and str(path) == name and '\\' not in name
          and '\x00' not in name and len(name) <= 4096, f'invalid archive path: {name}')
  return name


def native(path: Path, *, static: bool = False) -> None:
  """Inspect ELF64 AArch64 headers; installers must have no loader/DT_NEEDED."""
  size = path.stat().st_size
  require(0 < size < MAX_ASSET, f'asset exceeds 2 GiB limit or is empty: {path}')
  with path.open('rb') as stream:
    header = stream.read(64)
    require(len(header) == 64 and header[:7] == b'\x7fELF\x02\x01\x01' and int.from_bytes(header[18:20], 'little') == 183,
            f'expected AArch64 ELF64: {path}')
    if not static:
      return
    require(int.from_bytes(header[16:18], 'little') in (2, 3), 'installer must be executable ELF')
    offset = int.from_bytes(header[32:40], 'little')
    entry_size, count = struct.unpack_from('<HH', header, 54)
    require(entry_size == 56 and 0 < count <= 4096 and offset >= 64 and offset + count * entry_size <= size, 'invalid ELF program headers')
    stream.seek(offset)
    programs = [struct.unpack('<IIQQQQQQ', stream.read(56)) for _ in range(count)]
    require(any(program[0] == 1 for program in programs), 'installer has no ELF load segment')
    for kind, _, location, _, _, file_size, _, _ in programs:
      require(location + file_size <= size, 'ELF segment outside installer')
      require(kind != 3, 'installer is not static: PT_INTERP')
      if kind == 2:
        require(file_size % 16 == 0, 'invalid ELF dynamic segment')
        stream.seek(location)
        for _ in range(file_size // 16):
          tag, _ = struct.unpack('<QQ', stream.read(16))
          if tag == 0:
            break
          require(tag != 1, 'installer is not static: DT_NEEDED')


def build_receipt(path: Path, installer: Path, commit: str) -> dict[str, JSON]:
  receipt = json_file(path)
  require(type(receipt.get('schema')) is int and receipt.get('schema') == 1 and receipt.get('source_commit') == commit
          and receipt.get('architecture') == 'aarch64'
          and receipt.get('target') == 'aarch64-unknown-linux-musl', 'installer build receipt identity mismatch')
  require(hex_digest(receipt.get('installer_sha256')) == sha256(installer), 'installer build receipt hash mismatch')
  require(bool(string(receipt.get('build_command')).strip()), 'installer build command missing')
  return receipt


def disk_guard(destination: Path, growth: int) -> None:
  """Keep the project-required 25 GiB free reserve plus bounded growth."""
  ancestor = destination
  while not ancestor.exists():
    ancestor = ancestor.parent
  require(shutil.disk_usage(ancestor).free >= 25 * 1024**3 + growth,
          'insufficient disk: retain 25 GiB plus operation growth; recover 35 GiB before resuming')


def sidecar(path: Path) -> str:
  return f'{sha256(path)}  {path.name}\n'
