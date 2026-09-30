# /// script
# requires-python = ">=3.12,<3.13"
# dependencies = ["numpy==2.5.3", "pycapnp==2.2.4", "pyzmq==27.2.0", "zstandard==0.25.0"]
# ///
# Run through check_stats_daemon.py, with original msgq and Params bindings.
"""Execute unchanged original stats main; only clock/path/hardware dependencies are adapted."""
import ast
import json
import os
import sys
import time
import uuid
from collections import defaultdict
from datetime import datetime, UTC
from pathlib import Path
from types import SimpleNamespace
import zmq
from original_params_binding import load
from openpilot.cereal.messaging import SubMaster


class EndOfOracle(Exception):
  pass


def clock():
  print('clock', flush=True)
  value = sys.stdin.readline()
  if not value:
    raise EndOfOracle
  return float(value)


class FaultLog:
  """A single actual logging-boundary error; successful calls use original cloudlog."""
  def __init__(self, logger, trace):
    self.logger, self.trace, self.attempts = logger, Path(trace), []

  def event(self, name, **fields):
    self.attempts.append(name)
    self.trace.write_text(json.dumps(self.attempts))
    if len(self.attempts) == 1:
      raise zmq.ZMQError(zmq.EINVAL)
    return self.logger.event(name, **fields)

  def error(self, message):
    return self.logger.error(message)


def main():
  endpoint, directory, metadata, binding = sys.argv[1:]
  module, swaglog = load(binding, 'ipc:///tmp/logmessage' + os.environ['OPENPILOT_PREFIX'], Path(directory).parent / 'source-logs')
  from openpilot.common.utils import atomic_write
  from openpilot.system.version import get_build_metadata
  source = Path(__file__).resolve().parents[2] / 'openpilot/system/statsd.py'
  tree = ast.parse(source.read_text())
  tree.body = [node for node in tree.body if not isinstance(node, (ast.Import, ast.ImportFrom, ast.If))]
  scope = dict(os=os, zmq=zmq, time=SimpleNamespace(monotonic=clock), uuid=uuid, Path=Path,
      defaultdict=defaultdict, datetime=SimpleNamespace(now=lambda zone: datetime.fromtimestamp(1723456789, UTC).replace(microsecond=123456)),
      UTC=UTC, NoReturn=None, Params=module.Params, SubMaster=SubMaster,
      Paths=SimpleNamespace(stats_root=lambda: directory), cloudlog=swaglog.cloudlog,
      HARDWARE=SimpleNamespace(get_device_type=lambda: 'pc'), atomic_write=atomic_write,
      get_build_metadata=lambda: get_build_metadata(metadata), STATS_DIR_FILE_LIMIT=10000,
      STATS_SOCKET=endpoint, STATS_FLUSH_TIME_S=60)
  if 'STATS_ORACLE_LOG_FAILURE_TRACE' in os.environ:
    scope['cloudlog'] = FaultLog(swaglog.cloudlog, os.environ['STATS_ORACLE_LOG_FAILURE_TRACE'])
  exec(compile(tree, str(source), 'exec'), scope)
  try:
    scope['main']()
  except EndOfOracle:
    return


if __name__ == '__main__':
  main()
