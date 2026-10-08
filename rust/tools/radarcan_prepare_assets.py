#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# ─── How to run ───
# PYTHONPATH=opendbc_repo python rust/tools/radarcan_prepare_assets.py NEW_DIRECTORY
"""Package unchanged source DBCs and original generated inputs for RadarCAN."""

from __future__ import annotations

import hashlib
import json
from pathlib import Path
import shutil
import sys

from opendbc import DBC_PATH
from opendbc.dbc.generator.generator import generate_all


def prepare(destination: Path) -> None:
  source = Path(DBC_PATH)
  inputs = sorted(path for path in source.rglob('*') if path.is_file()
    and '__pycache__' not in path.parts and path.suffix != '.pyc')
  growth = 4 * sum(path.stat().st_size for path in inputs) + 64 * 1024**2
  free = shutil.disk_usage(destination.parent).free
  if free < 25 * 1024**3 + growth:
    raise OSError(f'RadarCAN assets need {25 * 1024**3 + growth} free bytes; found {free}')
  destination.mkdir()
  generated = generate_all()
  for original in sorted(source.glob('*.dbc')):
    if original.stem not in generated:
      shutil.copyfile(original, destination / original.name)
  for name, content in generated.items():
    (destination / (name + '.dbc')).write_text(content, encoding='utf-8')
  manifest = {'build_time_only': True, 'free_before': free, 'growth_bound': growth,
    'source_sha256': {str(path.relative_to(source)): hashlib.sha256(path.read_bytes()).hexdigest() for path in inputs},
    'asset_sha256': {path.name: hashlib.sha256(path.read_bytes()).hexdigest() for path in sorted(destination.glob('*.dbc'))}}
  (destination / 'radarcan-assets.json').write_text(json.dumps(manifest, indent=2) + '\n')
  print(f"Prepared {len(manifest['asset_sha256'])} original RadarCAN DBCs")


if __name__ == '__main__':
  prepare(Path(sys.argv[1]))
