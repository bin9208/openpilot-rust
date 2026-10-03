import argparse
import hashlib
import json
from pathlib import Path
import subprocess

from openpilot.selfdrive.carrot.bluetooth import model


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    source = args.output / "source"
    source.mkdir()
    reader = model.CommandReader("lane", source)
    reader.started = 10.
    expected, requests = [], []
    for index, (name, raw) in enumerate([
        ("learn", '{"address":"fixture","until":11}'),
        ("learn", '{"address":"fixture","until":Infinity}'),
        ("learn", '{"address":"fixture","until":NaN}'),
        ("cancelled", '{"fixture":11}'),
        ("cancelled", '{"fixture":Infinity}'),
        ("cancelled", '{"fixture":NaN}'),
        ("learn", '{"address":"fixture","until":11,"unused":"\\ud800"}'),
        ("cancelled", '{"fixture":11,"unused":NaN}'),
        ("learn", '{"address":"fixture","until":1e999}'),
        ("learn", '{"address":"fixture","until":-Infinity}'),
        ("cancelled", '{"fixture":1e999}'),
        ("cancelled", '{"fixture":-Infinity}'),
        ("lane", '{"id":"\\ud800","time":NOW,"action":"laneLeft","repeat":NaN}'),
        ("lane", '{"id":"\\ud800","time":NOW,"action":"laneRight"}'),
        ("lane", '{"id":"\\ud801","time":NOW,"action":"laneRight"}'),
        ("lane", '{"id":"nonfinite-time","time":Infinity,"action":"laneLeft"}'),
        ("lane", '{"id":"nan-time","time":NaN,"action":"laneLeft"}'),
        ("lane", '{"id":"overflow-time","time":1e999,"action":"laneLeft"}'),
        ("lane", '{"id":"repeat-inf","time":NOW,"action":"laneLeft","repeat":Infinity}'),
        ("lane", '{"id":"repeat-zero","time":NOW,"action":"laneLeft","repeat":-0.0}'),
        ("lane", '{"id":"ignored","time":NOW,"action":"laneLeft","unused":"\\udfff"}'),
        ("lane", '{"id":"invalid",'),
        ("lane", '{"events":[null,{"id":1},{"id":"valid","time":NOW,"action":"laneRight"}]}'),
    ]):
        now = 10.1 + index * .1
        raw = raw.replace("NOW", repr(now))
        writes = {"lane": {"id": f"case-{index}", "time": now,
                            "action": "laneLeft", "address": "fixture"},
                  "learn": {}, "cancelled": {}}
        for key, value in writes.items():
            (source / f"{key}.json").write_text(json.dumps(value), encoding="utf-8")
        (source / f"{name}.json").write_text(raw, encoding="utf-8")
        requests.append({"now": now, "allowed": True, "writes": writes,
                         "raw_writes": {name: raw}})
        action = reader.read(True, now)
        expected.append({"action": action, "last_id": reader.last_id, "repeat": reader.is_repeat})
    request = args.output / "request.jsonl"
    request.write_text("".join(json.dumps(value) + "\n" for value in requests))
    output = args.output / "actual.jsonl"
    subprocess.run([args.binary.resolve(), request.resolve(), output.resolve(),
                    (args.output / "native").resolve()], check=True)
    actual = [json.loads(line) for line in output.read_text().splitlines()]
    differences = [{"index": i, "expected": e, "actual": a}
                   for i, (e, a) in enumerate(zip(expected, actual, strict=True)) if e != a]
    result = {"cases": len(expected), "differences": differences, "source": model.__file__,
              "source_sha256": hashlib.sha256(Path(model.__file__).read_bytes()).hexdigest(),
              "binary_sha256": hashlib.sha256(args.binary.read_bytes()).hexdigest()}
    (args.output / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result))
    assert not differences, differences


if __name__ == "__main__":
    main()
