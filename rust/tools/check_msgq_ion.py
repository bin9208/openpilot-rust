import argparse
import hashlib
import json
import os
from pathlib import Path
import resource
import subprocess
import tempfile


def digest(path: Path) -> str:
  return hashlib.sha256(path.read_bytes()).hexdigest()


def disable_core() -> None:
  resource.setrlimit(resource.RLIMIT_CORE, (0, 0))


def cases() -> list[tuple[str, dict[str, str], bool, int]]:
  result = [("normal", {}, True, 0)]
  for operation in ("allocate", "share", "import", "invalidate", "clean", "free"):
    for error, count in ((4, 1), (4, 100), (4, 101), (5, 1), (11, 1)):
      fatal = operation in ("allocate", "share", "import") and (error != 4 or count == 101)
      retained_handle = int(operation == "free" and (error != 4 or count == 101))
      environment = {"IPC_ION_FAIL": operation, "IPC_ION_ERRNO": str(error), "IPC_ION_COUNT": str(count)}
      result.append((f"{operation}-{error}-{count}", environment, not fatal, retained_handle))
    result.append((f"{operation}-positive-status", {"IPC_ION_FAIL": operation, "IPC_ION_ERRNO": "4", "IPC_ION_RETURN": "1"},
                   operation not in ("allocate", "share", "import"), int(operation == "free")))
  for skip in (0, 1):
    result.append((f"mmap-failure-{skip}", {"IPC_ION_FAIL": "mmap", "IPC_ION_ERRNO": "12", "IPC_ION_SKIP": str(skip)}, False, 0))
  result.append(("open-failure", {"IPC_ION_FAIL": "open", "IPC_ION_ERRNO": "13"}, False, 0))
  return result


def main() -> None:
  parser = argparse.ArgumentParser(description="Compare original and Rust VisionIPC ION calls through a host driver fixture")
  parser.add_argument("--source", type=Path, required=True)
  parser.add_argument("--native", type=Path, required=True)
  parser.add_argument("--fixture", type=Path, required=True)
  parser.add_argument("--output", type=Path, required=True)
  parser.add_argument("--qemu", type=Path)
  parser.add_argument("--sysroot", type=Path)
  parser.add_argument("--asan-runtime", type=Path)
  parser.add_argument("--normal-only", action="store_true")
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  binaries = [args.source.resolve(), args.native.resolve(), args.fixture.resolve()]
  if args.qemu:
    if not args.sysroot:
      parser.error("--qemu requires --sysroot")
    binaries.append(args.qemu.resolve())
  report = {"binaries": {str(path): digest(path) for path in binaries}, "cases": [], "failures": []}
  selected = cases()[:1] if args.normal_only else cases()
  for name, overrides, should_succeed, retained_handles in selected:
    lanes = []
    for lane, binary in (("source", args.source), ("native", args.native)):
      folder = args.output / name / lane
      folder.mkdir(parents=True, exist_ok=True)
      trace = (folder / "trace.jsonl").resolve()
      trace.unlink(missing_ok=True)
      with tempfile.TemporaryDirectory(prefix="msgq_rust-i194-", dir="/dev/shm") as namespace:
        prefix = Path(namespace).name.removeprefix("msgq_")
        environment = {key: value for key, value in os.environ.items()
                       if not key.startswith(("IPC_ION_", "QEMU_")) and key not in ("CEREAL_FAKE", "LD_PRELOAD", "LD_LIBRARY_PATH")}
        environment.update(overrides, OPENPILOT_PREFIX=prefix, IPC_ION_TRACE=str(trace),
                           ASAN_OPTIONS="detect_leaks=1:halt_on_error=1:abort_on_error=1", UBSAN_OPTIONS="halt_on_error=1:print_stacktrace=1")
        preload = str(args.fixture.resolve())
        if args.asan_runtime:
          preload = f"{args.asan_runtime.resolve()}:{preload}"
        command = [str(binary.resolve())]
        if args.qemu:
          command = [str(args.qemu.resolve()), "-L", str(args.sysroot.resolve()), "-E", f"LD_PRELOAD={preload}", *command]
        else:
          environment["LD_PRELOAD"] = preload
        try:
          process = subprocess.run(command, env=environment, capture_output=True, text=True, timeout=15, preexec_fn=disable_core)
        finally:
          Path(f"/tmp/{prefix}_visionipc_ioncontract").unlink(missing_ok=True)
      (folder / "stdout.log").write_text(process.stdout)
      (folder / "stderr.log").write_text(process.stderr)
      records = [json.loads(line) for line in trace.read_text().splitlines()] if trace.exists() else []
      summary = next((record for record in records if record["op"] == "summary"), None)
      events = [record for record in records if record["op"] not in ("summary", "device_close")]
      results = [json.loads(line) for line in process.stdout.splitlines() if line.startswith('{"frame_id":')]
      metadata = {"command": command, "environment": {key: value for key, value in environment.items()
                  if key.startswith(("IPC_ION_", "ASAN_", "UBSAN_", "LD_"))},
                  "returncode": process.returncode, "stdout": process.stdout, "result": results, "summary": summary,
                  "trace_sha256": digest(trace) if trace.exists() else None,
                  "sanitizer_error": any(text in process.stderr for text in ("ERROR: AddressSanitizer", "runtime error:", "Sanitizer CHECK failed"))}
      (folder / "run.json").write_text(json.dumps(metadata, indent=2) + "\n")
      lanes.append((metadata, events))
    source, native = lanes
    if should_succeed:
      expected_summary = {"op": "summary", "handles": retained_handles, "mappings": 0}
      passed = (source[0]["returncode"] == native[0]["returncode"] == 0
                and source[0]["result"] == native[0]["result"] == [{"frame_id": 42, "checksum": 490432, "raw_sum": 448}]
                and source[1] == native[1] and source[0]["summary"] == native[0]["summary"] == expected_summary)
    else:
      passed = (source[0]["returncode"] != 0 and native[0]["returncode"] != 0
                and any("error" in record for record in source[1])
                and native[1][:len(source[1])] == source[1]
                and native[0]["summary"] == {"op": "summary", "handles": 0, "mappings": 0})
    passed = passed and not source[0]["sanitizer_error"] and not native[0]["sanitizer_error"]
    entry = {"name": name, "passed": passed, "should_succeed": should_succeed,
             "source_calls": len(source[1]), "native_calls": len(native[1]), "environment": overrides}
    report["cases"].append(entry)
    if not passed:
      report["failures"].append({**entry, "source": source[0], "native": native[0], "source_events": source[1], "native_events": native[1]})
  (args.output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
  print(json.dumps({"cases": len(report["cases"]), "failures": [case["name"] for case in report["failures"]]}, indent=2))
  if report["failures"]:
    raise SystemExit(1)


if __name__ == "__main__":
  main()
