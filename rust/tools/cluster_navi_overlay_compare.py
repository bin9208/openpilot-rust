from __future__ import annotations

import argparse
from dataclasses import asdict, replace
import hashlib
import itertools
import json
from pathlib import Path
import subprocess
import sys

from cluster_simulator_compare import compare


def main():
  cli = argparse.ArgumentParser()
  cli.add_argument("--root", type=Path, required=True)
  cli.add_argument("--native", type=Path)
  cli.add_argument("--output", type=Path, required=True)
  args = cli.parse_args()
  args.root, args.output = args.root.resolve(), args.output.resolve()
  args.output.mkdir(parents=True, exist_ok=True)
  sys.path.insert(0, str(args.root / "openpilot/selfdrive/carrot/cluster"))
  import cluster_models as models
  import cluster_navi_overlay as source
  from cluster_simulator import ClusterSimulator

  simulator = ClusterSimulator()
  for _ in range(40):
    base = simulator.update(models.SimulatorInput(throttle=0.5, steering=0.25, left_signal_requested=True), 0.05)
  meta = models.NaviItemMeta(7, 100, 10.0)
  navis = [None, models.NaviLiveState(1, "empty")]
  navis.extend(models.NaviLiveState(2, "speed", speed=models.NaviSpeedInfo(meta, 99.0, limit)) for limit in [None, -1, 0, 30])
  navis.append(models.NaviLiveState(3, "current", current=models.NaviGuidanceInfo(meta, 100, 5, 2)))
  navis.extend(models.NaviLiveState(4, "status", status=models.NaviStatusInfo(meta, "route", active, False, True)) for active in [False, True])
  request, expected = [], []
  for index, (limit, origin, navi, active, clock) in enumerate(
    itertools.product([None, 0, 80], [None, "v", "n", "x"], navis, [False, True], [None, "", " ", "12:00:00"])
  ):
    state = replace(
      base,
      speed_limit_kph=limit,
      speed_limit_source=origin,
      external_nav_active=active,
      center_clock_text=clock,
      navi_dashboard=models.NaviDashboardState(False, "base"),
    )
    overlay = replace(
      base,
      speed_kph=1.0,
      navi_live=navi,
      navi_dashboard=models.NaviDashboardState(True, "overlay") if index % 2 else None,
      center_clock_text="22:11:00" if index % 3 else None,
    )
    request.append({"base": asdict(state), "overlay": asdict(overlay)})
    expected.append(asdict(source.merge_navi_overlay_state(state, overlay)))
  request = json.loads(json.dumps(request))
  expected = json.loads(json.dumps(expected))
  (args.output / "request.json").write_text(json.dumps(request))
  (args.output / "source.json").write_text(json.dumps(expected))
  if args.native is None:
    print(json.dumps({"cases": len(expected)}))
    return
  run = subprocess.run([str(args.native.resolve())], input=json.dumps(request), text=True, capture_output=True, timeout=30)
  (args.output / "native.json").write_text(run.stdout)
  (args.output / "native.stderr").write_text(run.stderr)
  run.check_returncode()
  compare(expected, json.loads(run.stdout))
  result = {
    "cases": len(expected),
    "native_sha256": hashlib.sha256(args.native.read_bytes()).hexdigest(),
    "source_sha256": hashlib.sha256(Path(source.__file__).read_bytes()).hexdigest(),
  }
  (args.output / "result.json").write_text(json.dumps(result, indent=2))
  print(json.dumps(result))


if __name__ == "__main__":
  main()
