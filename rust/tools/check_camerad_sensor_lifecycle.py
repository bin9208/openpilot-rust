import argparse
import json
import os
from pathlib import Path
import subprocess

from check_camerad_kernel import digest, disable_core


def main() -> None:
  parser = argparse.ArgumentParser(description="Compare original sensor lifecycle and all submitted packet bytes")
  parser.add_argument("--source", type=Path, required=True)
  parser.add_argument("--native", type=Path, required=True)
  parser.add_argument("--fixture", type=Path, required=True)
  parser.add_argument("--output", type=Path, required=True)
  parser.add_argument("--qemu", type=Path)
  parser.add_argument("--sysroot", type=Path)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  cases = []
  for port in range(3):
    for enabled in [False, True]:
      for failed_probes in range(4):
        cases.append(
          (
            f"port-{port}-enabled-{enabled}-probe-{failed_probes}",
            port,
            enabled,
            {"CK_FAIL_OP": "camera:266", "CK_FAIL_COUNT": str(failed_probes), "CK_FAIL_ERRNO": "5"},
          )
        )
      for operation, skip in [("camera:267", 0), ("camera:261", 0), ("camera:261", 1), ("camera:261", 2), ("camera:261", 3)]:
        cases.append(
          (
            f"port-{port}-enabled-{enabled}-{operation}-{skip}",
            port,
            enabled,
            {"CK_FAIL_OP": operation, "CK_FAIL_SKIP": str(skip), "CK_FAIL_COUNT": "1", "CK_FAIL_ERRNO": "5"},
          )
        )
  for operation in ["camera:266", "camera:267", "camera:258", "camera:261"]:
    for count in [1, 100]:
      cases.append((f"retry-{operation}-{count}", 1, True, {"CK_FAIL_OP": operation, "CK_FAIL_COUNT": str(count), "CK_FAIL_ERRNO": "4"}))
  report = {"binaries": {str(p): digest(p) for p in [args.source, args.native, args.fixture]}, "cases": [], "failures": []}
  for name, port, enabled, overrides in cases:
    compared = []
    for lane, binary in [("source", args.source), ("native", args.native)]:
      folder = args.output / name / lane
      folder.mkdir(parents=True, exist_ok=True)
      trace = folder / "trace.jsonl"
      trace.unlink(missing_ok=True)
      env = {**os.environ, **overrides, "CK_TRACE": str(trace)}
      command = [str(binary), str(port), str(int(enabled))]
      if args.qemu:
        assert args.sysroot
        command = [str(args.qemu), "-L", str(args.sysroot), "-E", f"LD_PRELOAD={args.fixture}", *command]
      else:
        asan = "/usr/lib/llvm-18/lib/clang/18/lib/linux"
        env.update(
          LD_PRELOAD=f"{asan}/libclang_rt.asan-x86_64.so:{args.fixture}",
          LD_LIBRARY_PATH=asan,
          ASAN_OPTIONS="detect_leaks=0:abort_on_error=1",
          UBSAN_OPTIONS="halt_on_error=1:print_stacktrace=1",
        )
      process = subprocess.run(command, env=env, capture_output=True, text=True, timeout=30, preexec_fn=disable_core)
      (folder / "stdout").write_text(process.stdout)
      (folder / "stderr").write_text(process.stderr)
      records = [json.loads(line) for line in trace.read_text().splitlines()] if trace.exists() else []
      metadata = {
        "command": command,
        "environment": {key: value for key, value in env.items() if key.startswith(("CK_", "ASAN_", "UBSAN_", "LD_"))},
        "exit": process.returncode,
        "stdout": process.stdout,
        "stderr_sha256": digest(folder / "stderr"),
        "trace_sha256": digest(trace) if trace.exists() else None,
        "sanitizer_failure": any(text in process.stderr for text in ["ERROR: AddressSanitizer", "runtime error:", "Sanitizer CHECK failed"]),
      }
      (folder / "run.json").write_text(json.dumps(metadata, indent=2) + "\n")
      compared.append((metadata, records))
    source, native = compared
    passed = (
      source[0]["exit"] == native[0]["exit"] == 0
      and not source[0]["sanitizer_failure"]
      and not native[0]["sanitizer_failure"]
      and json.loads(source[0]["stdout"] or "null") == json.loads(native[0]["stdout"] or "null")
      and source[1] == native[1]
    )
    report["cases"].append({"name": name, "passed": passed, "calls": len(source[1]), "snapshots": sum(r["op"] == "snapshot" for r in source[1])})
    if not passed:
      differences = [(i, a, b) for i, (a, b) in enumerate(zip(source[1], native[1], strict=False)) if a != b]
      report["failures"].append(
        {"name": name, "source": source[0], "native": native[0], "lengths": [len(source[1]), len(native[1])], "first_difference": differences[:1]}
      )
    (args.output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(name, "PASS" if passed else "FAIL", flush=True)
  print(json.dumps({"cases": len(cases), "failures": len(report["failures"])}))
  raise SystemExit(bool(report["failures"]))


if __name__ == "__main__":
  main()
