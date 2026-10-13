import argparse
import ast
import hashlib
import json
from pathlib import Path
import re
import subprocess
import tempfile


FUNCTIONS = {
  "discover_route_logs",
  "adjacent_route_log_path",
  "route_search_spec",
  "route_sort_key",
  "segment_index",
  "segment_route_id",
  "segment_route_id_from_name",
  "segment_index_from_name",
}


def original(root):
  source = root / "openpilot/selfdrive/carrot/cluster/cluster_route_replay.py"
  namespace = {"Path": Path, "LOG_FILENAMES": {"rlog": "rlog.zst", "qlog": "qlog.zst"}, "NUMBERED_FOLDER_RE": re.compile(r"^(.*?)(\d+)$")}
  tree = ast.parse(source.read_text())
  selected = [node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name in FUNCTIONS]
  assert len(selected) == len(FUNCTIONS)
  exec(compile(ast.Module(selected, type_ignores=[]), str(source), "exec"), namespace)
  return namespace, hashlib.sha256(source.read_bytes()).hexdigest()


def result(function, *args):
  try:
    value = function(*args)
    if isinstance(value, Path):
      value = str(value)
    elif isinstance(value, list):
      value = [str(path) for path in value]
    return {"Ok": value}
  except (ValueError, RuntimeError, OSError) as error:
    return {"Err": str(error)}


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument("--root", type=Path, required=True)
  parser.add_argument("--native", type=Path, required=True)
  parser.add_argument("--output", type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  namespace, source_hash = original(args.root)
  names = [
    f"{route}--{index}" for route in ("aaa", "bbb--nested", "", "001") for index in ("0", "00", "01", "9", "100", "-1", "+1", "", " 1", "１", "٩", "1_0")
  ]
  names.extend(["none", "--", "name000", "a---1", "a--10--2", "a--2.txt"])
  queries = []
  with tempfile.TemporaryDirectory(prefix="route-paths-", dir=args.output) as temporary:
    root = Path(temporary).resolve()
    for parent in ("aaa--000", "aaa--001", "aaa--002", "aaa--010", "bbb--001", "bbb--2", "free", "nested/ccc--0"):
      folder = root / parent
      folder.mkdir(parents=True)
      for filename in ("rlog.zst", "qlog.zst"):
        (folder / filename).touch()
    (root / "aaa--003").mkdir()
    (root / "aaa--003/rlog.bz2").touch()
    (root / "linked").symlink_to(root / "aaa--001", target_is_directory=True)
    paths = [
      root,
      root / "aaa",
      root / "aaa--001",
      root / "aaa--005",
      root / "aaa--001/rlog.zst",
      root / "nested",
      root / "missing",
      root / "missing/deeper",
      root / "linked",
    ]
    for path in paths:
      for start, maximum in ((None, None), (0, None), (2, None), (None, 1), (None, 0), (None, -1)):
        queries.append({"path": str(path), "kind": "rlog", "start": start, "maximum": maximum, "direction": None, "names": names})
    for path in (root / "aaa--000", root / "aaa--001/rlog.zst", root / "aaa--002", root / "aaa--010", root / "free", root / "linked"):
      for kind in ("rlog", "qlog", "unsupported"):
        for direction in (-1, 1, 0):
          queries.append({"path": str(path), "kind": kind, "direction": direction, "names": []})
    expected = []
    for query in queries:
      path = Path(query["path"])
      expected.append(
        {
          "discovered": result(namespace["discover_route_logs"], path, query["kind"], query.get("start"), query.get("maximum")),
          "adjacent": result(namespace["adjacent_route_log_path"], path, query["direction"], query["kind"]) if query["direction"] is not None else None,
          "indexes": [namespace["segment_index_from_name"](name) for name in query["names"]],
          "routes": [namespace["segment_route_id_from_name"](name) for name in query["names"]],
        }
      )
    run = subprocess.run([str(args.native.resolve())], input="".join(json.dumps(query) + "\n" for query in queries), capture_output=True, text=True, check=True)
    actual = [json.loads(line) for line in run.stdout.splitlines()]
    assert len(actual) == len(expected)
    failures = [
      {"case": index, "query": queries[index], "source": source, "native": native}
      for index, (source, native) in enumerate(zip(expected, actual, strict=True))
      if source != native
    ]
    report = {
      "cases": len(expected),
      "name_cases": len(names) * 54,
      "failures": failures,
      "source_sha256": source_hash,
      "native_sha256": hashlib.sha256(args.native.read_bytes()).hexdigest(),
    }
    for filename, data in (("queries.json", queries), ("source.json", expected), ("native.json", actual), ("result.json", report)):
      (args.output / filename).write_text(json.dumps(data, indent=2) + "\n")
    print(json.dumps(report))
    if failures:
      raise SystemExit(1)


if __name__ == "__main__":
  main()
