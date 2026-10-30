from __future__ import annotations

import argparse
import json
from pathlib import Path
import shutil

from model_export.offline_compiler import CompilerCommand, OfflineQcomCompiler
from model_export.offline_pipeline import OfflinePipelineSpec
from model_export.offline_publish import publish

RESOLUTIONS = {"1928x1208": (1928, 1208), "1344x760": (1344, 760)}


def main() -> None:
    parser = argparse.ArgumentParser(description="Build native A630 pipelines from exact original ONNX bytes without opening a GPU")
    parser.add_argument("--kind", choices=("driving", "driver"), required=True)
    parser.add_argument("--model", type=Path, required=True)
    parser.add_argument("--sha256", required=True)
    parser.add_argument("--compiler-command", type=Path, required=True, help="JSON array containing the genuine ARM compiler command")
    parser.add_argument("--compiler-timeout", type=float, default=60)
    parser.add_argument("--camera", choices=tuple(RESOLUTIONS), action="append")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    command = json.loads(args.compiler_command.read_text())
    if not isinstance(command, list) or not all(isinstance(argument, str) for argument in command):
        parser.error("compiler command must be a JSON string array")
    compiler = OfflineQcomCompiler(CompilerCommand(tuple(command), args.compiler_timeout))
    specs = [OfflinePipelineSpec(args.kind, args.model, args.sha256, RESOLUTIONS[camera])
             for camera in args.camera or RESOLUTIONS]
    existing = args.output.resolve()
    while not existing.exists():
        existing = existing.parent
    if shutil.disk_usage(existing).free < 2 * 1024**3:
        parser.error("2 GiB estimated build growth required")
    publish(args.output, specs, compiler)
    print(json.dumps({"catalog": str(args.output / "current.json"), "kind": args.kind, "source_sha256": args.sha256,
                      "cameras": [spec.camera for spec in specs], "gpu_executed": False}, sort_keys=True))


if __name__ == "__main__":
    main()
