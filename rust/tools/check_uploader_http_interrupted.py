#!/usr/bin/env python3
import argparse
import json
import os
from pathlib import Path
import subprocess

from check_uploader_daemon import scenario


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  output = args.output.resolve()
  output.mkdir(parents=True, exist_ok=False)
  library = output / 'interrupted-read.so'
  fixture = Path(__file__).parent / 'fixtures/http/interrupted_read.c'
  subprocess.run(['cc', '-std=c11', '-shared', '-fPIC', '-Wall', '-Wextra', '-Werror', str(fixture), '-o', str(library), '-ldl', '-pthread'], check=True)
  trace = output / 'read.trace'
  os.environ.update(LD_PRELOAD=str(library), HTTP_EINTR_TRACE=str(trace))
  result = scenario(args.binary.resolve(), output, 'interrupted-put', {'metered': True})
  assert trace.read_text().splitlines() == ['PUT recv EINTR'] * 2
  assert [request['method'] for request in result['requests']] == ['GET', 'PUT', 'GET', 'PUT']
  result.update(interrupted_put_reads=2, repeated_http_requests=0)
  (output / 'report.json').write_text(json.dumps(result, indent=2) + '\n')
  print(json.dumps(result))


if __name__ == '__main__':
  main()
