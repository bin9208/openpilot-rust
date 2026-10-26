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
from .native import CachedModel
from .schema import Bindings, Entrypoint, Manifest, entrypoint_version


def export_cpu(jit: TinyJit, bindings: Bindings, destination: Path, *, entrypoints: tuple[Entrypoint, ...] = (), cached: CachedModel | None = None) -> None:
    captured = capture_cpu(jit, bindings, cached=cached)
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
    if captured.compilation:
        if len(captured.compilation) != len(captured.sources):
            raise ValueError("compiled evidence/source alignment differs")
        evidence = {}
        for index, item in enumerate(captured.compilation):
            if item is None:
                continue
            compiled = item.compiled_source.encode("utf-8")
            (destination / f"kernel-{index}.compiled.ll").write_bytes(compiled)
            policy = item.binding.get("source_policy")
            if policy == "preserve-original-flags":
                (destination / f"kernel-{index}.source-policy.json").write_bytes(item.rewrite_receipt)
            elif policy is None:
                (destination / f"kernel-{index}.guarded-add.json").write_bytes(item.rewrite_receipt)
            else:
                raise ValueError("unknown compiled source policy")
            evidence[str(index)] = item.binding | {"compiled_source_sha256":hashlib.sha256(compiled).hexdigest()}
        if evidence:
            (destination / "compilation-evidence.json").write_text(json.dumps({"version":1, "kernels":evidence}, sort_keys=True, indent=2) + "\n")
    if cached is not None:
        package = cached.package(captured, destination)
        (destination / "cached-container.o").write_bytes(package["object"])
        (destination / "cached-isa.json").write_bytes(package["isa-receipt"])
        (destination / "cached-admission.json").write_bytes(package["admission"])
        objects.append(str(destination / "cached-container.o"))
        (destination / "cached-preflight-request.json").write_bytes(cached.session.preflight["request"])
        (destination / "cached-preflight-status.json").write_bytes(cached.session.preflight["status"])
        (destination / "cached-native-binding.json").write_text(json.dumps(cached.session.receipt(), sort_keys=True, indent=2) + "\n")
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
    if cached is not None:
        serialized["version"] = 3
        serialized["cached_code"] = {"certificate_sha256":"0" * 64,
                                     "kernels":[row.kernel for row in sorted(captured.cached_programs, key=lambda row:row.original_index)]}
    (destination / "graph.json").write_text(json.dumps(serialized, indent=2) + "\n")
    if cached is not None:
        sealed = cached.package(captured, destination, final=True)
        if any(sealed[name] != package[name] for name in ("object", "isa-receipt", "admission")):
            raise ValueError("native cached preflight/package/seal evidence changed")
        (destination / "cached-code.json").write_bytes(sealed["certificate"])
        serialized["cached_code"]["certificate_sha256"] = hashlib.sha256(sealed["certificate"]).hexdigest()
        (destination / "graph.json").write_text(json.dumps(serialized, indent=2) + "\n")
