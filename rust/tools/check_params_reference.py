#!/usr/bin/env python3
import argparse
import fcntl
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[2]


def build_native(output: Path, capnp_prefix: Path | None) -> Path:
  schema = output / "schema"
  generated = output / "cereal/gen/cpp"
  schema.mkdir()
  generated.mkdir(parents=True)
  for name in ["log", "custom", "deprecated"]:
    shutil.copyfile(ROOT / f"openpilot/cereal/{name}.capnp", schema / f"{name}.capnp")
  shutil.copyfile(ROOT / "opendbc_repo/opendbc/car/car.capnp", schema / "car.capnp")
  shutil.copytree(ROOT / "openpilot/cereal/include", schema / "include")
  subprocess.run(["capnp", "compile", f"-I{schema}", f"--src-prefix={schema}", f"-oc++:{generated}",
                  *map(str, schema.glob("*.capnp"))], check=True)
  binary = output / "params-reference"
  command = ["g++", "-std=c++17", "-pthread", "-ffunction-sections", "-fdata-sections", "-Wl,--gc-sections",
             f"-I{ROOT / 'openpilot'}", f"-I{output}", f"-I{generated}"]
  if capnp_prefix is not None:
    command += [f"-I{capnp_prefix / 'include'}"]
  command += [str(ROOT / path) for path in ["openpilot/common/params.cc", "openpilot/common/util.cc", "rust/tools/params_reference.cc"]]
  subprocess.run([*command, "-o", str(binary)], check=True)
  return binary


def check(rust: Path, capnp_prefix: Path | None) -> None:
  with tempfile.TemporaryDirectory(prefix="rust-params-reference-") as temporary:
    directory = Path(temporary)
    native = build_native(directory, capnp_prefix)
    store = directory / "store"

    def run(binary: Path, operation: str, key: str, value: bytes = b"") -> bytes:
      return subprocess.run([str(binary), str(store), "d", operation, key], input=value, stdout=subprocess.PIPE, check=True).stdout

    native_catalog = run(native, "catalog", "-")
    assert run(rust, "catalog", "-") == native_catalog
    binary_value = bytes(range(256)) * 1024
    for writer, reader in [(native, rust), (rust, native)]:
      run(writer, "put", "CarParams", binary_value)
      assert run(reader, "get", "CarParams") == binary_value
      run(writer, "put", "CarParams", b"")
      assert (store / "d/CarParams").read_bytes() == b""
      assert run(reader, "get", "CarParams") == b""
      run(reader, "remove", "CarParams")
      assert not (store / "d/CarParams").exists()
      run(writer, "put", "CarParams", b"transient")
      run(writer, "put", "IsMetric", b"1")
      (store / "d/UnknownOldKey").write_bytes(b"old")
      run(reader, "clear", "4")
      assert not (store / "d/CarParams").exists()
      assert not (store / "d/UnknownOldKey").exists()
      assert run(writer, "get", "IsMetric") == b"1"
    for writer in [rust, native]:
      with (store / ".lock").open("rb") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        with subprocess.Popen([str(writer), str(store), "d", "put", "CarParams"], stdin=subprocess.PIPE) as process:
          assert process.stdin is not None
          process.stdin.write(b"lock parity")
          process.stdin.close()
          try:
            process.wait(timeout=0.1)
          except subprocess.TimeoutExpired:
            pass
          else:
            raise AssertionError(f"{writer} did not respect the native .lock")
          fcntl.flock(lock, fcntl.LOCK_UN)
          assert process.wait(timeout=10) == 0
      assert (store / "d/CarParams").read_bytes() == b"lock parity"
    print(f"PASS: {len(native_catalog.splitlines())} Params keys and native/Rust binary, empty, remove, clear and lock interoperability")


if __name__ == "__main__":
  parser = argparse.ArgumentParser(description="Compare Rust storage with the original C++ Params implementation")
  parser.add_argument("--binary", type=Path, default=ROOT / "rust/target/debug/examples/store")
  parser.add_argument("--capnp-prefix", type=Path)
  arguments = parser.parse_args()
  check(arguments.binary.resolve(), arguments.capnp_prefix)
