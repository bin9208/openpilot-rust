import os
import builtins
import json
import subprocess
import hashlib
import stat
from pathlib import Path

SOURCE_DBC_PATH = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'dbc')


def _checked_build_root(create=False) -> Path | None:
  """Private SCons/CI output contract; an unset root preserves the old runtime."""
  value = os.environ.get("OPENPILOT_BUILD_ASSET_ROOT")
  if value is None:
    return None
  path = Path(value)
  repository = Path(__file__).resolve().parents[2]
  if not path.is_absolute() or path.resolve() != path or ".." in path.parts:
    raise ValueError("build asset root must be absolute and canonical")
  if any(character in value for character in "\"'`$\\\n\r"):
    raise ValueError("build asset root contains unsupported compiler-argument characters")
  if path == repository or path.is_relative_to(repository) or repository.is_relative_to(path):
    raise ValueError("build asset root overlaps source")
  if create:
    path.mkdir(mode=0o700, parents=True, exist_ok=True)
  if path.exists():
    metadata = path.lstat()
    protected = [repository, Path(SOURCE_DBC_PATH).resolve(), Path(SOURCE_DBC_PATH).resolve().parent,
                 repository / "openpilot/selfdrive/assets", repository / "openpilot/selfdrive/assets/fonts"]
    if any((item.stat().st_dev, item.stat().st_ino) == (metadata.st_dev, metadata.st_ino) for item in protected):
      raise ValueError("build asset root aliases source identity")
    if not stat.S_ISDIR(metadata.st_mode) or metadata.st_uid != os.getuid() or stat.S_IMODE(metadata.st_mode) != 0o700:
      raise ValueError("build asset root must be an owned private directory")
  return path


def _asset_hash(path: Path) -> str:
  descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
  with os.fdopen(descriptor, "rb") as stream:
    before = os.fstat(stream.fileno())
    if not stat.S_ISREG(before.st_mode) or before.st_size > 512 * 1024 * 1024:
      raise ValueError(f"asset is not a bounded regular file: {path}")
    result = hashlib.file_digest(stream, "sha256").hexdigest()
    after = os.fstat(stream.fileno())
    current = path.lstat()
    identity = lambda row: (row.st_dev, row.st_ino, row.st_mode, row.st_size, row.st_mtime_ns, row.st_ctime_ns)
    if identity(before) != identity(after) or identity(after) != identity(current):
      raise ValueError(f"asset changed while reading: {path}")
    return result


def _dbc_inputs() -> dict[str, str]:
  source = Path(SOURCE_DBC_PATH).resolve()
  repository = Path(__file__).resolve().parents[2]
  files = list(source.glob("*.dbc"))
  for path in source.joinpath("generator").rglob("*"):
    if path.is_symlink():
      raise ValueError("DBC input symlink is not supported by staging")
    if path.is_file():
      files.append(path)
  files += [Path(__file__).resolve(), source / "SConscript"]
  native = repository / "rust/crates/dev-tools/src"
  files += [native / "dbc_generator.rs", *native.joinpath("dbc_generator").rglob("*.rs")]
  return {str(path.relative_to(repository)): _asset_hash(path) for path in sorted(files)}


def _selected_dbc_path() -> str:
  value = os.environ.get("OPENPILOT_DBC_ROOT")
  if value is None:
    return SOURCE_DBC_PATH
  base = _checked_build_root()
  if base is None or value != str(base / "dbc"):
    raise ValueError("DBC root differs from build output")
  return value


DBC_PATH = _selected_dbc_path()


def _validate_dbc_stage() -> None:
  if "OPENPILOT_DBC_ROOT" not in os.environ:
    return
  path = Path(_selected_dbc_path())
  if path.is_symlink() or path.resolve() != path:
    raise ValueError("DBC output must be canonical")
  if path.exists():
    metadata = path.lstat()
    if not stat.S_ISDIR(metadata.st_mode) or metadata.st_uid != os.getuid() or stat.S_IMODE(metadata.st_mode) != 0o700:
      raise ValueError("DBC output must be an owned private directory")
  marker = path / ".ready"
  if not marker.is_file() or marker.is_symlink() or marker.stat().st_size != len(b"openpilot-scons-dbc-v1\n") or marker.read_bytes() != b"openpilot-scons-dbc-v1\n":
    raise ValueError("DBC stage is not ready")
  manifest_path = path / "manifest.json"
  _asset_hash(manifest_path)
  if manifest_path.stat().st_size > 4 * 1024 * 1024:
    raise ValueError("DBC manifest exceeds bound")
  manifest = json.loads(manifest_path.read_bytes())
  if type(manifest.get("format_version")) is not int or manifest["format_version"] != 1 or manifest.get("kind") != "scons-dbc" or manifest.get("source_sha256") != _dbc_inputs():
    raise ValueError("DBC source or role differs from stage")
  if manifest.get("generator_sha256") != _asset_hash(native_dbc_generator()):
    raise ValueError("DBC generator differs from stage")
  actual = {file.name: _asset_hash(file) for file in sorted(path.glob("*.dbc"))}
  if actual != manifest.get("asset_sha256"):
    raise ValueError("DBC output roster or bytes changed")

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
    source = SOURCE_DBC_PATH if "OPENPILOT_DBC_ROOT" in os.environ else DBC_PATH
    _generated_dbc_cache = json.loads(run_dbc_generator("generate-all", "--source", Path(source) / "generator"))
  return _generated_dbc_cache
