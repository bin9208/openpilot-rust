from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time

from cluster_route_frames_compare import native_frame
from cluster_simulator_compare import compare


def main():
  cli = argparse.ArgumentParser()
  cli.add_argument("--root", type=Path, required=True)
  cli.add_argument("--log", type=Path, required=True)
  cli.add_argument("--native", type=Path)
  cli.add_argument("--launcher", type=Path)
  cli.add_argument("--output", type=Path, required=True)
  args = cli.parse_args()
  args.root, args.output, args.log = args.root.resolve(), args.output.resolve(), args.log.resolve()
  args.output.mkdir(parents=True, exist_ok=True)
  sys.path.insert(0, str(args.root))
  sys.path.insert(0, str(args.root / "openpilot/selfdrive/carrot/cluster"))
  import cluster_route_replay as source

  empty, invalid, missing, fifo = [args.output / name for name in ["empty.raw", "invalid.raw", "missing.raw", "blocked.fifo"]]
  empty.write_bytes(b"")
  invalid.write_bytes(b"bad")
  if not fifo.exists():
    os.mkfifo(fifo)
  requests = [{"generation": 4, "index": index, "path": str(path)} for index, path in enumerate([empty, args.log, invalid, missing, args.log])]
  results = []
  worker = source.RouteLogPreloadWorker("live")
  try:
    for request in requests:
      worker.request(request["generation"], request["index"], Path(request["path"]))
      response = worker.receive(True)
      results.append(
        {
          "generation": response.generation,
          "index": response.index,
          "path": response.path,
          "frames": None if response.frames is None else [native_frame(frame, source.frame_to_state) for frame in response.frames],
          "error": response.error,
        }
      )
    worker.restart()
    worker.request(5, 6, args.log)
    response = worker.receive(True)
    results.append(
      {
        "generation": response.generation,
        "index": response.index,
        "path": response.path,
        "frames": [native_frame(frame, source.frame_to_state) for frame in response.frames],
        "error": response.error,
      }
    )
    idle_pid = worker._worker.pid
    start = time.monotonic()
    worker.close()
    idle_seconds = time.monotonic() - start
    worker.restart()
    blocked_pid = worker._worker.pid
    worker.request(6, 7, fifo)
    time.sleep(0.15)
    start = time.monotonic()
    worker.close()
    blocked_seconds = time.monotonic() - start
    worker.restart()
    worker._worker.terminate()
    worker._worker.join(timeout=1)
    empty_poll = worker.receive(False) is None
    try:
      worker.receive(True)
      exit_detected = False
    except RuntimeError as error:
      exit_detected = str(error) == "route preload worker exited"
  finally:
    worker.close()
  expected = {
    "responses": results,
    "idle_seconds": idle_seconds,
    "blocked_seconds": blocked_seconds,
    "idle_reaped": not Path(f"/proc/{idle_pid}").exists(),
    "blocked_reaped": not Path(f"/proc/{blocked_pid}").exists(),
    "empty_poll": empty_poll,
    "exit_detected": exit_detected,
  }
  (args.output / "source.json").write_text(json.dumps(expected))
  request = {"requests": requests, "restart_path": str(args.log), "blocked_path": str(fifo), "launcher": str(args.launcher)}
  (args.output / "request.json").write_text(json.dumps(request))
  assert idle_seconds < 1.5 and blocked_seconds < 1.5 and expected["idle_reaped"] and expected["blocked_reaped"]
  assert empty_poll and exit_detected
  if args.native is None:
    print(json.dumps({"source_responses": len(results), "idle_seconds": idle_seconds, "blocked_seconds": blocked_seconds}))
    return
  run = subprocess.run([str(args.native.resolve())], input=json.dumps(request), text=True, capture_output=True, timeout=30)
  (args.output / "native.json").write_text(run.stdout)
  (args.output / "native.stderr").write_text(run.stderr)
  run.check_returncode()
  actual = json.loads(run.stdout)
  for value in [expected, actual]:
    for response in value["responses"]:
      response["error"] = response["error"] is not None
  compare(json.loads(json.dumps(expected["responses"])), actual["responses"])
  assert actual["idle_seconds"] < 1.5 and actual["blocked_seconds"] < 1.5
  assert actual["idle_reaped"] and actual["blocked_reaped"]
  assert actual["empty_poll"] and actual["exit_detected"]
  report = {
    "responses": len(results),
    "frames": sum(len(result["frames"] or []) for result in results),
    "native_idle_seconds": actual["idle_seconds"],
    "native_blocked_seconds": actual["blocked_seconds"],
    "native_sha256": hashlib.sha256(args.native.read_bytes()).hexdigest(),
    "source_sha256": hashlib.sha256(Path(source.__file__).read_bytes()).hexdigest(),
  }
  (args.output / "result.json").write_text(json.dumps(report, indent=2) + "\n")
  print(json.dumps(report))


if __name__ == "__main__":
  main()
