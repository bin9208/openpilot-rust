# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy==2.5.3", "zstandard==0.25.0"]
# ///
# Run with PYTHONPATH=.:tinygrad_repo JIT_BATCH_SIZE=0 uv run rust/tools/build_model_pipelines.py --help
from __future__ import annotations

import argparse
import ctypes
import hashlib
import json
import os
from pathlib import Path

from model_export.original import DriverArtifacts, driver, driving
from model_export.publish import BundleSpec, publish
from openpilot.common.file_chunker import open_file_chunked


def source_hash(path: Path) -> str:
    with open_file_chunked(str(path)) as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def llvm_target() -> dict:
    # Use the same loaded LLVM provider as the existing CPU compiler. This is
    # optional metadata for native orchestration, not a second compiler helper.
    from tinygrad.runtime.autogen import llvm
    library = Path(os.environ["LLVM_PATH"]).resolve(strict=True)
    if Path(llvm.dll._name).resolve(strict=True) != library:
        raise ValueError("loaded LLVM provider differs from LLVM_PATH")
    def text(function) -> str:
        value = function()
        try:
            return ctypes.string_at(value).decode("utf-8")
        finally:
            llvm.LLVMDisposeMessage(value)
    return {"llvm_path": str(library), "llvm_sha256": source_hash(library),
            "llvm_cpu": text(llvm.LLVMGetHostCPUName),
            "llvm_features": sorted(text(llvm.LLVMGetHostCPUFeatures).split(","))}


def main() -> None:
    parser = argparse.ArgumentParser(description="Convert trusted original compiled artifacts to immutable native model pipelines")
    parser.add_argument("--trusted-model-directory", type=Path, required=True)
    parser.add_argument("--trusted-driving", type=Path)
    parser.add_argument("--backend", choices=("cpu", "qcom"), required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--kind", choices=("both", "driver"), default="both")
    parser.add_argument("--target-receipt", type=Path)
    args = parser.parse_args()
    if args.kind == "driver" and args.trusted_driving is not None:
        parser.error("--trusted-driving requires --kind both")
    if args.target_receipt is not None:
        if args.backend != "cpu":
            parser.error("--target-receipt requires --backend cpu")
        if args.target_receipt.exists() or args.target_receipt.is_symlink():
            raise FileExistsError(args.target_receipt)
    target_before = llvm_target() if args.target_receipt is not None else None
    root = args.trusted_model_directory
    road = args.trusted_driving or root / "driving_tinygrad.pkl"
    model = root / "dmonitoring_model_tinygrad.pkl"
    metadata = root / "dmonitoring_model_metadata.pkl"
    road_sources = {"driving":source_hash(road)} if args.kind == "both" else {}
    driver_sources = {"model":source_hash(model), "metadata":source_hash(metadata)}
    specs = []
    for resolution in ((1928,1208), (1344,760)):
        warp = root / f"dm_warp_{resolution[0]}x{resolution[1]}_tinygrad.pkl"
        if args.kind == "both":
            specs.append(BundleSpec("driving", resolution, driving(road, resolution), road_sources))
        specs.append(BundleSpec("driver", resolution, driver(DriverArtifacts(model, warp, metadata)),
                                driver_sources | {"warp":source_hash(warp)}))
    publish(args.output, specs, args.backend)
    if args.target_receipt is not None:
        target_after = llvm_target()
        if target_before != target_after:
            raise RuntimeError("LLVM target changed during catalog compilation")
        with args.target_receipt.open("x") as receipt:
            json.dump({"version": 1, "stored_jit_executed": False,
                       "before": target_before, "after": target_after}, receipt, indent=2, sort_keys=True)
            receipt.write("\n")
    print(f"Published native pipeline catalog: {args.output / 'current.json'}")


if __name__ == "__main__":
    main()
