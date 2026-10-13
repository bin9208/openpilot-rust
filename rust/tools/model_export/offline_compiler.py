from __future__ import annotations

import math
import subprocess
from dataclasses import dataclass

from tinygrad.device import Compiler

from .schema import ExportError


class CompilerFailure(ExportError):
    def __init__(self, phase: str, detail: str) -> None:
        self.phase = phase
        self.detail = detail
        super().__init__(f"offline QCOM compiler {phase}: {detail}")


@dataclass(frozen=True, slots=True)
class CompilerCommand:
    argv: tuple[str, ...]
    timeout_seconds: float

    def __post_init__(self) -> None:
        if not self.argv or any(not argument for argument in self.argv):
            raise CompilerFailure("configuration", "nonempty compiler command required")
        if not math.isfinite(self.timeout_seconds) or self.timeout_seconds <= 0:
            raise CompilerFailure("configuration", "positive finite timeout required")


class OfflineQcomCompiler(Compiler):
    """Invoke the genuine A630 compiler in a separate ARM process, never a GPU device."""

    def __init__(self, command: CompilerCommand) -> None:
        super().__init__(None)
        self.command = command

    def compile(self, source: str) -> bytes:
        try:
            result = subprocess.run(
                self.command.argv,
                input=source.encode(),
                capture_output=True,
                timeout=self.command.timeout_seconds,
                check=False,
            )
        except subprocess.TimeoutExpired as error:
            raise CompilerFailure("timeout", str(self.command.timeout_seconds)) from error
        except OSError as error:
            raise CompilerFailure("process", str(error)) from error
        if result.returncode != 0:
            raise CompilerFailure("process", result.stderr.decode(errors="replace"))
        if not result.stdout:
            raise CompilerFailure("output", "compiler returned no binary")
        return result.stdout
