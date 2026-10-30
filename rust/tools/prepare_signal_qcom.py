"""Build the two pinned signal graphs with the genuine offline A630 compiler.

This is a package producer. It never opens KGSL or installs Python on a device.
The supplied original tinygrad tree is authenticated before importing it.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

TREE = "2c7501785916e365318c31976d4aef5b6e44a332"
COMPILER = "fb7e6390cc25700d6935b2eef3acad85a43f247206bdf84cb4d37bd48a60b093"
ROOT = Path(__file__).resolve().parents[2]


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def authenticate_tree(root: Path) -> None:
    entries = subprocess.check_output(["git", "-C", str(ROOT), "ls-tree", "-rz", TREE]).split(b"\0")
    if len(entries) < 100:
        raise ValueError("original tinygrad tree is unavailable")
    for entry in filter(None, entries):
        metadata, name = entry.split(b"\t", 1)
        mode, kind, expected = metadata.split()
        path = root / name.decode()
        if kind != b"blob" or mode not in (b"100644", b"100755") or path.is_symlink():
            raise ValueError("unsupported original tinygrad entry")
        data = path.read_bytes()
        if hashlib.sha1(b"blob " + str(len(data)).encode() + b"\0" + data).hexdigest().encode() != expected:
            raise ValueError(f"original tinygrad identity differs: {name.decode()}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tinygrad-root", type=Path, required=True)
    parser.add_argument("--models", type=Path, required=True)
    parser.add_argument("--compiler-command", type=Path, required=True, help="JSON argv; real compiler, optionally under QEMU")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    authenticate_tree(args.tinygrad_root)
    argv = json.loads(args.compiler_command.read_text())
    if not isinstance(argv, list) or "--library" not in argv:
        raise ValueError("genuine compiler library argument required")
    library = Path(argv[argv.index("--library") + 1])
    if digest(library) != COMPILER:
        raise ValueError("Qualcomm compiler differs from the pinned AGNOS compiler")
    if args.output.exists():
        raise ValueError("output must be a new directory")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    if shutil.disk_usage(args.output.parent).free < 2**30:
        raise ValueError("export requires 1 GiB estimated growth")
    sys.path.insert(0, str(args.tinygrad_root.resolve()))
    from model_export.offline_onnx import load_model
    from model_export.offline_schedule import export_model
    from model_export.offline_compiler import CompilerCommand, OfflineQcomCompiler

    pins = json.loads((ROOT / "rust/upstream/signal-runtime.json").read_text())
    producer = {str(path.relative_to(ROOT)): digest(path) for path in sorted((ROOT / "rust/tools/model_export").glob("*.py"))}
    producer[str(Path(__file__).resolve().relative_to(ROOT))] = digest(Path(__file__))
    compiler = OfflineQcomCompiler(CompilerCommand(tuple(argv), 120))
    with tempfile.TemporaryDirectory(prefix=".signal-", dir=args.output.parent) as temporary:
        stage = Path(temporary) / "signal-qcom"
        stage.mkdir()
        for name, pin in pins["models"].items():
            model = load_model(args.models / f"{name}.onnx", pin["sha256"], float16=0)
            if (list(model.bindings.inputs) != [pin["input"]] or list(model.bindings.outputs) != [pin["output"]]
                or list(model.bindings.inputs[pin["input"]].shape) != pin["shape"]
                or list(model.bindings.outputs[pin["output"]].shape) != pin["output_shape"]):
                raise ValueError(f"{name} fixed bindings differ from the pinned contract")
            directory = stage / name
            compiled = export_model(model, compiler, directory, float16=0)
            identity = dict(schema=1, source_commit=pins["source_commit"], tinygrad_tree=TREE, model=name,
                            model_sha256=pin["sha256"], input=pin["input"], input_shape=pin["shape"],
                            output=pin["output"], output_shape=pin["output_shape"], float16=0)
            receipt = dict(identity=identity, compiler_sha256=COMPILER, producer=producer,
                           graph_sha256=digest(directory / "graph.json"), kernels=len(compiled.manifest.kernels),
                           hardware_execution="not performed by the offline producer")
            (directory / "signal-qcom.json").write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n")
            print(f"{name}: {len(compiled.manifest.kernels)} genuine A630 kernels", flush=True)
        stage.rename(args.output)


if __name__ == "__main__":
    main()
