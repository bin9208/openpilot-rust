#!/usr/bin/env python3
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# ─── How to run ───
# python rust/tools/stage_qcom_compiler.py OUTPUT PYTHON_ARCHIVE LIBLLVM_QCOM QEMU CROSS_LIB_DIR
"""Stage verified ARM compiler inputs and a host-only QEMU command for CI."""
from __future__ import annotations

from dataclasses import dataclass
import hashlib
import json
from pathlib import Path
import shutil
import sys
import tarfile
from typing import Final, TypedDict

from card_ci_space import require_space

PYTHON_URL: Final = (
    "https://github.com/astral-sh/python-build-standalone/releases/download/20260929/"
    + "cpython-3.12.14%2B20260929-aarch64-unknown-linux-gnu-install_only.tar.gz"
)
PYTHON_SHA256: Final = "9c797cf657f6dced51d3e74eeabd7c1ca742d5bf10f66080a290e424ced8edaf"
COMPILER_URL: Final = (
    "https://raw.githubusercontent.com/commaai/agnos-builder/ec1cf237a84565a056dbbc6f433b1b1c20c07a2c/"
    + "userspace/root/usr/lib/aarch64-linux-gnu/libllvm-qcom.so"
)
COMPILER_SHA256: Final = "fb7e6390cc25700d6935b2eef3acad85a43f247206bdf84cb4d37bd48a60b093"
GROWTH_BYTES: Final = 1024**3


@dataclass(frozen=True, slots=True)
class InvalidProvider(ValueError):
    reason: str

    def __str__(self) -> str:
        return self.reason


@dataclass(frozen=True, slots=True)
class Providers:
    python_archive: Path
    compiler: Path
    qemu: Path
    libraries: Path


class Provenance(TypedDict):
    python_url: str
    python_sha256: str
    compiler_url: str
    compiler_sha256: str
    qemu_sha256: str
    loader_sha256: str
    command: list[str]
    gpu_executed: bool


def verify_file(path: Path, expected: str) -> str:
    """Reject tampered provider bytes before extraction or execution."""
    with path.open("rb") as stream:
        actual = hashlib.file_digest(stream, "sha256").hexdigest()
    if actual != expected:
        raise InvalidProvider(f"SHA256 mismatch: {path}: {actual} != {expected}")
    return actual


def require_arm_elf(path: Path) -> None:
    """Require little-endian 64-bit AArch64 executable/shared-object inputs."""
    with path.open("rb") as stream:
        header = stream.read(20)
    if header[:6] != b"\x7fELF\x02\x01" or int.from_bytes(header[18:20], "little") != 183:
        raise InvalidProvider(f"expected AArch64 ELF: {path}")


def extract_python(archive: Path, output: Path) -> None:
    """Apply tar's data filter after bounding paths and uncompressed size."""
    with tarfile.open(archive) as source:
        members = source.getmembers()
        if sum(member.size for member in members) > GROWTH_BYTES // 2:
            raise InvalidProvider("Python archive exceeds staging growth bound")
        for member in members:
            path = Path(member.name)
            if path.is_absolute() or ".." in path.parts or path.parts[:1] != ("python",):
                raise InvalidProvider(f"unexpected Python archive member: {member.name}")
        source.extractall(output, members=members, filter="data")


def stage(output: Path, providers: Providers) -> list[str]:
    """Emit a genuine compiler command without initializing any GPU backend."""
    require_space(output.parent, GROWTH_BYTES)
    verify_file(providers.python_archive, PYTHON_SHA256)
    verify_file(providers.compiler, COMPILER_SHA256)
    require_arm_elf(providers.compiler)
    loader = providers.libraries.resolve(strict=True) / "ld-linux-aarch64.so.1"
    require_arm_elf(loader)
    qemu = providers.qemu.resolve(strict=True)
    output.mkdir(parents=True, exist_ok=False)
    extract_python(providers.python_archive, output)
    python = output / "python"
    require_arm_elf(python / "bin/python3.12")
    compiler = output / "libllvm-qcom.so"
    shutil.copyfile(providers.compiler, compiler)
    root = Path(__file__).resolve().parents[2]
    command = [
        "env", f"PYTHONHOME={python}", f"PYTHONPATH={root / 'tinygrad_repo'}", f"LLVM_QCOM_PATH={compiler}", "DEV=CPU:LLVM",
        str(qemu), "-cpu", "cortex-a57", str(loader), "--library-path", f"{python / 'lib'}:{providers.libraries.resolve()}",
        str(python / "bin/python3.12"), str(root / "rust/tools/model_export/offline_arm_compiler.py"),
    ]
    with qemu.open("rb") as stream:
        qemu_hash = hashlib.file_digest(stream, "sha256").hexdigest()
    with loader.open("rb") as stream:
        loader_hash = hashlib.file_digest(stream, "sha256").hexdigest()
    provenance: Provenance = {
        "python_url": PYTHON_URL, "python_sha256": PYTHON_SHA256,
        "compiler_url": COMPILER_URL, "compiler_sha256": COMPILER_SHA256,
        "qemu_sha256": qemu_hash, "loader_sha256": loader_hash, "command": command, "gpu_executed": False,
    }
    (output / "command.json").write_text(json.dumps(command) + "\n")
    (output / "provenance.json").write_text(json.dumps(provenance, indent=2) + "\n")
    return command


def main() -> None:
    if len(sys.argv) != 6:
        raise InvalidProvider("usage: OUTPUT PYTHON_ARCHIVE LIBLLVM_QCOM QEMU CROSS_LIB_DIR")
    output, archive, compiler, qemu, libraries = (Path(value).resolve() for value in sys.argv[1:])
    print(json.dumps(stage(output, Providers(archive, compiler, qemu, libraries))))


if __name__ == "__main__":
    main()
