"""Owned filesystem comparison against the unchanged original extraction block."""

from __future__ import annotations

import argparse
import ast
from hashlib import sha256
import io
import json
import os
from pathlib import Path
import stat
import subprocess
import tarfile
import tempfile
from types import CodeType
from typing import TypedDict

from check_usbgpu_provision_contract import Json, Native


class ArchiveFile(TypedDict):
  path: str
  mode: int
  bytes: int
  directory: bool


class ArchiveResult(TypedDict):
  case: str
  equal: bool
  source_error: str | None
  source_files: list[ArchiveFile]
  native: Native
  native_files: list[ArchiveFile]
  native_argv: list[str]
  native_request: dict[str, Json]
  native_exit: int
  native_stderr: str
  residual_staging: list[str]


def source_block() -> tuple[CodeType, str, str]:
  path = Path(__file__).resolve().parents[2] / "openpilot/selfdrive/modeld/precompiled_model.py"
  source = path.read_text()
  function = next(node for node in ast.parse(source).body if isinstance(node, ast.FunctionDef) and node.name == "ensure_precompiled")
  branch = next(node for node in ast.walk(function) if isinstance(node, ast.If) and ast.unparse(node.test) == "not runtime.exists()")
  return compile(ast.Module(body=branch.body, type_ignores=[]), str(path), "exec"), str(path), sha256(source.encode()).hexdigest()


def snapshot(root: Path) -> list[ArchiveFile]:
  rows = []
  if root.exists():
    for path in sorted(root.rglob("*")):
      metadata = path.lstat()
      rows.append(
        {
          "path": str(path.relative_to(root)),
          "mode": stat.S_IMODE(metadata.st_mode),
          "bytes": metadata.st_size if stat.S_ISREG(metadata.st_mode) else 0,
          "directory": stat.S_ISDIR(metadata.st_mode),
        }
      )
  return rows


def run_case(args: argparse.Namespace, compiled: CodeType, name: str, entries: list[tuple[str, bytes, int, int]]) -> ArchiveResult:
  root = args.evidence / name
  root.mkdir()
  archive = root / "runtime.tar.gz"
  with tarfile.open(archive, "w:gz") as writer:
    for path, kind, mode, size in entries:
      item = tarfile.TarInfo(path)
      item.type, item.mode, item.size = kind, mode, size
      if kind == tarfile.SYMTYPE:
        item.linkname = "owned.bin"
      if size > 1024:
        # A real bounded compressed payload proves the expanded-size rejection.
        class ZeroReader:
          def read(self, amount: int) -> bytes:
            return bytes(amount)

        writer.addfile(item, ZeroReader())
      else:
        writer.addfile(item, io.BytesIO(b"owned"[:size]))
  for side in ("source", "native"):
    (root / side).mkdir()
  os.link(archive, root / "source/runtime.tar.gz")
  source_destination = root / "source/runtime"
  original_error = None
  try:
    exec(compiled, {"root": root / "source", "runtime": source_destination, "tempfile": tempfile, "tarfile": tarfile, "os": os})
  except (OSError, ValueError, tarfile.TarError) as error:
    original_error = f"{type(error).__name__}: {error}"
  model = {"model_id": "owned", "filename": "model.pkl", "size": 1, "sha256": "a" * 64, "url": "https://owned.invalid/model.pkl"}
  request = {"action": "archive", "model": model, "cache": str(root / "native/runtime"), "ca": str(args.ca), "value": str(archive)}
  process = subprocess.run([str(args.binary)], input=json.dumps(request), capture_output=True, text=True, timeout=35, check=False)
  actual = json.loads(process.stdout)
  source_files, native_files = snapshot(source_destination), snapshot(root / "native/runtime")
  residual = [path.name for path in (root / "native").iterdir() if path.name.startswith(".tmp")]
  equal = process.returncode == 0 and (original_error is not None) == ("error" in actual) and source_files == native_files and not residual
  return {
    "case": name,
    "equal": equal,
    "source_error": original_error,
    "source_files": source_files,
    "native": actual,
    "native_files": native_files,
    "native_argv": [str(args.binary)],
    "native_request": request,
    "native_exit": process.returncode,
    "native_stderr": process.stderr,
    "residual_staging": residual,
  }


def main() -> None:
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument("--binary", type=Path, required=True)
  parser.add_argument("--ca", type=Path, required=True)
  parser.add_argument("--evidence", type=Path, required=True)
  args = parser.parse_args()
  args.evidence.mkdir(parents=True)
  compiled, path, source_sha = source_block()
  cases = [
    ("absolute", [("/owned.bin", tarfile.REGTYPE, 0o644, 5)]),
    ("dotdot", [("a", tarfile.DIRTYPE, 0o755, 0), ("a/../owned.bin", tarfile.REGTYPE, 0o644, 5)]),
    ("escape", [("../escape.bin", tarfile.REGTYPE, 0o644, 5)]),
    ("directory-mode000", [("a", tarfile.DIRTYPE, 0, 0), ("a/owned.bin", tarfile.REGTYPE, 0o644, 5)]),
    ("file-mode000", [("owned.bin", tarfile.REGTYPE, 0, 5)]),
    ("group-execute", [("owned.bin", tarfile.REGTYPE, 0o010, 5)]),
    ("symlink", [("owned", tarfile.SYMTYPE, 0o644, 0)]),
    ("special", [("owned", tarfile.FIFOTYPE, 0o644, 0)]),
    ("expanded-limit", [("owned.bin", tarfile.REGTYPE, 0o644, (256 << 20) + 1)]),
  ]
  rows = [run_case(args, compiled, name, entries) for name, entries in cases]
  result = {
    "source_path": path,
    "source_sha256": source_sha,
    "binary_sha256": sha256(args.binary.read_bytes()).hexdigest(),
    "rows": rows,
    "differences": [row["case"] for row in rows if not row["equal"]],
  }
  (args.evidence / "result.json").write_text(json.dumps(result, indent=2) + "\n")
  print(json.dumps({"cases": len(rows), "differences": result["differences"]}))
  if result["differences"]:
    raise AssertionError(result["differences"])


if __name__ == "__main__":
  main()
