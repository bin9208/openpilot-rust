#!/usr/bin/env python3
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Stage verified generic eGPU companions without a synthetic remote catalog."""
from __future__ import annotations

import argparse
import ast
from dataclasses import dataclass
import hashlib
import json
from pathlib import Path, PurePosixPath
import shutil
from typing import Final

FIRMWARE: Final = (
  'psp_14_0_2_sos.bin', 'gc_12_0_0_pfp.bin', 'gc_12_0_0_me.bin',
  'gc_12_0_0_mec.bin', 'gc_12_0_0_rlc.bin', 'gc_12_0_0_imu.bin',
  'sdma_7_0_0.bin', 'smu_14_0_2.bin',
)


@dataclass(frozen=True, slots=True)
class Copy:
  source: Path
  target: Path
  sha256: str


@dataclass(frozen=True, slots=True)
class Bundle:
  path: Path
  manifest_sha256: str
  firmware_license: Path
  model: Path | None


def digest(path: Path) -> str:
  with path.open('rb') as stream:
    return hashlib.file_digest(stream, 'sha256').hexdigest()


def regular(root: Path, relative: str) -> Path:
  path = PurePosixPath(relative)
  if not relative or path.is_absolute() or '..' in path.parts:
    raise RuntimeError(f'invalid native companion path: {relative}')
  result = root
  for component in path.parts:
    result /= component
    if result.is_symlink():
      raise RuntimeError(f'native companion symlink: {result}')
  if not result.is_file():
    raise RuntimeError(f'missing native companion: {result}')
  return result


def stage(root: Path, bundle: Bundle) -> None:
  package = bundle.path
  identity = bundle.manifest_sha256
  manifest = regular(package, 'manifest.json')
  if digest(manifest) != identity:
    raise RuntimeError('native package manifest identity mismatch')
  record = json.loads(manifest.read_text())
  if record['version'] != 1 or len(record['files']) > 512:
    raise RuntimeError('unsupported native companion manifest')
  selected = record['model_sha256']
  if len(selected) != 64 or any(c not in '0123456789abcdef' for c in selected):
    raise RuntimeError('invalid selected native model identity')
  destination = root / 'rust/bin/usbgpu-assets'
  files = [Copy(manifest, destination / 'manifest.json', identity)]
  names: set[str] = set()
  total = 0
  metadata = json.loads(regular(root, 'rust/crates/usbgpu/assets/amd-metadata.json').read_text())
  for entry in record['files']:
    name = entry['path']
    source = regular(package, name)
    if name in names or source.stat().st_size != entry['bytes'] or digest(source) != entry['sha256']:
      raise RuntimeError(f'native companion checksum/size/duplicate mismatch: {name}')
    names.add(name)
    total += entry['bytes']
    files.append(Copy(source, destination / name, entry['sha256']))
  required = {'tinygrad-LICENSE', 'probe-gfx1200.json', 'source-provenance.json',
              f'models/{selected}/model.hcq.json', f'models/{selected}/model.hcq-meta.json'}
  for name in FIRMWARE:
    required.add('firmware/' + name)
    if digest(regular(package, 'firmware/' + name)) != metadata['firmware_hashes'][name]:
      raise RuntimeError(f'firmware differs from source-pinned AMD metadata: {name}')
  for camera in ('1344x760', '1928x1208'):
    required.add(f'warp-gfx1200-{camera}.json')
    prefix = f'warp-qcom-a630-{camera}'
    required.update(f'{prefix}/{name}' for name in ('graph.json', 'weights.bin', 'provenance.json'))
    graph = json.loads(regular(package, f'{prefix}/graph.json').read_text())
    required.update(f'{prefix}/kernel-{i}.bin' for i in range(len(graph['kernels'])))
  if not required <= names or total > 128 << 20:
    raise RuntimeError('incomplete/oversized native companion package')
  license_root = root / 'rust/bin/licenses/usbgpu'
  files += [Copy(bundle.firmware_license, license_root / 'LICENSE.amdgpu', digest(bundle.firmware_license)),
            Copy(package / 'tinygrad-LICENSE', license_root / 'tinygrad-LICENSE', digest(package / 'tinygrad-LICENSE')),
            Copy(package / 'source-provenance.json', license_root / 'build-provenance.json', digest(package / 'source-provenance.json'))]
  provider = None
  if bundle.model is not None:
    source_definition = regular(root, 'openpilot/selfdrive/modeld/big_model.py')
    definition = ast.parse(source_definition.read_text())
    selected_source = next(node.value for node in definition.body if isinstance(node, ast.Assign)
                           and any(isinstance(target, ast.Name) and target.id == 'CINQUE_V3_MODEL' for target in node.targets))
    pinned = ast.literal_eval(selected_source)
    descriptor = json.loads(regular(package, f'models/{selected}/model.hcq.json').read_text())
    model_metadata = json.loads(regular(package, f'models/{selected}/model.hcq-meta.json').read_text())
    if (pinned['sha256'] != selected or pinned['size'] != descriptor['model_bytes']
        or bundle.model.stat().st_size != pinned['size'] or digest(bundle.model) != selected):
      raise RuntimeError('native offline provider must contain the exact source-pinned PKL')
    model_path = f'models/{selected}/model.pkl'
    files.append(Copy(bundle.model, destination / model_path, selected))
    provenance = json.loads(regular(package, 'source-provenance.json').read_text())
    provider = json.dumps({'version': 1, 'kind': 'native-generic-hcq', 'model_sha256': selected,
                           'model_checkpoint': model_metadata['checkpoint'],
                           'pickle': {'path': model_path, 'size': pinned['size'], 'sha256': selected},
                           'assets_manifest_sha256': identity,
                           'runtime_archive_sha256': provenance['runtime_sha256']}, indent=2) + '\n'
  growth = sum(item.source.stat().st_size for item in files)
  if shutil.disk_usage(root).free < (25 << 30) + growth:
    raise OSError('require 25 GiB plus package growth; recover 35 GiB before resuming')
  for item in files:
    if any(p.is_symlink() for p in item.target.parents):
      raise RuntimeError(f'package directory symlink: {item.target}')
    if item.target.is_symlink() or (item.target.exists() and digest(item.target) != item.sha256):
      raise RuntimeError(f'conflicting package artifact: {item.target}')
  provider_target = destination / 'native-provider.json'
  if provider is not None and (provider_target.is_symlink() or
      (provider_target.exists() and provider_target.read_text() != provider)):
    raise RuntimeError(f'conflicting native provider record: {provider_target}')
  for item in files:
    item.target.parent.mkdir(parents=True, exist_ok=True)
    if not item.target.exists():
      shutil.copyfile(item.source, item.target)
  if provider is not None and not provider_target.exists():
    provider_target.write_text(provider)
  print(json.dumps({'package': str(destination), 'manifest_sha256': identity,
                    'model_sha256': selected, 'files': len(files) + int(provider is not None),
                    'bytes': growth + (len(provider.encode()) if provider is not None else 0)}))


def main() -> None:
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument('--root', type=Path, required=True)
  parser.add_argument('--package', type=Path, required=True)
  parser.add_argument('--expect-manifest-sha256', required=True)
  parser.add_argument('--firmware-license', type=Path, required=True)
  parser.add_argument('--model', type=Path)
  args = parser.parse_args()
  stage(args.root.resolve(), Bundle(args.package.resolve(), args.expect_manifest_sha256,
        args.firmware_license.resolve(), args.model.resolve() if args.model is not None else None))


if __name__ == '__main__':
  main()
