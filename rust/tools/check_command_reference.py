from __future__ import annotations

import argparse
import json
from pathlib import Path
import subprocess

from openpilot.selfdrive.carrot.bluetooth.model import CommandReader


def event(identifier, created, action="laneLeft", **fields):
    return {"id": identifier, "time": created, "action": action, "address": "fixture", **fields}


def fixtures():
    yield {"now": 10.01, "allowed": True, "writes": {}}
    yield {"now": 10.1, "allowed": True, "writes": {"lane": {"events": [event("old", 9.9), event("first", 10.1, repeat=True)]}}}
    yield {"now": 10.11, "allowed": True, "writes": {"lane": {"events": [event("second", 10.11)]}}}
    yield {"now": 10.15, "allowed": True, "writes": {}}
    yield {"now": 10.2, "allowed": True, "writes": {}}
    yield {"now": 10.25, "allowed": False, "writes": {"lane": {"events": [event("disabled", 10.25), event("disabled2", 10.25)]}}}
    yield {"now": 10.3, "allowed": True, "writes": {}}
    yield {"now": 10.45, "allowed": True, "writes": {"lane": {"events": [event("expired", 10.01), event("future", 10.5)]}}}
    yield {"now": 10.55, "allowed": True, "writes": {}}
    yield {"now": 10.6, "allowed": True, "writes": {"lane": {"events": [event("cancelled", 10.5), event("fresh", 10.6)]}, "cancelled": {"fixture": 10.5}}}
    yield {"now": 10.65, "allowed": True, "writes": {"lane": {"events": [event("learning", 10.6)]}, "learn": {"address": "fixture", "until": 11.0}}}
    yield {"now": 11.05, "allowed": True, "writes": {}}
    yield {"now": 11.1, "allowed": True, "writes": {"lane": event("legacy", 11.1, "laneRight", repeat=[1])}}
    for index, payload in enumerate([None, [], 1, {"events": 1}, {"events": [None, {"id": 2}, {"id": "bad-time", "time": "bad"}]}]):
        yield {"now": 11.2 + index * .1, "allowed": True, "writes": {"lane": payload}}
    for index in range(500):
        now = 12. + index * .025
        messages = [event(f"stream-{index}-{count}", now, "laneLeft" if count % 2 else "laneRight", repeat=count % 2) for count in range(3)]
        if index % 5 == 0:
            messages.insert(0, event(f"invalid-{index}", now, "unknown"))
        if index % 7 == 0:
            messages.insert(0, event(f"expired-{index}", now - .5))
        yield {"now": now, "allowed": index % 4 != 0, "writes": {"lane": {"events": messages}}}
        yield {"now": now + .02, "allowed": True, "writes": {}}
    now = 30.
    yield {"now": now, "allowed": False, "writes": {"lane": {"events": [event(f"bounded-{i}", now) for i in range(200)]}}}
    yield {"now": now + .05, "allowed": True, "writes": {"lane": event("bounded-0", now)}}
    yield {"now": now + .1, "allowed": True, "writes": {"lane": event("bounded-199", now)}}
    for batch in range(3):
        yield {"now": 31. + batch * .05, "allowed": False,
               "writes": {"lane": {"events": [event(f"evict-{batch}-{i}", 31. + batch * .05) for i in range(64)]}}}
    yield {"now": 31.2, "allowed": True, "writes": {"lane": event("evict-0-0", 31.)}}
    yield {"now": 31.25, "allowed": True, "writes": {"lane": event("evict-2-63", 31.1)}}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    root = args.output / "original"
    root.mkdir()
    original = CommandReader("lane", root)
    original.started = 10.
    request = args.output / "request.jsonl"
    reference = []
    with request.open("w") as target:
        for step in fixtures():
            target.write(json.dumps(step) + "\n")
            for name, value in step["writes"].items():
                (root / f"{name}.json").write_text(json.dumps(value))
            action = original.read(step["allowed"], step["now"])
            reference.append({"action": action, "last_id": original.last_id, "repeat": original.is_repeat})
    (args.output / "reference.json").write_text(json.dumps(reference, indent=2) + "\n")
    output = args.output / "actual.jsonl"
    subprocess.run([args.binary.resolve(), request.resolve(), output.resolve(), (args.output / "rust").resolve()], check=True)
    actual = [json.loads(line) for line in output.read_text().splitlines()]
    for index, (expected, observed) in enumerate(zip(reference, actual, strict=True)):
        assert expected == observed, (index, expected, observed)
    report = {"reads": len(reference), "commands_returned": sum(value["action"] is not None for value in reference),
              "comparison": "exact action, consumed ID and repeat state", "device_validation": False}
    (args.output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
