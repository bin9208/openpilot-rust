from __future__ import annotations

import argparse
import ast
from collections import Counter
import json
from pathlib import Path
import subprocess
from types import SimpleNamespace
from typing import Optional, Union

import capnp
import numpy as np

from openpilot.cereal import log
from openpilot.cereal.services import SERVICE_LIST
from openpilot.common.utils import MovingAverage


def source():
    path = Path(__file__).resolve().parents[2] / "openpilot/cereal/messaging/__init__.py"
    tree = ast.parse(path.read_text())
    nodes = [node for node in tree.body if isinstance(node, (ast.FunctionDef, ast.ClassDef))
             and node.name in ("new_message", "FrequencyTracker", "SubMaster")]
    environment = {"simulation": "0"}
    scope = {"Optional": Optional, "List": list, "Union": Union, "Dict": dict, "capnp": capnp,
             "log": log, "SERVICE_LIST": SERVICE_LIST, "MovingAverage": MovingAverage, "time": SimpleNamespace(monotonic=lambda: 0.),
             "Poller": object, "sub_sock": lambda *args, **kwargs: None,
             "os": SimpleNamespace(getenv=lambda key, default: environment.get(key.lower(), default))}
    exec(compile(ast.Module(body=nodes, type_ignores=[]), str(path), "exec"), scope)
    return scope, environment


def fixtures():
    names = ["carState", "modelV2", "liveTracks", "carrotMan", "carParams", "liveCalibration"]
    options = [{"frequency": 20.}, {"poll": {"one": "modelV2"}}, {"poll": {"many": ["modelV2", "liveTracks"]}},
               {"poll": {"many": ["modelV2"]}}, {"frequency": 100.}, {"frequency": 4.},
               {"frequency": 20., "ignore_alive": ["carState"], "ignore_frequency": ["modelV2"], "ignore_valid": ["liveTracks"]},
               {"frequency": 20., "simulation": True}]
    random = np.random.default_rng(20260930)
    for config in options:
        time = 100.
        for frame in range(1400):
            time += .05 if frame < 300 or frame >= 1100 else float(random.choice([.025, .05, .05, .05, .1, .25, .6]))
            if frame == 900:
                time += 1000.
            services = []
            if frame % 40 < 32:
                services += ["carState", "modelV2"]
            if frame % 3 != 0:
                services.append("liveTracks")
            if frame % 5 == 0:
                services.append("liveCalibration")
            if frame % 17 == 0:
                services.append("carrotMan")
            if frame % 201 == 0:
                services.append("carParams")
            yield {"configuration": {"services": names, "options": config} if frame == 0 else None,
                   "time": time, "services": services, "valid": frame % 113 != 0, "value": float(random.uniform(-1, 40)),
                   "checks": [[], ["carState", "modelV2"], ["carrotMan"], ["carParams"], ["liveTracks", "liveCalibration"]]}
    for index, time in enumerate([1., 1., 1.1, 1.15, 1.2]):
        yield {"configuration": {"services": ["carState"], "options": {}} if index == 0 else None,
               "time": time, "services": ["carState"], "valid": True, "value": 2., "checks": [[]]}


def compare(expected, actual, path=""):
    if isinstance(expected, dict):
        assert expected.keys() == actual.keys(), (path, expected.keys(), actual.keys())
        return max((compare(value, actual[key], f"{path}.{key}") for key, value in expected.items()), default=0.)
    if isinstance(expected, list):
        assert len(expected) == len(actual), path
        return max((compare(a, b, f"{path}[{i}]") for i, (a, b) in enumerate(zip(expected, actual, strict=True))), default=0.)
    if isinstance(expected, float):
        error = abs(expected - actual)
        assert error <= 1e-12, (path, expected, actual)
        return error
    assert expected == actual, (path, expected, actual)
    return 0.


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    scope, environment = source()
    request = args.output / "request.jsonl"
    expected = args.output / "expected.jsonl"
    original = None
    outcomes = Counter()
    with request.open("w") as requests, expected.open("w") as references:
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
                if service == "carState":
                    message.carState.vEgo = frame["value"]
                messages.append(message)
            requests.write(json.dumps({"configuration": configuration, "time": frame["time"],
                                       "messages": [list(message.to_bytes()) for message in messages], "checks": frame["checks"]}) + "\n")
            error = None
            try:
                original.update_msgs(frame["time"], [message.as_reader() for message in messages])
            except ZeroDivisionError:
                error = "zero_interval"
            topics = []
            for name in original.services:
                tracker = original.freq_tracker[name]
                topics.append({"name": name, "seen": original.seen[name], "updated": original.updated[name],
                    "receive_time": original.recv_time[name], "receive_frame": original.recv_frame[name], "log_mono_time": original.logMonoTime[name],
                    "alive": original.alive[name], "frequency_ok": original.freq_ok[name], "valid": original.valid[name],
                    "polled": name not in original.non_polled_services, "velocity": original[name].vEgo if name == "carState" else None,
                    "tracker": {"min": tracker.min_freq, "max": tracker.max_freq, "previous": tracker.prev_time,
                                "count": tracker.avg_dt.count, "index": tracker.avg_dt.index, "sum": tracker.avg_dt.sum,
                                "recent_count": tracker.recent_avg_dt.count, "recent_index": tracker.recent_avg_dt.index,
                                "recent_sum": tracker.recent_avg_dt.sum}})
            checks = [[original.all_alive(names), original.all_freq_ok(names), original.all_valid(names), original.all_checks(names)]
                      for names in frame["checks"]]
            outcomes[f"all_checks:{checks[0][3]}"] += 1
            outcomes[f"frequency_ok:{original.freq_ok['carState']}"] += 1
            outcomes[f"error:{error}"] += 1
            references.write(json.dumps({"frame": original.frame, "frequency": original.update_freq,
                                          "topics": topics, "checks": checks, "error": error}) + "\n")
    actual = args.output / "actual.jsonl"
    subprocess.run([args.binary.resolve(), request.resolve(), actual.resolve()], check=True)
    maximum = 0.
    count = 0
    with expected.open() as expected_file, actual.open() as actual_file:
        for index, (a, b) in enumerate(zip(expected_file, actual_file, strict=True)):
            maximum = max(maximum, compare(json.loads(a), json.loads(b), str(index)))
            count += 1
    assert outcomes["all_checks:True"] and outcomes["all_checks:False"]
    assert outcomes["frequency_ok:True"] and outcomes["frequency_ok:False"]
    assert outcomes["error:zero_interval"] == 1
    report = {"updates": count, "maximum_absolute_error": maximum, "numeric_tolerance": 1e-12,
              "outcomes": outcomes, "device_validation": False}
    (args.output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
