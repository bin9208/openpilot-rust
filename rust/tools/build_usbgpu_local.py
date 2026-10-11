# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy==2.5.3", "zstandard==0.25.0"]
# ///
# Run with PYTHONPATH=.:tinygrad_repo:rust/tools uv run rust/tools/build_usbgpu_local.py --help
from __future__ import annotations

import argparse
from pathlib import Path

from model_export.local import Sources, publish


def main() -> None:
  parser = argparse.ArgumentParser(description='Export trusted captured local USB GPU artifacts before native runtime startup')
  parser.add_argument('--trusted-compiled-model', type=Path, required=True)
  parser.add_argument('--selected-onnx-sha256', required=True)
  parser.add_argument('--trusted-hardware-assets', type=Path, required=True)
  parser.add_argument('--output-assets', type=Path, required=True)
  args = parser.parse_args()
  destination = publish(Sources(args.trusted_compiled_model, args.selected_onnx_sha256, args.trusted_hardware_assets), args.output_assets)
  print(destination / 'manifest.json')


if __name__ == '__main__':
  main()
