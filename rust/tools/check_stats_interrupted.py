# /// script
# requires-python = ">=3.12,<3.13"
# dependencies = ["numpy==2.5.3", "pycapnp==2.1.0", "pyzmq==27.2.0", "zstandard==0.25.0"]
# ///
# Run with the original IPC imports: python rust/tools/check_stats_interrupted.py BINARY BINDING PRELOAD OUTPUT [--before]
import argparse
import json
import os
from pathlib import Path
import select
import subprocess
import sys
import time

import zmq

from logmessaged_native import Peer as Collector

ROOT = Path(__file__).resolve().parents[2]
CASES = {'one': '4,0', 'repeated': '4,4,4,0', 'empty': '4,11,0', 'invalid': '4,22', 'closed': '4,88'}


def response(process: subprocess.Popen[str]) -> str:
  assert select.select([process.stdout], [], [], 5)[0], 'clock acknowledgement timeout'
  return process.stdout.readline().strip()


def scenario(args: argparse.Namespace, case: str, original: bool) -> dict:
  output = args.output / case / ('source' if original else 'native')
  output.mkdir(parents=True)
  collector = Collector(args.binary, output / 'collector', original=True)
  collector.start()
  metadata = output / 'metadata'
  metadata.mkdir()
  (metadata / 'build.json').write_text(json.dumps({'channel': 'fixture', 'openpilot': {'version': 'v'}}))
  directory = output / 'stats'
  endpoint = 'ipc:///tmp/stats-receive-' + collector.prefix
  trace = output / 'receive.tsv'
  environment = dict(os.environ, OPENPILOT_PREFIX=collector.prefix, PARAMS_ROOT=str(output / 'params'),
                     HOME=str(output / 'home'), ZMQ_TEST_RECV_ERRORS=CASES[case], ZMQ_TEST_RECV_TRACE=str(trace))
  command = [str(args.binary), endpoint, str(directory), str(metadata)]
  if original:
    command = [sys.executable, str(ROOT / 'rust/tools/stats_source.py'), endpoint, str(directory), str(metadata), str(args.binding)]
    library = next((Path(zmq.__file__).parent.parent / 'pyzmq.libs').glob('libzmq*.so*'))
    environment.update(LD_PRELOAD=str(args.preload), ZMQ_TEST_REAL_LIBRARY=str(library))
  fatal = case in ('invalid', 'closed') or (args.before and not original)
  with (output / 'stderr.log').open('w') as stderr, zmq.Context() as context, context.socket(zmq.PUSH) as producer:
    producer.setsockopt(zmq.SNDTIMEO, 3000)
    producer.setsockopt(zmq.LINGER, 0)
    process = subprocess.Popen(command, env=environment, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=stderr, text=True)
    try:
      assert response(process) == 'clock'
      producer.connect(endpoint)
      producer.send_string('retained:7|g')
      process.stdin.write('100\n')
      process.stdin.flush()
      if fatal:
        assert response(process) == ''
        assert process.wait(timeout=5) != 0
      else:
        assert response(process) == 'clock'
        deadline = time.monotonic() + 5
        while not any(int(line.split('\t')[3]) > 0 for line in trace.read_text().splitlines()):
          assert time.monotonic() < deadline, 'metric delivery timeout'
          process.stdin.write('100\n')
          process.stdin.flush()
          assert response(process) == 'clock'
        process.stdin.write('161\n')
        process.stdin.flush()
        assert response(process) == 'clock'
        process.stdin.write('162\n')
        process.stdin.flush()
        assert response(process) == 'clock'
        process.stdin.close()
        assert process.wait(timeout=5) == 0
      rows = [list(map(int, line.split('\t'))) for line in trace.read_text().splitlines()]
      expected = [4] if args.before and not original else [int(value) for value in CASES[case].split(',') if int(value)]
      assert [row[1] for row in rows if row[1]] == expected, rows
      assert all(row[4] == zmq.DONTWAIT for row in rows), rows
      files = [path.read_text() for path in directory.iterdir()]
      if fatal:
        assert not files
      else:
        assert len(files) == 1 and files[0].startswith('gauge.retained,') and ' value=7.0,' in files[0]
      result = {'exit': process.returncode, 'files': files, 'faults': expected}
      (output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
      return result
    finally:
      if process.poll() is None:
        process.kill()
        process.wait(timeout=5)
      process.stdout.close()
      if not process.stdin.closed:
        process.stdin.close()
      collector.stop()
      collector.close()
      Path(endpoint[6:]).unlink(missing_ok=True)


def main() -> None:
  parser = argparse.ArgumentParser()
  for name in ('binary', 'binding', 'preload', 'output'):
    parser.add_argument(name, type=lambda value: Path(value).resolve())
  parser.add_argument('--before', action='store_true')
  args = parser.parse_args()
  results = []
  for case in CASES:
    source = scenario(args, case, True)
    native = scenario(args, case, False)
    if args.before:
      assert native['faults'] == [4] and native['exit'] != 0
    else:
      assert source == native, (case, source, native)
    results.append({'case': case, 'source': source, 'native': native})
    print(case, 'reproduced mismatch' if args.before else 'PASS', flush=True)
  (args.output / 'result.json').write_text(json.dumps({'before': args.before, 'results': results}, indent=2) + '\n')


if __name__ == '__main__':
  main()
