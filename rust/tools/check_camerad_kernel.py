"""Shared lifecycle helpers; kernel comparison is native check-camerad-kernel."""
import hashlib
from pathlib import Path
import resource


def digest(path: Path) -> str:
  return hashlib.sha256(path.read_bytes()).hexdigest()


def disable_core() -> None:
  resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
