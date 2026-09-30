# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy==2.5.3", "zstandard==0.25.0"]
# ///
# Run with PYTHONPATH=.:tinygrad_repo JIT_BATCH_SIZE=0 uv run rust/tools/build_model_pipelines.py --help
from __future__ import annotations

import argparse
import hashlib
from pathlib import Path

from model_export.original import DriverArtifacts, driver, driving
from model_export.publish import BundleSpec, publish
from openpilot.common.file_chunker import open_file_chunked


def source_hash(path: Path) -> str:
    with open_file_chunked(str(path)) as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def main() -> None:
    parser = argparse.ArgumentParser(description="Convert trusted original compiled artifacts to immutable native model pipelines")
    parser.add_argument("--trusted-model-directory", type=Path, required=True)
    parser.add_argument("--trusted-driving", type=Path)
    parser.add_argument("--backend", choices=("cpu", "qcom"), required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    root = args.trusted_model_directory
    road = args.trusted_driving or root / "driving_tinygrad.pkl"
    model = root / "dmonitoring_model_tinygrad.pkl"
    metadata = root / "dmonitoring_model_metadata.pkl"
    road_sources = {"driving":source_hash(road)}
    driver_sources = {"model":source_hash(model), "metadata":source_hash(metadata)}
    specs = []
    for resolution in ((1928,1208), (1344,760)):
        warp = root / f"dm_warp_{resolution[0]}x{resolution[1]}_tinygrad.pkl"
        specs.append(BundleSpec("driving", resolution, driving(road, resolution), road_sources))
        specs.append(BundleSpec("driver", resolution, driver(DriverArtifacts(model, warp, metadata)),
                                driver_sources | {"warp":source_hash(warp)}))
    publish(args.output, specs, args.backend)
    print(f"Published native pipeline catalog: {args.output / 'current.json'}")


if __name__ == "__main__":
    main()
