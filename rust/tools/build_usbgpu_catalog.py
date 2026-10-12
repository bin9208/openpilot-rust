"""Convert a verified precompiled catalog into immutable native USB GPU companions."""

from __future__ import annotations
import argparse
from pathlib import Path
from usbgpu_catalog_package import Sources, publish


def main() -> None:
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument('--trusted-catalog', type=Path, required=True)
  parser.add_argument('--selected-model-sha256', required=True)
  parser.add_argument('--trusted-model', type=Path, required=True)
  parser.add_argument('--trusted-runtime-archive', type=Path, required=True)
  parser.add_argument('--trusted-hardware-assets', type=Path, required=True)
  parser.add_argument('--output-assets', type=Path, required=True)
  args = parser.parse_args()
  result = publish(
    Sources(args.trusted_catalog, args.trusted_model, args.trusted_runtime_archive, args.trusted_hardware_assets, args.selected_model_sha256),
    args.output_assets,
  )
  print(result / 'manifest.json')


if __name__ == '__main__':
  main()
