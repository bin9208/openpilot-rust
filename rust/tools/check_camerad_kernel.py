import argparse
import hashlib
import json
import os
from pathlib import Path
import resource
import subprocess


def digest(path: Path) -> str:
  return hashlib.sha256(path.read_bytes()).hexdigest()


def disable_core() -> None:
  resource.setrlimit(resource.RLIMIT_CORE, (0, 0))


def scenarios() -> list[tuple[str, str, int, dict[str, str]]]:
  modes = {
    "session": "camera:267",
    "destroy_session": "camera:268",
    "acquire_sensor": "camera:258",
    "acquire_phy": "camera:258",
    "configure": "camera:261",
    "start": "camera:259",
    "stop": "camera:260",
    "release_device": "camera:262",
    "flush_device": "camera:264",
    "link": "camera:269",
    "activate": "camera:278",
    "deactivate": "camera:278",
    "unlink": "camera:270",
    "schedule": "camera:271",
    "flush_requests": "camera:272",
    "probe": "camera:266",
    "release_buffer": "camera:276",
    "fences": "sync:0",
    "wait": "sync:6",
    "destroy_fence": "sync:1",
    "poll": "poll",
    "event": "event",
    "imports": "camera:275",
    "allocation": "camera:274",
    "pool": "camera:274",
  }
  cases = []
  for mode, opcode in modes.items():
    seeds = [2, 3, 4, 5, 6, 7] if mode == "imports" else [0, 1, 2, 3, 17, 0x80000000, 0xFFFFFFFF]
    cases.extend((f"{mode}-{seed}", mode, seed, {}) for seed in seeds)
    if mode in {"link", "pool", "allocation", "imports"}:
      continue
    for errno, count in [(4, 1), (4, 100), (4, 101), (11, 1), (5, 1)]:
      cases.append((f"{mode}-error-{errno}-{count}", mode, 7, {"CK_FAIL_OP": opcode, "CK_FAIL_ERRNO": str(errno), "CK_FAIL_COUNT": str(count)}))
    cases.append((f"{mode}-retry-mutation", mode, 7, {"CK_FAIL_OP": opcode, "CK_FAIL_ERRNO": "4", "CK_FAIL_COUNT": "2", "CK_MUTATE_FAILURE": "1"}))
  for mode in ["fences", "wait", "destroy_fence"]:
    for code in [-5, 3]:
      cases.append((f"{mode}-embedded-{code}", mode, 7, {"CK_SYNC_RESULT": str(code)}))
  for error, count in [(4, 2), (4, 101), (11, 1), (5, 1)]:
    cases.append((f"bps-create-error-{error}-{count}", "fences", 7, {"CK_FAIL_OP": "camera:0", "CK_FAIL_ERRNO": str(error), "CK_FAIL_COUNT": str(count)}))
    cases.append(
      (
        f"yuv-import-error-{error}-{count}",
        "imports",
        7,
        {"CK_FAIL_OP": "camera:275", "CK_FAIL_SKIP": "1", "CK_FAIL_ERRNO": str(error), "CK_FAIL_COUNT": str(count)},
      )
    )
  for mode in ["allocation", "pool"]:
    cases.append((f"{mode}-allocation-error-mutated", mode, 3, {"CK_FAIL_OP": "camera:274", "CK_FAIL_ERRNO": "5", "CK_MUTATE_FAILURE": "1"}))
  for result in [0, -1, 1]:
    for events in [0, 1, 2, 8, 16, 32]:
      cases.append((f"poll-return-{result}-{events}", "poll", 1, {"CK_POLL_RETURN": str(result), "CK_POLL_EVENTS": str(events)}))
  return cases


def normalize_trace(records: list[dict]) -> list[dict]:
  for record in records:
    if record["op"] == "open" and record["fd"] in (601, 602):
      record["flags"] &= ~os.O_CLOEXEC
  return records


def main() -> None:
  parser = argparse.ArgumentParser(description="Compare original/native camera kernel calls through real libc boundaries")
  parser.add_argument("--source", type=Path, required=True)
  parser.add_argument("--native", type=Path, required=True)
  parser.add_argument("--fixture", type=Path, required=True)
  parser.add_argument("--output", type=Path, required=True)
  parser.add_argument("--qemu", type=Path)
  parser.add_argument("--sysroot", type=Path)
  parser.add_argument("--baseline-only", action="store_true")
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  report = {"binaries": {str(p): digest(p) for p in [args.source, args.native, args.fixture]}, "cases": [], "failures": []}
  cases = scenarios()
  if args.baseline_only:
    baseline = {}
    for case in cases:
      if not case[3]:
        baseline.setdefault(case[1], case)
    cases = list(baseline.values())
  for name, mode, seed, overrides in cases:
    results = []
    for lane, binary in [("source", args.source), ("native", args.native)]:
      folder = args.output / name / lane
      folder.mkdir(parents=True, exist_ok=True)
      trace = folder / "trace.jsonl"
      trace.unlink(missing_ok=True)
      env = {**os.environ, **overrides, "CK_TRACE": str(trace)}
      command = [str(binary), mode, str(seed)]
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
      records = [json.loads(line) for line in trace.read_text().splitlines()] if trace.exists() else []
      results.append((metadata, normalize_trace(records)))
    source, native = results
    passed = (
      source[0]["exit"] == native[0]["exit"] == 0
      and not source[0]["sanitizer_failure"]
      and not native[0]["sanitizer_failure"]
      and json.loads(source[0]["stdout"] or "null") == json.loads(native[0]["stdout"] or "null")
      and source[1] == native[1]
    )
    entry = {"name": name, "mode": mode, "seed": seed, "environment": overrides, "passed": passed, "calls": len(source[1])}
    report["cases"].append(entry)
    if not passed:
      differences = [(i, a, b) for i, (a, b) in enumerate(zip(source[1], native[1], strict=False)) if a != b]
      report["failures"].append(
        {**entry, "source": source[0], "native": native[0], "lengths": [len(source[1]), len(native[1])], "first_difference": differences[:1]}
      )
    (args.output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(name, "PASS" if passed else "FAIL", flush=True)
  print(json.dumps({"cases": len(cases), "failures": len(report["failures"])}))
  raise SystemExit(bool(report["failures"]))


if __name__ == "__main__":
  main()
