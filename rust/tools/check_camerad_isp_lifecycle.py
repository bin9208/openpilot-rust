import argparse
from collections import Counter
import json
import os
from pathlib import Path
import subprocess

from check_camerad_kernel import digest, disable_core


def normalize(source: list[dict], native: list[dict]) -> tuple[list[dict], dict]:
  allocations = {}
  for record in source:
    if record.get("name") == "camera:274" and record["ret"] == 0:
      before, after = bytes.fromhex(record["before"]), bytes.fromhex(record["after"])
      allocations[int.from_bytes(after[92:96], "little")] = int.from_bytes(before[84:88], "little")
  owned = {fd for fd, flags in allocations.items() if flags != 0x58}
  blobs = {fd for fd, flags in allocations.items() if flags == 0x811}
  closed = Counter(record["fd"] for record in native if record["op"] == "close" and record["fd"] in owned)
  assert closed == Counter(owned), (closed, owned)
  extra_unmaps = [record for record in native if record["op"] == "munmap" and record["fd"] in blobs]
  assert len(extra_unmaps) == len(blobs)
  for fd in blobs:
    observed = [record for record in native if record["op"] == "memory" and record["stage"] == "release" and record["fd"] == fd]
    expected = [dict(record, stage="release") for record in source if record["op"] == "memory" and record["stage"] == "snapshot" and record["fd"] == fd]
    assert observed == expected, f"BPS config allocation {fd} changed between acquire and release"
  output = [
    record
    for record in native
    if not (
      record["op"] == "close"
      and record["fd"] in owned
      or record["op"] == "munmap"
      and record["fd"] in blobs
      or record["op"] == "memory"
      and record["stage"] == "release"
      and record["fd"] in blobs
    )
  ]
  return output, {"native_closes_source_leaked_fds": sorted(owned), "native_unmaps_source_leaked_config": sorted(blobs)}


def run(args, lane: str, binary: Path, name: str, arguments: list[str], overrides: dict) -> tuple[dict, list[dict]]:
  folder = args.output / name / lane
  folder.mkdir(parents=True, exist_ok=True)
  trace = folder / "trace.jsonl"
  trace.unlink(missing_ok=True)
  env = {**os.environ, **overrides, "CK_TRACE": str(trace)}
  command = [str(binary), *arguments]
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
  process = subprocess.run(command, env=env, capture_output=True, text=True, timeout=45, preexec_fn=disable_core)
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
  return metadata, records


def main() -> None:
  parser = argparse.ArgumentParser(description="Compare ISP/BPS resource ownership, submitted packets and all mapped bytes")
  for name in ["source", "native", "fixture", "output"]:
    parser.add_argument(f"--{name}", type=Path, required=True)
  parser.add_argument("--qemu", type=Path)
  parser.add_argument("--sysroot", type=Path)
  parser.add_argument("--smoke", action="store_true")
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  cases = [
    (f"sensor-{kind}-mode-{mode}-depth-{depth}", [str(kind), str(mode), str(depth)], {}) for kind in range(1, 4) for mode in range(3) for depth in [1, 18]
  ]
  if args.smoke:
    cases = cases[:1] + [cases[4]]
  else:
    for mode in [1, 2]:
      for operation in [258, 261, 274]:
        for count in [1, 100]:
          cases.append(
            (
              f"retry-{mode}-{operation}-{count}",
              ["3", str(mode), "18"],
              {"CK_FAIL_OP": f"camera:{operation}", "CK_FAIL_COUNT": str(count), "CK_FAIL_ERRNO": "4"},
            )
          )
  report = {"binaries": {str(p): digest(p) for p in [args.source, args.native, args.fixture]}, "cases": [], "failures": []}
  for name, arguments, overrides in cases:
    source, native = [run(args, lane, binary, name, arguments, overrides) for lane, binary in [("source", args.source), ("native", args.native)]]
    cleaned, ownership, error = native[1], {}, None
    try:
      cleaned, ownership = normalize(source[1], native[1])
    except AssertionError as failure:
      error = str(failure)
    passed = (
      source[0]["exit"] == native[0]["exit"] == 0
      and not source[0]["sanitizer_failure"]
      and not native[0]["sanitizer_failure"]
      and json.loads(source[0]["stdout"] or "null") == json.loads(native[0]["stdout"] or "null")
      and error is None
      and source[1] == cleaned
    )
    report["cases"].append({"name": name, "passed": passed, "calls": len(source[1]), "ownership": ownership})
    if not passed:
      first = next(((i, a, b) for i, (a, b) in enumerate(zip(source[1], cleaned, strict=False)) if a != b), None)
      report["failures"].append(
        {"name": name, "source": source[0], "native": native[0], "lengths": [len(source[1]), len(cleaned)], "ownership_error": error, "first_difference": first}
      )
    (args.output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(name, "PASS" if passed else "FAIL", flush=True)
    if not passed:
      break
  print(json.dumps({"cases": len(report["cases"]), "failures": len(report["failures"])}))
  raise SystemExit(bool(report["failures"]))


if __name__ == "__main__":
  main()
