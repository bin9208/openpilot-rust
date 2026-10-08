import argparse
import ast
import datetime
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
from types import SimpleNamespace as NS

sys.path.insert(0, str(Path(__file__).parent))
from carrot_man_compare import ROOT, compare
from carrot_man_serv_compare import setup_values


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument("--binary", required=True)
  parser.add_argument("--evidence", required=True, type=Path)
  args = parser.parse_args()
  tree = ast.parse((ROOT / "openpilot/selfdrive/carrot/carrot_serv.py").read_text())
  owner = next(n for n in tree.body if isinstance(n, ast.ClassDef) and n.name == "CarrotServ")
  function = next(n for n in owner.body if isinstance(n, ast.FunctionDef) and n.name == "set_time")
  scope = dict(subprocess=subprocess, print=lambda *a: None)
  exec(compile(ast.fix_missing_locations(ast.Module(body=[function], type_ignores=[])), "carrot_serv.py", "exec"), scope)
  expected = []
  with tempfile.TemporaryDirectory(prefix="carrot-time-219-") as temporary:
    root = Path(temporary)
    requests = [dict(op="configure", root=str(root), values=setup_values())]
    real_datetime = datetime.datetime
    real_run = subprocess.run
    for index, (file, drift, outcomes) in enumerate([
      (b"zone", 9999, []), (b"zone", 10000, []), (b"", 0, []), (b"zone", 20000, [False]),
      (None, 0, [False, True]), (None, 0, [True, False]),
    ]):
      path = root / f"localtime-{index}"
      if file is not None:
        path.write_bytes(file)
      epoch = 1700000000
      now = real_datetime.utcfromtimestamp(epoch) + datetime.timedelta(milliseconds=drift)
      class Clock(real_datetime):
        @classmethod
        def utcnow(cls):
          return now
      commands = []
      answers = iter(outcomes)
      values = {}
      def run(command, **kwargs):
        if isinstance(command, str):
          name, arguments = "sh", ["-c", command]
        else:
          name, *arguments = command
        commands.append(dict(name=name, args=[argument.replace("/data/etc/localtime", str(path)) for argument in arguments]))
        if not next(answers, True):
          raise subprocess.CalledProcessError(1, command)
        return NS(returncode=0)
      source_os = NS(path=NS(getsize=lambda p: path.stat().st_size,
        exists=lambda p: path.exists(), islink=lambda p: path.is_symlink()))
      scope["os"] = source_os
      datetime.datetime = Clock
      subprocess.run = run
      try:
        scope["set_time"](NS(params=NS(put=lambda key, value: values.update({key: value}))), epoch, "Asia/Seoul")
      finally:
        datetime.datetime = real_datetime
        subprocess.run = real_run
      requests.append(dict(op="time_set", epoch=epoch, timezone="Asia/Seoul", path=str(path), now_millis=epoch * 1000 + drift, outcomes=outcomes))
      expected.append(dict(commands=commands, timezone=values.get("TimezoneName"), source=values.get("TimezoneSource")))
    result = real_run([args.binary], input="".join(json.dumps(r) + "\n" for r in requests), text=True, capture_output=True)
    if result.returncode:
      raise RuntimeError(result.stderr)
    actual = [json.loads(line) for line in result.stdout.splitlines()][1:]
    args.evidence.mkdir(parents=True, exist_ok=True)
    for name, rows in [("inputs", requests), ("source", expected), ("rust", actual)]:
      (args.evidence / f"time-{name}.jsonl").write_text("".join(json.dumps(row) + "\n" for row in rows))
    compare(actual, expected, "time command condition/failure and actual Params")
    (args.evidence / "time-summary.json").write_text(json.dumps(dict(passed=True, scenarios=len(expected),
      surface="original time-setting function and Rust command boundary; no host time/zone mutation"), indent=2) + "\n")
    print(f"PASS {len(expected)} original time-setting boundary scenarios")


if __name__ == "__main__":
  main()
