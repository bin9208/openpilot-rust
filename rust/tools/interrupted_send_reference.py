"""Run unchanged Python producers in fresh, preload-instrumented child processes."""

import ast
import json
import os
from pathlib import Path
import sys

import zmq
from logging_producer_reference import console_handler, original_socket_handler, source

ROOT = Path(__file__).resolve().parents[2]


def main():
  kind, endpoint = sys.argv[1:]
  if kind == 'logging':
    logger, _ = source()
    console, _ = console_handler(None)
    console.setStream(sys.stderr)
    logger.addHandler(console)
    logger.addHandler(original_socket_handler(endpoint, logger))
    send = logger.error
  else:
    path = ROOT / 'openpilot/system/statsd.py'
    tree = ast.parse(path.read_text())
    tree.body = [node for node in tree.body if isinstance(node, ast.ClassDef) and node.name in ('StatLog', 'METRIC_TYPE')]
    scope = {'os': os, 'zmq': zmq, 'STATS_SOCKET': endpoint}
    exec(compile(tree, str(path), 'exec'), scope)
    producer = scope['StatLog']()
    send = producer._send
  for line in sys.stdin:
    try:
      send(json.loads(line))
    except zmq.ZMQError as error:
      response = {'error': str(error), 'errno': error.errno}
    else:
      response = {'ok': True}
    print(json.dumps(response), flush=True)


if __name__ == '__main__':
  main()
