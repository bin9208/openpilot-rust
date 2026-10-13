from __future__ import annotations

import argparse
import ast
import hashlib
import itertools
import json
from pathlib import Path
import random
import subprocess
import sys
from types import SimpleNamespace

from cluster_simulator_compare import compare


def main():
  cli = argparse.ArgumentParser()
  cli.add_argument("--root", type=Path, required=True)
  cli.add_argument("--native", type=Path)
  cli.add_argument("--output", type=Path, required=True)
  args = cli.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  source = args.root / "openpilot/selfdrive/carrot/cluster/main.py"
  tree = ast.parse(source.read_text())
  demo = next(node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == "run_demo")
  nodes = list(ast.walk(demo))

  def conditional(text):
    return next(node for node in nodes if isinstance(node, ast.If) and ast.unparse(node.test) == text)

  def execute(body, scope):
    exec(compile(ast.Module(body=body, type_ignores=[]), str(source), "exec"), scope)

  blocks = {
    "toggle": conditional("action.toggle_pause").body,
    "seek": conditional("action.seek_s is not None").body,
    "finish": conditional("route_end_waiting or route_source.is_finished(playback_seconds, route_loop)").body[:5],
    "loop": conditional("route_loop and route_end_waiting").body,
    "pause": conditional("next_corner_lateral_offset_m != route_active_corner_lateral_offset_m").body[-3:],
  }
  retry = conditional("now >= route_next_retry_time")
  calc = next(node for node in nodes if isinstance(node, ast.Assign)
              and any(isinstance(t, ast.Name) and t.id == "playback_seconds" for t in node.targets) and isinstance(node.value, ast.IfExp))
  cutin = conditional("route_cutin_alert_tracker.update(route_state_recorded_cutin_sound_candidates(state))")
  sound_def = next(node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == "route_state_recorded_cutin_sound_candidates")
  sys.path.insert(0, str(args.root))
  from openpilot.selfdrive.controls.lib.cutin_alert import CutinAlertCandidate, CutinAlertTracker

  rng = random.Random(253)
  cases, expected = [], []
  for duration, speed, start in itertools.product([0.0, 0.1, 20.0], [0.25, 1.0, 3.0], [-1.0, 0.0, 12.0, 50.0]):
    case = {"start": start, "duration": duration, "now": 100.0, "speed": speed, "steps": []}
    scope = {"route_playback_base_s": min(max(0.0, start), duration), "route_wall_base_time": 100.0, "route_replay_speed": speed,
             "route_paused": False, "route_end_waiting": False, "route_next_retry_time": 0.0, "route_source": SimpleNamespace(duration=duration),
             "route_cutin_alert_tracker": CutinAlertTracker(), "CutinAlertCandidate": CutinAlertCandidate}
    execute([sound_def], scope)
    rows = []
    for index in range(80):
      op = list(blocks)[index % len(blocks)] if index < 5 else rng.choice(["time", "toggle", "seek", "pause", "finish", "retry", "loop", "cutin"])
      step = {"op": op, "now": 100.0 + index * 0.37, "time": rng.choice([-1.0, 0.0, 0.01, 12.5, 30.0]),
              "sound": bool(rng.getrandbits(1)), "enabled": bool(rng.getrandbits(1))}
      case["steps"].append(step)
      scope.update(now=step["now"], playback_seconds=step["time"], action=SimpleNamespace(seek_s=step["time"]))
      flag = None
      if op in blocks:
        if op != "loop" or scope["route_end_waiting"]:
          execute(blocks[op], scope)
      elif op == "retry":
        flag = eval(compile(ast.Expression(retry.test), str(source), "eval"), scope)
        if flag:
          execute(retry.body[:1], scope)
      elif op == "cutin":
        scope["state"] = SimpleNamespace(recorded_cutin_sound=step["sound"])
        if step["enabled"]:
          flag = eval(compile(ast.Expression(cutin.test), str(source), "eval"), scope)
          if flag:
            execute(cutin.body[:3], scope)
        else:
          scope["route_cutin_alert_tracker"].reset()
          flag = False
      execute([calc], scope)
      rows.append({"clock": {"base": scope["route_playback_base_s"], "wall": scope["route_wall_base_time"], "speed": speed,
                            "paused": scope["route_paused"], "waiting": scope["route_end_waiting"], "next_retry": scope["route_next_retry_time"],
                            "sound_active": bool(scope["route_cutin_alert_tracker"].previous)}, "time": scope["playback_seconds"], "flag": flag})
    cases.append(case)
    expected.append(rows)
  (args.output / "request.json").write_text(json.dumps(cases))
  (args.output / "source.json").write_text(json.dumps(expected))
  if args.native is None:
    print(json.dumps({"cases": len(cases), "states": sum(len(case["steps"]) for case in cases)}))
    return
  run = subprocess.run([str(args.native)], input=json.dumps(cases), capture_output=True, text=True, timeout=30)
  (args.output / "native.json").write_text(run.stdout)
  (args.output / "native.stderr").write_text(run.stderr)
  run.check_returncode()
  compare(expected, json.loads(run.stdout))
  result = {"cases": len(cases), "states": sum(len(case["steps"]) for case in cases), "source_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
            "native_sha256": hashlib.sha256(args.native.read_bytes()).hexdigest(),
            "scope": "actual main.py AST playback control branches and CutinAlertTracker; process ownership validated separately"}
  (args.output / "result.json").write_text(json.dumps(result, indent=2) + "\n")
  print(json.dumps(result))


if __name__ == "__main__":
  main()
