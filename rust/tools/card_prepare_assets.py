#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# ─── How to run ───
# python3 rust/tools/card_prepare_assets.py REPOSITORY NEW_DBC_DIRECTORY
"""Prepare card's source DBC assets during packaging, never at runtime."""

from __future__ import annotations

import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
from typing import Final, TypedDict

GENERATED: Final = (
  "nissan_x_trail_2017_generated.dbc",
  "nissan_leaf_2018_generated.dbc",
  "chrysler_pacifica_2017_hybrid_generated.dbc",
  "chrysler_ram_dt_generated.dbc",
  "chrysler_ram_hd_generated.dbc",
  "subaru_global_2017_generated.dbc",
  "subaru_global_2020_hybrid_generated.dbc",
  "subaru_forester_2017_generated.dbc",
  "subaru_outback_2015_generated.dbc",
  "subaru_outback_2019_generated.dbc",
  "toyota_new_mc_pt_generated.dbc",
  "toyota_nodsu_pt_generated.dbc",
  "toyota_secoc_pt_generated.dbc",
  "toyota_tnga_k_pt_generated.dbc",
  "honda_accord_2018_can_generated.dbc",
  "honda_civic_hatchback_ex_2017_can_generated.dbc",
  "honda_civic_ex_2022_can_generated.dbc",
  "honda_crv_ex_2017_can_generated.dbc",
  "honda_crv_ex_2017_body_generated.dbc",
  "acura_rdx_2020_can_generated.dbc",
  "honda_insight_ex_2019_can_generated.dbc",
  "acura_ilx_2016_can_generated.dbc",
  "honda_crv_touring_2016_can_generated.dbc",
  "honda_crv_executive_2016_can_generated.dbc",
  "honda_fit_ex_2018_can_generated.dbc",
  "honda_odyssey_exl_2018_generated.dbc",
  "honda_odyssey_extreme_edition_2018_china_can_generated.dbc",
  "acura_rdx_2018_can_generated.dbc",
  "honda_civic_touring_2016_can_generated.dbc",
)


class AssetManifest(TypedDict):
  format_version: int
  build_time_only: bool
  source_sha256: dict[str, str]
  asset_sha256: dict[str, str]


class InsufficientSpaceError(OSError):
  def __init__(self, free: int, required: int) -> None:
    self.free = free
    self.required = required
    super().__init__(f"card assets require {required} free bytes; found {free}")


def digest(path: Path) -> str:
  return hashlib.sha256(path.read_bytes()).hexdigest()


def prepare_assets(repository: Path, destination: Path) -> AssetManifest:
  """Publish a new directory containing immutable copies and generated DBCs.

  The caller packages this directory at opendbc_repo/opendbc/dbc. Generation
  uses copied upstream inputs so neither the source tree nor earlier evidence
  is mutated. Existing destinations fail rather than silently mixing assets.
  """
  repository = repository.resolve()
  destination = destination.absolute()
  if destination.exists():
    raise FileExistsError(destination)
  source = repository / "opendbc_repo/opendbc/dbc"
  originals = sorted(source.glob("*.dbc"))
  generator = source / "generator"
  generator_inputs = [generator / "generator.py"]
  for brand in ("nissan", "chrysler", "subaru", "toyota", "honda"):
    generator_inputs.extend(sorted((generator / brand).glob("*.dbc")))
    generator_inputs.extend(sorted((generator / brand).glob("*.py")))
  # Account for input staging and the resulting output, with bounded headroom.
  growth = 4 * sum(path.stat().st_size for path in originals + generator_inputs) + 16 * 1024**2
  required = 25 * 1024**3 + growth
  free = shutil.disk_usage(destination.parent).free
  if free < required:
    raise InsufficientSpaceError(free, required)
  manifest: AssetManifest = {
    "format_version": 1,
    "build_time_only": True,
    "source_sha256": {str(path.relative_to(repository)): digest(path) for path in originals + generator_inputs},
    "asset_sha256": {},
  }
  with tempfile.TemporaryDirectory(prefix="card-assets-", dir=destination.parent) as temporary:
    staging = Path(temporary)
    inputs = staging / "generator"
    assets = staging / "dbc"
    inputs.mkdir()
    assets.mkdir()
    for original in generator_inputs:
      copied = inputs / original.relative_to(generator)
      copied.parent.mkdir(parents=True, exist_ok=True)
      shutil.copyfile(original, copied)
    subprocess.run([
      sys.executable, "-c",
      "from generator import create_all; import sys; create_all(sys.argv[1], sys.argv[2])",
      str(assets), str(inputs),
    ], cwd=inputs, check=True)
    for original in originals:
      if original.name not in GENERATED:
        shutil.copyfile(original, assets / original.name)
    for name in GENERATED:
      # Missing generation is a packaging error, not deferred to device startup.
      (assets / name).stat()
    manifest["asset_sha256"] = {path.name: digest(path) for path in sorted(assets.glob("*.dbc"))}
    (assets / "card-assets.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    assets.rename(destination)
  return manifest


if __name__ == "__main__":
  if len(sys.argv) != 3:
    raise SystemExit("usage: card_prepare_assets.py REPOSITORY NEW_DBC_DIRECTORY")
  result = prepare_assets(Path(sys.argv[1]), Path(sys.argv[2]))
  print(f"Prepared {len(result['asset_sha256'])} DBC assets; Python used only at build time")
