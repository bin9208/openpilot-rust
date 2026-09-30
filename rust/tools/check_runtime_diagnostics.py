import argparse
import importlib.util
import json
import math
from pathlib import Path
import random
import struct
import subprocess
import types

SOURCE = Path(__file__).resolve().parents[2] / "openpilot/common/runtime_diagnostics.py"


def sample(name, value):
  return {"name": name, "value": str(value) if type(value) is int else struct.pack(">d", value).hex(), "integer": type(value) is int}


def compare(expected, actual, path=""):
  assert type(expected) is type(actual), (path, type(expected), type(actual), expected, actual)
  if isinstance(expected, dict):
    assert list(expected) == list(actual), (path, expected, actual)
    for key in expected:
      compare(expected[key], actual[key], path + "." + key)
  elif isinstance(expected, list):
    assert len(expected) == len(actual)
    for i, (left, right) in enumerate(zip(expected, actual, strict=True)):
      compare(left, right, path + f"[{i}]")
  elif isinstance(expected, float):
    assert (math.isnan(expected) and math.isnan(actual)) or struct.pack(">d", expected) == struct.pack(">d", actual), (path, expected, actual)
  else:
    assert expected == actual, (path, expected, actual)


def source_trace(setup, steps):
  specification = importlib.util.spec_from_file_location("original_runtime_diagnostics", SOURCE)
  module = importlib.util.module_from_spec(specification)
  specification.loader.exec_module(module)
  state = {"now": setup["started"], "scheduler": setup["scheduler"]}
  output = []
  module.time = types.SimpleNamespace(monotonic=lambda: state["now"])
  module.os = types.SimpleNamespace(getpid=lambda: setup["pid"])
  module.RuntimeDiagnostics._schedstat = lambda self: tuple(state["scheduler"] or ())
  emitted = []
  module.RuntimeDiagnostics._schedstat.__name__ = "_schedstat"

  def emit(event, **values):
    assert event == "runtimeTiming"
    emitted.append(values)

  instance = module.RuntimeDiagnostics(setup["component"], emit, setup["interval"])
  instance.schedstats_enabled = setup["enabled"]
  for step in steps:
    state.update(now=step["now"], scheduler=step["scheduler"])
    values = {item["name"]: int(item["value"]) if item["integer"] else struct.unpack(">d", bytes.fromhex(item["value"]))[0]
              for item in step["values"]}
    emitted.clear()
    instance.record(context=dict(step["context"]), **values)
    output.append(emitted[0] if emitted else None)
  return output


def fixtures():
  generator = random.Random(45)
  setup = {"component": "modeld", "interval": 1.0, "started": 0.0, "scheduler": [100, 200, 3], "enabled": True, "pid": 123}
  steps = []
  for i in range(6000):
    values = [sample("inference_ms", generator.uniform(-20, 100)),
              sample("published", i % 2), sample("dropped_frames", i % 4)]
    if i % 7 == 0:
      values.append(sample("missing", math.nan if i % 2 else math.inf))
    if i % 101 == 0:
      values.append(sample("inference_ms", math.nan))
    context = [("backend", "RustNative"), ("frame_id", i)]
    if i % 97 == 0:
      context.append(("frames", 99))
    steps.append({"now": (i + 1) / 20, "scheduler": None if i % 83 == 0 else [100 + i * 4000, 200 + i * 8000, 3 + i],
                  "values": values, "context": context})
  yield "aggregation", setup, steps
  setup = dict(setup, component="rounding", interval=0.0, enabled=None, scheduler=None)
  floats = [0.0, -0.0, 1.2345, -1.2345, 2.675, -2.675, math.inf, -math.inf, math.nan, 1e308, -1e308]
  floats += [struct.unpack(">d", generator.getrandbits(64).to_bytes(8, "big"))[0] for _ in range(12000)]
  steps = [{"now": i + 1.0, "scheduler": None, "values": [sample("value", value)], "context": []}
           for i, value in enumerate(floats)]
  yield "rounding", setup, steps
  setup = dict(setup, interval=1.0)
  values = [2**53, 2**53 + 1, float(2**53), -(2**53 + 1), -float(2**53), 2**63 - 1, -(2**63), -0.5, 0, 0.5]
  steps = [{"now": (i + 1) / 4, "scheduler": None, "values": [sample("integer", value)], "context": []}
           for i, value in enumerate(values * 30)]
  steps += [{"now": 100.0, "scheduler": None, "values": [sample("overflow", 1e308)], "context": []},
            {"now": 100.5, "scheduler": None, "values": [sample("overflow", 1e308)], "context": []},
            {"now": 101.0, "scheduler": None, "values": [sample("overflow", 1e308)], "context": []}]
  yield "integer-and-overflow", setup, steps


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument("--binary", type=Path, required=True)
  parser.add_argument("--output", type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  reports = {}
  for name, setup, steps in fixtures():
    expected = source_trace(setup, steps)
    payload = "\n".join(json.dumps(value) for value in [setup, *steps]) + "\n"
    result = subprocess.run([str(args.binary.resolve())], input=payload, text=True, capture_output=True, check=True)
    actual = [json.loads(line) for line in result.stdout.splitlines()]
    (args.output / f"{name}-input.jsonl").write_text(payload)
    (args.output / f"{name}-actual.json").write_text(json.dumps(actual))
    (args.output / f"{name}-expected.json").write_text(json.dumps(expected))
    compare(expected, actual, name)
    reports[name] = {"steps": len(steps), "emissions": sum(value is not None for value in actual)}
  report = {"result": "pass", "comparison": "exact types, field order and float bits", "scenarios": reports}
  (args.output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
  print(json.dumps(report))


if __name__ == "__main__":
  main()
