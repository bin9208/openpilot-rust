import argparse
import json
from pathlib import Path
import subprocess
import types

import capnp

from check_message_state import fixtures, source
from check_runtime_diagnostics import compare
from openpilot.common import runtime_diagnostics


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument("--binary", type=Path, required=True)
  parser.add_argument("--output", type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  scope, environment = source()
  requests, expected = [], []
  original = None
  for index, frame in enumerate(fixtures()):
    configuration = frame["configuration"]
    if configuration:
      options = configuration["options"]
      environment["simulation"] = str(int(options.get("simulation", False)))
      poll = options.get("poll")
      original = scope["SubMaster"](configuration["services"], poll=next(iter(poll.values())) if poll else None,
        frequency=options.get("frequency"), ignore_alive=options.get("ignore_alive"),
        ignore_avg_freq=options.get("ignore_frequency"), ignore_valid=options.get("ignore_valid"))
    messages = []
    for service in frame["services"]:
      try:
        message = scope["new_message"](service)
      except capnp.lib.capnp.KjException:
        message = scope["new_message"](service, 0)
      message.valid = frame["valid"]
      message.logMonoTime = index * 50_000_000
      messages.append(message)
    try:
      original.update_msgs(frame["time"], [message.as_reader() for message in messages])
    except ZeroDivisionError:
      pass
    services = ["unknown", *reversed(original.services), original.services[0]]
    now = frame["time"] + (index % 17 - 3) / 1000
    runtime_diagnostics.time = types.SimpleNamespace(monotonic=lambda now=now: now)
    expected.append(runtime_diagnostics.communication_snapshot(original, services))
    requests.append({"configuration": configuration, "time": frame["time"], "snapshot_time": now,
      "messages": [list(message.to_bytes()) for message in messages], "services": services})
  payload = "\n".join(json.dumps(frame) for frame in requests) + "\n"
  result = subprocess.run([args.binary.resolve()], input=payload, text=True, capture_output=True, check=True)
  actual = [json.loads(line) for line in result.stdout.splitlines()]
  (args.output / "input.jsonl").write_text(payload)
  (args.output / "expected.json").write_text(json.dumps(expected))
  (args.output / "actual.json").write_text(json.dumps(actual))
  compare(expected, actual)
  report = {"result": "pass", "updates": len(actual), "comparison": "exact types, field order and float bits"}
  (args.output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
  print(json.dumps(report))


if __name__ == "__main__":
  main()
