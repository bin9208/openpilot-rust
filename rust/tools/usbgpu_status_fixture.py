# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# How to run:
# Imported by: python rust/tools/check_usbgpu_status.py --binary BIN --evidence FILE
"""Synthetic local-package admission fixtures; no trained model or GPU execution."""

from __future__ import annotations

from dataclasses import dataclass
import hashlib
import json
from pathlib import Path
import shutil
import sys
from typing import Final, Literal, TypeAlias, TypedDict, assert_never

ROOT: Final = Path(__file__).resolve().parents[2]
Json: TypeAlias = None | bool | int | float | str | list['Json'] | dict[str, 'Json']
FIRMWARE: Final = (
  'psp_14_0_2_sos.bin', 'gc_12_0_0_pfp.bin', 'gc_12_0_0_me.bin', 'gc_12_0_0_mec.bin',
  'gc_12_0_0_rlc.bin', 'gc_12_0_0_imu.bin', 'sdma_7_0_0.bin', 'smu_14_0_2.bin',
)
CAMERAS: Final = ((1928, 1208, 4804608), (1344, 760, 2428928))
KERNEL: Final = b'owned-test-code!'
SELECTED: Final = b'synthetic-selected-ONNX-for-status-only'
PICKLE: Final = b'synthetic-local-PKL-for-status-only'
Case: TypeAlias = Literal[
  'chunked', 'flat', 'previous', 'precompiled_selection', 'missing_package', 'missing_pointer', 'bad_pointer',
  'pointer_identity', 'missing_camera', 'selected_identity', 'source_hash', 'missing_chunk', 'chunk_count',
  'manifest_hash', 'kernel_hash', 'weights_hash', 'graph_layout', 'unlisted_graph_file', 'firmware_pin',
  'symlink', 'rejected', 'previous_rejected', 'bad_state', 'missing_original',
]


class FileReceipt(TypedDict):
  path: str
  bytes: int
  sha256: str


class HardwareReceipt(FileReceipt):
  origin: str


@dataclass(frozen=True, slots=True)
class Fixture:
  root: Path
  model_sha256: str
  pickle_sha256: str

  @property
  def models(self) -> Path:
    return self.root / 'models'

  @property
  def cache(self) -> Path:
    return self.root / 'cache'

  @property
  def assets(self) -> Path:
    return self.root / 'assets'

  @property
  def name(self) -> str:
    return f'big_driving_{self.model_sha256[:16]}_tinygrad.pkl'

  @property
  def model(self) -> Path:
    return self.models / self.name

  @property
  def package(self) -> Path:
    return self.assets / 'local' / self.name / self.pickle_sha256

  def input_paths(self) -> list[str]:
    return [str(self.models), str(self.cache), str(self.assets)]


def sha256(data: bytes) -> str:
  return hashlib.sha256(data).hexdigest()


def write_json(path: Path, value: Json) -> None:
  path.write_text(json.dumps(value, sort_keys=True) + '\n')


def hardware(directory: Path, *, fresh: bool) -> list[HardwareReceipt]:
  """Reuse SHA-verified firmware or fetch only the canonical source's eight blobs."""
  free = shutil.disk_usage(directory.parent).free
  if free < (25 << 30) + (128 << 20):
    message = f'firmware fixture needs 25 GiB free plus 128 MiB growth; available={free}'
    raise RuntimeError(message)
  directory.mkdir()
  hashes = json.loads((ROOT / 'rust/crates/usbgpu/assets/amd-metadata.json').read_text())['firmware_hashes']
  sys.path.insert(0, str(ROOT / 'tinygrad_repo'))
  from tinygrad import helpers
  helpers.cache_dir = str(directory / 'source-download-cache')
  receipts: list[HardwareReceipt] = []
  for name in FIRMWARE:
    local = ROOT / 'rust/bin/usbgpu-assets/firmware' / name
    use_local = not fresh and local.is_file()
    data = local.read_bytes() if use_local else helpers.fetch_fw('amdgpu', name, hashes[name])
    assert sha256(data) == hashes[name], name
    (directory / name).write_bytes(data)
    receipts.append({'path': name, 'bytes': len(data), 'sha256': hashes[name],
                     'origin': str(local) if use_local else 'tinygrad.helpers.fetch_fw (pinned linux-firmware 1e2c15348485939baf1b6d1f5a7a3b799d80703d)'})
  return receipts


def graph(directory: Path, frame_bytes: int) -> None:
  """Match local full-camera bindings with a small, non-executed synthetic kernel."""
  directory.mkdir()
  sizes = [64, frame_bytes, frame_bytes, 36, 36, 32]
  names = ['packed_npy_inputs', 'frame', 'big_frame', 'tfm', 'big_tfm']
  descriptor = {
    'version': 2, 'backend': 'amd-elf', 'arch': 'gfx1200', 'weights_sha256': sha256(b''),
    'allocations': [{'bytes': size, 'weight_offset': None} for size in sizes],
    'views': [{'allocation': index, 'offset': 0, 'bytes': size} for index, size in enumerate(sizes)],
    'inputs': [{'name': name, 'view': index} for index, name in enumerate(names)],
    'outputs': [{'name': 'model', 'view': 5}],
    'kernels': [{'name': 'policy', 'binary_sha256': sha256(KERNEL), 'binary_bytes': len(KERNEL)}],
    'calls': [{'op': 'kernel', 'kernel': 0, 'views': [0, 5], 'scalars': [], 'global': [1, 1, 1], 'local': [1, 1, 1]}],
    'entrypoints': [{'name': 'prepare', 'start': 0, 'end': 0}, {'name': 'policy', 'start': 0, 'end': 1}],
  }
  write_json(directory / 'graph.json', descriptor)
  (directory / 'kernel-0.bin').write_bytes(KERNEL)
  (directory / 'weights.bin').write_bytes(b'')


def inventory(directory: Path) -> list[FileReceipt]:
  return [{'path': str(path.relative_to(directory)), 'bytes': path.stat().st_size, 'sha256': sha256(path.read_bytes())}
          for path in sorted(directory.rglob('*')) if path.is_file() and path != directory / 'manifest.json']


def create(root: Path, firmware: Path) -> Fixture:
  """Create an admissible, hash-keyed local package independently of native writers."""
  fixture = Fixture(root, sha256(SELECTED), sha256(PICKLE))
  fixture.models.mkdir(parents=True)
  fixture.cache.mkdir()
  fixture.package.mkdir(parents=True)
  manifest = {'model_id': 'synthetic-status', 'filename': 'model.onnx', 'size': len(SELECTED),
              'sha256': fixture.model_sha256, 'url': 'https://models.test/model.onnx'}
  write_json(fixture.cache / 'state.json', {'active': manifest, 'previous': None})
  (fixture.cache / f'model-{fixture.model_sha256[:16]}.onnx').write_bytes(SELECTED)
  Path(str(fixture.model) + '.chunkmanifest').write_text('1')
  Path(str(fixture.model) + '.chunk01of01').write_bytes(PICKLE)
  write_json(fixture.package.parent / 'current.json', {'version': 1, 'pickle_sha256': fixture.pickle_sha256})
  shutil.copytree(firmware, fixture.package / 'firmware', ignore=shutil.ignore_patterns('source-download-cache'))
  shutil.copyfile(ROOT / 'tinygrad_repo/LICENSE', fixture.package / 'tinygrad-LICENSE')
  probe_kernel = {'name': 'synthetic-admission-only', 'elf': list(KERNEL), 'sha256': sha256(KERNEL)}
  write_json(fixture.package / 'probe-gfx1200.json', {
    'version': 1, 'arch': 'gfx1200', 'seed': 42, 'buffers': [8, 8, 1 << 22],
    'copies': [{'buffer': 1, 'offset': 0, 'data': [25, 17, 184, 20, 42, 0, 0, 0]}],
    'kernels': [probe_kernel, probe_kernel],
    'calls': [{'kernel': index, 'buffers': [], 'values': [], 'global': [1, 1, 1], 'local': [1, 1, 1]} for index in range(2)],
    'output': {'buffer': 2, 'offset': 0, 'bytes': 1 << 22}, 'source_sha256': sha256(b'synthetic-probe'),
  })
  for width, height, frame_bytes in CAMERAS:
    graph(fixture.package / f'amd-{width}x{height}', frame_bytes)
  write_json(fixture.package / 'manifest.json', {
    'version': 1, 'format': 'run-policy', 'model_sha256': fixture.model_sha256, 'pickle_sha256': fixture.pickle_sha256,
    'pickle_bytes': len(PICKLE), 'filename': fixture.name, 'checkpoint': 'synthetic-status-admission-only',
    'scalar_inputs': [{'name': name, 'offset': offset, 'shape': shape} for name, offset, shape in
                      [('desire', 0, [8]), ('traffic_convention', 32, [1, 2]), ('action_t', 40, [1, 2]), ('prev_feat', 48, [1, 4])]],
    'output_count': 8, 'output_slices': {'hidden_state': [4, 8]},
    'cameras': [{'camera': [width, height], 'graph': f'amd-{width}x{height}', 'qcom_warp': None} for width, height, _ in CAMERAS],
    'files': inventory(fixture.package),
  })
  return fixture


def mutate(fixture: Fixture, case: Case) -> None:
  """Change one readiness precondition; semantic controls rehash their file inventory."""
  manifest_path = fixture.package / 'manifest.json'
  manifest = json.loads(manifest_path.read_text())
  state_path = fixture.cache / 'state.json'
  state = json.loads(state_path.read_text())
  marker = Path(str(fixture.model) + '.chunkmanifest')
  chunk = Path(str(fixture.model) + '.chunk01of01')
  kernel = fixture.package / 'amd-1928x1208/kernel-0.bin'
  match case:
    case 'chunked':
      return
    case 'flat':
      marker.unlink()
      chunk.rename(fixture.model)
    case 'previous' | 'previous_rejected':
      write_json(state_path, {'active': None, 'previous': state['active']})
      if case == 'previous_rejected':
        Path(str(fixture.model) + '.native_rejected').write_text(fixture.pickle_sha256)
    case 'precompiled_selection':
      state['active']['filename'] = 'model.pkl'
      (fixture.cache / f'model-{fixture.model_sha256[:16]}.onnx').rename(fixture.cache / f'model-{fixture.model_sha256[:16]}.pkl')
      write_json(state_path, state)
    case 'missing_package':
      shutil.rmtree(fixture.assets / 'local')
    case 'missing_pointer':
      (fixture.package.parent / 'current.json').unlink()
    case 'bad_pointer':
      (fixture.package.parent / 'current.json').write_text('{')
    case 'pointer_identity':
      write_json(fixture.package.parent / 'current.json', {'version': 1, 'pickle_sha256': '0' * 64})
    case 'missing_camera':
      manifest['cameras'].pop()
      write_json(manifest_path, manifest)
    case 'selected_identity':
      manifest['model_sha256'] = '0' * 64
      write_json(manifest_path, manifest)
    case 'source_hash':
      chunk.write_bytes(bytes([PICKLE[0] ^ 1]) + PICKLE[1:])
    case 'missing_chunk':
      chunk.unlink()
    case 'chunk_count':
      marker.write_text('1025')
    case 'manifest_hash':
      manifest['files'][0]['sha256'] = '0' * 64
      write_json(manifest_path, manifest)
    case 'kernel_hash':
      kernel.write_bytes(bytes([KERNEL[0] ^ 1]) + KERNEL[1:])
    case 'weights_hash':
      (fixture.package / 'amd-1928x1208/weights.bin').write_bytes(b'x')
    case 'graph_layout':
      graph_path = fixture.package / 'amd-1928x1208/graph.json'
      descriptor = json.loads(graph_path.read_text())
      descriptor['views'][1]['bytes'] -= 1
      write_json(graph_path, descriptor)
      manifest['files'] = inventory(fixture.package)
      write_json(manifest_path, manifest)
    case 'unlisted_graph_file':
      (fixture.package / 'amd-1928x1208/unlisted.bin').write_bytes(b'x')
    case 'firmware_pin':
      path = fixture.package / 'firmware' / FIRMWARE[0]
      data = path.read_bytes()
      path.write_bytes(bytes([data[0] ^ 1]) + data[1:])
      manifest['files'] = inventory(fixture.package)
      write_json(manifest_path, manifest)
    case 'symlink':
      kernel.rename(fixture.root / 'kernel.bin')
      kernel.symlink_to(fixture.root / 'kernel.bin')
    case 'rejected':
      Path(str(fixture.model) + '.native_rejected').write_text(fixture.pickle_sha256)
    case 'bad_state':
      state_path.write_text('{')
    case 'missing_original':
      (fixture.cache / f'model-{fixture.model_sha256[:16]}.onnx').unlink()
    case unreachable:
      assert_never(unreachable)
