import argparse
import ast
from contextlib import redirect_stdout
from dataclasses import dataclass
import hashlib
import io
import json
from pathlib import Path
import random
import subprocess
from types import SimpleNamespace
from collections.abc import Iterable


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('root', type=Path)
  parser.add_argument('output', type=Path)
  parser.add_argument('--binary', type=Path)
  args = parser.parse_args()
  output = args.output.resolve()
  output.mkdir(parents=True, exist_ok=False)
  path = args.root.resolve() / 'openpilot/selfdrive/carrot/cluster/cluster_profile.py'
  names = {'ProfileSampleStats', 'ProfileReporter'}
  nodes = [node for node in ast.parse(path.read_text()).body if isinstance(node, ast.ClassDef) and node.name in names]
  scope = {
    'dataclass': dataclass,
    'Iterable': Iterable,
    'time': SimpleNamespace(perf_counter=lambda: 0.0, process_time=lambda: 0.0),
    '_read_system_cpu_stat': lambda: None,
  }
  exec(compile(ast.Module(nodes, type_ignores=[]), str(path), 'exec'), scope)
  generator = random.Random(253)
  cases = []
  for enabled in [False, True]:
    for interval in [0.01, 0.2, 2.0]:
      for _trial in range(20):
        reporter = scope['ProfileReporter'](enabled, interval)
        reporter._runtime_summary = lambda _elapsed: 'owned native summary'
        operations, reports = [], []
        for step in range(60):
          if step % 7 == 6:
            now = step * 0.1
            operations.append({'report': now})
            with redirect_stdout(io.StringIO()) as captured:
              reporter.maybe_report(now)
            reports.append(captured.getvalue())
          else:
            name = f'stage{generator.randrange(6)}'
            value = generator.choice([-3.0, 0.0, 1.0, 1.5, 2.0, 10.0, 27.5])
            reporter.add(name, value)
            reporter.frame_done()
            operations.append({'name': name, 'sample': value, 'frame': True})
        cases.append({'enabled': enabled, 'interval': interval, 'operations': operations, 'reports': reports})
  request = output / 'input.json'
  request.write_text(json.dumps(cases))
  result = {
    'cases': len(cases),
    'ordered_steps': sum(len(case['operations']) for case in cases),
    'source_sha256': hashlib.sha256(path.read_bytes()).hexdigest(),
  }
  if args.binary:
    binary = args.binary.resolve()
    command = [str(binary), str(request)]
    process = subprocess.run(command, text=True, capture_output=True, timeout=30)
    (output / 'native.stdout').write_text(process.stdout)
    (output / 'native.stderr').write_text(process.stderr)
    assert process.returncode == 0, process.stderr
    native = json.loads(process.stdout)
    assert len(native) == len(cases)
    differences = [
      {'case': index, 'expected': case['reports'], 'actual': actual}
      for index, (case, actual) in enumerate(zip(cases, native, strict=True))
      if case['reports'] != actual
    ]
    result.update(
      binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),
      invocation=command,
      differences=differences,
      matched=len(cases) - len(differences),
    )
  (output / 'result.json').write_text(json.dumps(result, indent=2))
  print(json.dumps(result))
  if result.get('differences'):
    raise SystemExit(1)


if __name__ == '__main__':
  main()
