"""Execute source and native EGL policies against the same private C ABI fixture."""

import argparse
import json
import os
from pathlib import Path
import subprocess
import sys

parser = argparse.ArgumentParser()
parser.add_argument('--binary', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
root = Path(__file__).resolve().parents[2]
# Caller checks disk before invoking this small native fixture build.
command = ['g++', '-shared', '-fPIC', '-std=c++17', str(root / 'rust/tools/ui_qa/egl_fixture.cc'), '-ldl', '-o', str(args.output / 'libEGL.so')]
with (args.output / 'build.log').open('w') as log:
  subprocess.run(command, stdout=log, stderr=subprocess.STDOUT, check=True)
(args.output / 'libGLESv2.so').symlink_to('libEGL.so') if not (args.output / 'libGLESv2.so').exists() else None
results = []
for case in ['success', 'retry', 'fallback', 'fail', 'retry-fail', 'no-display', 'init-fail']:
  states = []
  traces = []
  for lane in ['source', 'native']:
    trace = args.output / f'{case}-{lane}.jsonl'
    trace.write_text('')
    env = dict(os.environ, EGL_CASE=case, EGL_TRACE=str(trace.resolve()), LD_LIBRARY_PATH=str(args.output.resolve()), PYTHONPATH=str(root))
    command = [sys.executable, str(root / 'rust/tools/ui_qa/egl_source.py')] if lane == 'source' else [str(args.binary)]
    result = subprocess.run(command, env=env, capture_output=True, text=True, check=True)
    (args.output / f'{case}-{lane}.stdout').write_text(result.stdout)
    (args.output / f'{case}-{lane}.stderr').write_text(result.stderr or '(no stderr)\n')
    states.append(json.loads(result.stdout))
    traces.append([json.loads(line) for line in trace.read_text().splitlines()])
  result = {'case': case, 'state_equal': states[0] == states[1], 'trace_equal': traces[0] == traces[1], 'fd_delta': states[1]['fd_delta']}
  results.append(result)
  print(json.dumps(result), flush=True)
(args.output / 'results.json').write_text(json.dumps(results, indent=2))
assert all(row['state_equal'] and row['trace_equal'] and row['fd_delta'] == 0 for row in results), results
print('PASS: 7 actual-source/native EGL lifecycle cases, NV12 attributes, retry, extension fallback, errors and duplicated-FD cleanup')
