#!/usr/bin/env python3
# /// script
# requires-python = ">=3.12"
# dependencies = ["pycapnp==2.1.0"]
# ///
"""Compare Rust wire fields with the original Python collector on synthetic procfs."""

import argparse
import ast
import builtins
import logging
import os
import runpy
from pathlib import Path
import subprocess
import tempfile
from types import ModuleType
from unittest.mock import patch

import capnp

ROOT = Path(__file__).resolve().parents[2]


def reference() -> ModuleType:
  path = ROOT / "openpilot/system/proclogd.py"
  tree = ast.parse(path.read_text(), filename=str(path))
  tree.body = [node for node in tree.body
               if not (isinstance(node, ast.ImportFrom) and (node.module or "").startswith("openpilot"))
               and not isinstance(node, ast.If)
               and not (isinstance(node, ast.FunctionDef) and node.name == "main")]
  module = ModuleType("proclogd_reference")
  module.cloudlog = logging.getLogger("proclogd_reference")
  exec(compile(tree, str(path), "exec"), module.__dict__)
  return module


def make_process(root: Path, pid: int, rss: int) -> None:
  directory = root / str(pid)
  directory.mkdir()
  fields = ["0"] * 50
  for index, value in {0: "S", 1: "1", 11: "125", 12: "27", 13: "-3", 14: "4",
                       15: "20", 16: "-5", 17: "3", 19: "501", 20: "16777216",
                       21: str(rss), 36: "6"}.items():
    fields[index] = value
  (directory / "stat").write_text(f"{pid} (worker ) (a) {' '.join(fields)}")
  (directory / "cmdline").write_bytes(b"worker\0--test\0\xe1\x90\xff\0")
  (directory / "exe").symlink_to("/synthetic/bin/worker")
  (directory / "smaps_rollup").write_text("Pss: 50 kB\nPss_Anon: 30 kB\nPss_Shmem: 10 kB\n")


def check(binary: Path, fallback: bool) -> None:
  services = runpy.run_path(str(ROOT / "openpilot/cereal/services.py"))["SERVICE_LIST"]
  assert int(subprocess.check_output([str(binary), "--queue-size"])) == services["procLog"].queue_size
  schema = capnp.load(str(ROOT / "openpilot/cereal/log.capnp"),
                      imports=[str(ROOT / "openpilot/cereal"), str(ROOT / "opendbc_repo/opendbc/car")])
  original = reference()
  real_open, real_listdir, real_readlink, real_exists = builtins.open, os.listdir, os.readlink, os.path.exists
  with tempfile.TemporaryDirectory(prefix="rust-proclog-reference-") as temporary:
    root = Path(temporary)
    (root / "stat").write_text("cpu 9 9 9 9 9 9 9\ncpu0 100 2 30 400 5 6 7\ncpu7 200 3 40 500 6 7 8\nintr 44\n")
    (root / "meminfo").write_text("MemTotal: 128 kB\nMemFree: 16 kB\nMemAvailable: 32 kB\nBuffers: 2 kB\nCached: 4 kB\nActive: 5 kB\nInactive: 6 kB\nShmem: 7 kB\n")
    make_process(root, 123, 2000)
    make_process(root, 456, 1)
    smaps_file = "smaps" if fallback else "smaps_rollup"
    if fallback:
      for pid in [123, 456]:
        (root / str(pid) / "smaps_rollup").unlink()
        (root / str(pid) / "smaps").write_bytes(b"1000-2000 r--p 00000000 00:00 0 /synthetic/\xff\nPss: 50 kB\nPss_Anon: 30 kB\nPss_Shmem: 10 kB\n")

    def mapped(path: str) -> str:
      if path == "/proc" or path.startswith("/proc/"):
        return str(root) + path[5:]
      return path

    with subprocess.Popen([str(binary), str(root), str(original.JIFFY), str(original.PAGE_SIZE)],
                          stdin=subprocess.PIPE, stdout=subprocess.PIPE) as process:
      assert process.stdin is not None and process.stdout is not None
      for cycle in range(22):
        if cycle == 1:
          (root / "123" / smaps_file).write_text("Pss: 99 kB\nPss_Anon: 80 kB\nPss_Shmem: 4 kB\n")
        timestamp = 1000 + cycle
        process.stdin.write(f"{timestamp}\n".encode())
        process.stdin.flush()
        size = int.from_bytes(process.stdout.read(8), "little")
        assert 0 < size < 1_000_000, size
        payload = process.stdout.read(size)
        expected = schema.Event.new_message(logMonoTime=timestamp, valid=True)
        expected.init("procLog")
        with patch("builtins.open", lambda path, mode="r": real_open(mapped(path), mode)), \
             patch("os.listdir", lambda path: real_listdir(mapped(path))), \
             patch("os.readlink", lambda path: real_readlink(mapped(path))), \
             patch("os.path.exists", lambda path: real_exists(mapped(path))):
          original.build_proc_log_message(expected)
        with schema.Event.from_bytes(payload) as observed:
          actual, wanted = observed.to_dict(), expected.to_dict()
          actual["procLog"]["procs"].sort(key=lambda item: item["pid"])
          wanted["procLog"]["procs"].sort(key=lambda item: item["pid"])
          assert actual == wanted, (cycle, actual, wanted)
          pss = actual["procLog"]["procs"][0]["memPss"]
          assert pss == (50 if cycle < 20 else 99) * 1024, (cycle, pss)
      process.stdin.close()
      assert process.wait(timeout=10) == 0
  print(f"PASS: every Event/procLog field matches the original Python implementation for 22 {smaps_file} cache cycles")


if __name__ == "__main__":
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument("--binary", type=Path, default=ROOT / "rust/target/debug/examples/reference_trace")
  binary = parser.parse_args().binary.resolve()
  check(binary, fallback=False)
  check(binary, fallback=True)
