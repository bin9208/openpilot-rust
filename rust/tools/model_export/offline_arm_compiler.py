#!/usr/bin/env python3
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# ─── How to run ───
# QEMU/ARM Python, PYTHONPATH=tinygrad_repo LLVM_QCOM_PATH=<genuine library>
# python rust/tools/model_export/offline_arm_compiler.py < kernel.cl > kernel.bin
from __future__ import annotations

import sys

from tinygrad.runtime.support.compiler_qcom import QCOMCompiler


def main() -> int:
    try:
        source = sys.stdin.buffer.read().decode()
        compiler = QCOMCompiler("a630,IMAGE_PITCH_ALIGNMENT=64")
        binary = compiler.compile(source)
        sys.stdout.buffer.write(binary)
    except (OSError, RuntimeError, UnicodeDecodeError) as error:
        print(f"{type(error).__name__}: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
