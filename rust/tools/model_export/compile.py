from __future__ import annotations

import hashlib
import json
import os
import platform
import subprocess
from dataclasses import asdict
from pathlib import Path

from tinygrad import TinyJit

from .capture import capture_cpu
from .schema import Bindings, Entrypoint, Manifest, entrypoint_version


def export_cpu(jit: TinyJit, bindings: Bindings, destination: Path, *, entrypoints: tuple[Entrypoint, ...] = ()) -> None:
    captured = capture_cpu(jit, bindings)
    version = entrypoint_version(entrypoints, len(captured.calls))
    destination.mkdir(parents=True, exist_ok=False)
    source = destination / "kernels.c"
    source.write_text("#include <stdint.h>\n#include <string.h>\n" + "\n".join(captured.wrappers))
    objects = []
    for index, (body, object_code) in enumerate(zip(captured.sources, captured.objects, strict=True)):
        (destination / f"kernel-{index}.txt").write_text(body)
        if not object_code:
            continue
        object_path = destination / f"kernel-{index}.o"
        object_path.write_bytes(object_code)
        objects.append(str(object_path))
    library = destination / "kernels.so"
    subprocess.run([os.environ.get("CC", "clang"), "-shared", "-fPIC", "-O2", "-ffreestanding", "-fno-math-errno",
                    str(source), *objects, "-o", str(library), "-lm"], check=True)
    (destination / "weights.bin").write_bytes(captured.weights)
    arch = {"arm64": "aarch64", "AMD64": "x86_64"}.get(platform.machine(), platform.machine())
    manifest = Manifest(version, captured.backend, arch, hashlib.sha256(captured.weights).hexdigest(), hashlib.sha256(library.read_bytes()).hexdigest(),
                        captured.allocations, captured.views, captured.inputs, captured.outputs, captured.kernels, captured.calls, entrypoints)
    serialized = asdict(manifest)
    if not entrypoints:
        del serialized["entrypoints"]
    (destination / "graph.json").write_text(json.dumps(serialized, indent=2) + "\n")
