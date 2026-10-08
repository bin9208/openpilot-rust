#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = ["pyzmq"]
# ///
# ─── How to run ───
# PYTHONPATH=.:rust/tools uv run rust/tools/check_logging_connect.py --binary BINARY --output OUTPUT
# Requires strace with stack tracing; records the actual zmq_connect poll before injection.
# ──────────────────
"""Compare interrupted logger connection with the unchanged Python handler."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import select
import subprocess
import sys
import tempfile
from typing import Literal, TypedDict

import zmq

ROOT = Path(__file__).resolve().parents[2]
TEXT = 'interrupted-connect-record'


class Observation(TypedDict):
  command: list[str]
  response: dict[str, str | int]
  packet: str | None
  packet_count: int
  exit_code: int
  trace: str


def original(endpoint: str) -> None:
  from logging_producer_reference import original_socket_handler, source
  logger, _ = source()
  handler = original_socket_handler(endpoint, logger)
  logger.addHandler(handler)
  try:
    try:
      logger.info(TEXT)
      response = {'delivery': 'sent'}
    except zmq.ZMQError as error:
      response = {'error': error.strerror}
    print(json.dumps(response), flush=True)
    sys.stdin.read()
  finally:
    handler.close()


def connect_poll(trace: Path) -> int:
  ordinal = 0
  active = False
  for line in trace.read_text().splitlines():
    if line.startswith('poll('):
      ordinal += 1
      active = True
    elif not line.startswith(' >'):
      active = False
    if active and 'zmq_connect+' in line:
      return ordinal
  raise AssertionError(f'no traced zmq_connect poll: {trace}')


def execute(kind: Literal['source', 'native'], binary: Path, output: Path,
            *, interrupted: int | None = None, invalid: bool = False) -> Observation:
  output.mkdir(parents=True)
  trace = output / 'syscalls.log'
  with tempfile.TemporaryDirectory(prefix='246-log-connect-') as directory, zmq.Context() as context:
    endpoint = 'ipc://' + directory + '/collector'
    with context.socket(zmq.PULL) as collector:
      collector.bind(endpoint)
      address = 'invalid-protocol://owned-246' if invalid else endpoint
      executable = [str(binary), address] if kind == 'native' else [sys.executable, '-P', str(Path(__file__).resolve()), '--source', address]
      command = ['strace', '--kill-on-exit', '-k', '-e', 'trace=poll,socketpair']
      if interrupted is not None:
        command += ['-e', f'inject=poll:error=EINTR:when={interrupted}']
      command += ['-o', str(trace), *executable]
      env = {**os.environ, 'PYTHONPATH': f'{ROOT}:{ROOT / "rust/tools"}'}
      with (output / 'stderr.log').open('w') as stderr:
        process = subprocess.Popen(command, env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=stderr, text=True)
        assert process.stdin is not None and process.stdout is not None
        try:
          if kind == 'native':
            process.stdin.write(json.dumps({'action': 'emit', 'level': 20, 'text': TEXT, 'exception': None}) + '\n')
            process.stdin.flush()
          assert select.select([process.stdout], [], [], 5)[0], command
          response = json.loads(process.stdout.readline())
          packet = collector.recv() if collector.poll(5000 if response.get('delivery') == 'sent' else 250) else None
          process.stdin.close()
          code = process.wait(timeout=5)
          extra = collector.recv() if collector.poll(250) else None
        finally:
          try:
            process.stdin.close()
          except BrokenPipeError as error:
            print(f'owned probe stdin cleanup: {error}', file=sys.stderr)
          if process.poll() is None:
            process.kill()
            process.wait(timeout=5)
          process.stdout.close()
      if packet is not None:
        (output / 'record.packet').write_bytes(packet)
      if extra is not None:
        (output / 'extra.packet').write_bytes(extra)
      row = Observation(command=command, response=response, packet=packet.hex() if packet else None,
                        packet_count=int(packet is not None) + int(extra is not None), exit_code=code, trace=str(trace))
      (output / 'result.json').write_text(json.dumps(row, indent=2) + '\n')
      return row


def main() -> None:
  if sys.argv[1:2] == ['--source']:
    original(sys.argv[2])
    return
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', required=True, type=Path)
  parser.add_argument('--output', required=True, type=Path)
  args = parser.parse_args()
  binary = args.binary.resolve()
  output = args.output.resolve()
  output.mkdir(parents=True, exist_ok=False)
  rows = []
  for kind in ('source', 'native'):
    control = execute(kind, binary, output / kind / 'control')
    ordinal = connect_poll(Path(control['trace']))
    interrupted = execute(kind, binary, output / kind / 'interrupted', interrupted=ordinal)
    invalid = execute(kind, binary, output / kind / 'invalid', invalid=True)
    trace = Path(interrupted['trace']).read_text()
    injected = trace[trace.index('INJECTED'):].split('\npoll(', 1)[0]
    delivered = all(row['response'].get('delivery') == 'sent' and row['packet_count'] == 1 and row['packet'] is not None
                    and json.loads(bytes.fromhex(row['packet'])[1:])['msg'] == TEXT for row in (control, interrupted))
    same_context = Path(control['trace']).read_text().count('socketpair(') == trace.count('socketpair(')
    invalid_preserved = 'error' in invalid['response'] and invalid['packet'] is None
    rows.append({'kind': kind, 'connect_poll_ordinal': ordinal, 'control': control, 'interrupted': interrupted,
                 'invalid': invalid, 'delivered_once': delivered, 'same_context': same_context,
                 'causal_stack': 'zmq_connect+' in injected, 'non_eintr_error_preserved': invalid_preserved,
                 'passed': delivered and same_context and invalid_preserved and 'zmq_connect+' in injected
                           and all(row['exit_code'] == 0 for row in (control, interrupted, invalid))})
  result = {'binary': str(binary), 'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(), 'rows': rows,
            'source_sha256': hashlib.sha256((ROOT / 'openpilot/common/swaglog.py').read_bytes()).hexdigest()}
  (output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
  for row in rows:
    print(row['kind'], 'PASS' if row['passed'] else 'FAIL', flush=True)
  raise SystemExit(0 if all(row['passed'] for row in rows) else 1)


if __name__ == '__main__':
  main()
