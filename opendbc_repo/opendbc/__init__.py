import os
import builtins
import json
import subprocess
from pathlib import Path

DBC_PATH = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'dbc')

# -I include path for e.g. "#include <opendbc/safety/safety.h>"
INCLUDE_PATH = os.path.abspath(os.path.join(os.path.dirname(os.path.realpath(__file__)), "../"))

_generated_dbc_cache: dict[str, str] | None = None

def native_dbc_generator() -> Path:
  """Locate the native generator; relative overrides are repository-relative."""
  root = Path(__file__).resolve().parents[2]
  configured = os.environ.get("OPENPILOT_DBC_GENERATOR")
  if configured is not None:
    path = Path(configured)
  else:
    path = Path(os.environ.get("CARGO_TARGET_DIR", "rust/target")) / "debug/dbc-generator"
  return path if path.is_absolute() else root / path


def prepare_native_dbc_generator() -> Path:
  """SCons-only bootstrap; explicit prebuilt tools are never rebuilt."""
  if "OPENPILOT_DBC_GENERATOR" not in os.environ:
    root = Path(__file__).resolve().parents[2]
    output = subprocess.check_output([root / "rust/tools/build-dbc-generator"])
    if not output.endswith(b"\n"):
      raise ValueError("native DBC bootstrap did not return an executable path")
    os.environ["OPENPILOT_DBC_GENERATOR"] = os.fsdecode(output[:-1])
  path = native_dbc_generator()
  if not path.is_file() or not os.access(path, os.X_OK):
    raise FileNotFoundError(f"Native DBC generator is not executable: {path}")
  return path


def run_dbc_generator(*arguments: str | os.PathLike) -> bytes:
  command = [native_dbc_generator(), "--json-errors", *arguments]
  result = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
  if result.returncode:
    failure = subprocess.CalledProcessError(result.returncode, command, result.stdout, result.stderr)
    # Procedural writers were separate scripts; preserve that subprocess boundary.
    if arguments and arguments[0] == "render":
      raise failure
    try:
      error = json.loads(result.stderr)
    except (ValueError, UnicodeError):
      raise failure from None
    if not isinstance(error, dict):
      raise failure
    if error.get("decode_bytes") is not None:
      bytes(error["decode_bytes"]).decode("utf-8")
    if error.get("encode_bytes") is not None:
      os.fsdecode(bytes(error["encode_bytes"])).encode("utf-8")
    name = error.get("kind")
    kind = getattr(builtins, name, None) if isinstance(name, str) else None
    if isinstance(kind, type) and issubclass(kind, OSError):
      number = error.get("errno")
      path = error.get("path_bytes")
      if number is not None and path is not None:
        raise kind(number, os.strerror(number), os.fsdecode(bytes(path)))
      raise kind(error.get("message", "native DBC generation failed"))
    if kind is ValueError:
      raise kind(error.get("message", "native DBC generation failed"))
    raise failure
  return result.stdout


def get_generated_dbcs() -> dict[str, str]:
  """Lazily generate all *_generated DBC content in memory.
  Returns {name: content} where name has no .dbc extension."""
  global _generated_dbc_cache
  if _generated_dbc_cache is None:
    _generated_dbc_cache = json.loads(run_dbc_generator("generate-all", "--source", Path(DBC_PATH) / "generator"))
  return _generated_dbc_cache
