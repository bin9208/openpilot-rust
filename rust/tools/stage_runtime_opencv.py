#!/usr/bin/env python3
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Stage a verified combined OpenCV provider and its native ARM consumers."""
from __future__ import annotations

import argparse
from dataclasses import dataclass
import json
from pathlib import Path
import shutil

from stage_runtime_media import alias, copy, digest


@dataclass(frozen=True, slots=True)
class Artifact:
  source: Path
  destination: Path
  sha256: str


@dataclass(frozen=True, slots=True)
class Alias:
  directory: Path
  name: str
  target: str


def stage(root: Path, provider: Path, evidence: Path) -> None:
  free = shutil.disk_usage(root).free
  if free < 25 * 1024**3 + 256 * 1024**2:
    raise OSError('require 25 GiB plus 256 MiB; recover 35 GiB before resuming')
  identity = json.loads((provider / 'receipt.json').read_text())
  comparisons = json.loads((evidence / 'opencv-native/result.json').read_text())
  webcam = json.loads((evidence / 'webcam-native/result.json').read_text())
  if (identity['architecture'] != 'aarch64' or not identity['source_unchanged']
      or comparisons['status'] != 'pass' or not all(row['match'] for row in webcam['pixels'])
      or not webcam['capture']['match']):
    raise RuntimeError('actual ARM source comparisons have not passed')
  for report in ['provider-abi.json', 'consumer-abi.json']:
    if not json.loads((evidence / report).read_text())['compatible']:
      raise RuntimeError('actual AGNOS dependency closure has not passed')
  binaries = root / 'rust/bin'
  lib = binaries / 'lib'
  artifacts = [Artifact(provider / path, lib / Path(path).name, sha)
               for path, sha in identity['libraries'].items()]
  aliases = [Alias(lib, p.name, p.resolve(strict=True).name)
             for p in sorted((provider / 'install/lib').glob('libopencv_*.so*')) if p.is_symlink()]
  frozen = json.loads((evidence / 'frozen-binaries.json').read_text())['binaries']
  artifacts += [Artifact(Path(row['frozen']), binaries / Path(row['frozen']).name, row['sha256'])
                for row in frozen if Path(row['frozen']).name in ('openpilot-xiaoge', 'openpilot-webcam')]
  licenses = binaries / 'licenses/opencv'
  artifacts += [Artifact(p, licenses / p.name, digest(p)) for p in sorted((provider / 'licenses').iterdir())]
  artifacts += [Artifact(provider / p, licenses / p, digest(provider / p))
                for p in ('source.json', 'opencv-defined-access.patch', 'native-libraries.sha256')]
  for item in artifacts:
    if digest(item.source) != item.sha256:
      raise RuntimeError(f'provider identity changed: {item.source}')
    if item.destination.is_symlink() or (item.destination.exists() and digest(item.destination) != item.sha256):
      raise RuntimeError(f'conflicting package artifact: {item.destination}')
  for item in aliases:
    destination = item.directory / item.name
    if destination.is_symlink():
      if destination.readlink() != Path(item.target):
        raise RuntimeError(f'conflicting package alias: {destination}')
    elif destination.exists():
      raise RuntimeError(f'package alias would replace a file: {destination}')
  records = [{'source': str(item.source), 'path': str(item.destination), 'sha256': item.sha256} for item in artifacts]
  linked = [{'path': str(item.directory / item.name), 'target': item.target} for item in aliases]
  provenance = {
    'architecture': 'aarch64', 'source_revision': identity['source_revision'],
    'source_archive_sha256': identity['source_archive_sha256'], 'hardening_patch_sha256': identity['hardening_patch_sha256'],
    'ownership': 'OpenCV and its bundled primitives are external providers; consumer policy is Rust.',
    'libraries': identity['libraries'], 'files': records, 'aliases': linked,
    'validation_directory': str(evidence), 'validation_hashes': {p: digest(evidence / p) for p in
      ('opencv-native/result.json', 'webcam-native/result.json', 'provider-abi.json', 'consumer-abi.json', 'frozen-binaries.json')},
    'scope': 'Actual source ARM image/model/video comparisons and AGNOS ELF closure. No hardware or complete installed-runtime acceptance.',
    'system_dependencies': 'Use pinned AGNOS libraries. No C runtime, FFmpeg or system libraries are copied by this stager.',
  }
  output = licenses / 'provenance.json'
  payload = json.dumps(provenance, indent=2) + '\n'
  if output.is_symlink() or (output.exists() and output.read_text() != payload):
    raise RuntimeError(f'conflicting provenance: {output}')
  for item in artifacts:
    for parent in item.destination.parents:
      if parent == root:
        break
      if parent.is_symlink():
        raise RuntimeError(f'package companion directory is a symlink: {parent}')
  for item in artifacts:
    copy(item.source, item.destination, item.sha256)
  for item in aliases:
    alias(item.directory, item.name, item.target)
  if not output.exists():
    output.write_text(payload)
  print(json.dumps({'files': len(records), 'aliases': len(linked), 'provenance': str(output), 'free_before': free}))


def main() -> None:
  cli = argparse.ArgumentParser(description=__doc__)
  for name in ('root', 'provider', 'evidence'):
    cli.add_argument('--' + name, type=Path, required=True)
  args = cli.parse_args()
  stage(args.root.resolve(strict=True), args.provider.resolve(strict=True), args.evidence.resolve(strict=True))


if __name__ == '__main__':
  main()
