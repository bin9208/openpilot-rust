# /// script
# requires-python = ">=3.12,<3.13"
# dependencies = ["pycapnp==2.2.4", "pyzmq==27.2.0"]
# ///
# Run via check_stats_producer.py.
"""Original StatLog with an isolated socket path; fork/drop and backpressure peer."""
import ast
import os
import sys
from pathlib import Path
import zmq


def main():
  source = Path(__file__).resolve().parents[2] / 'openpilot/system/statsd.py'
  tree = ast.parse(source.read_text())
  tree.body = [node for node in tree.body if isinstance(node, ast.ClassDef)]
  scope = dict(os=os, zmq=zmq, STATS_SOCKET=sys.argv[1])
  exec(compile(tree, str(source), 'exec'), scope)
  producer = scope['StatLog']()
  if sys.argv[2] == 'pressure':
    for index in range(20000):
      producer.gauge('pressure', float(index))
    print('ready', flush=True)
    assert sys.stdin.readline()
    del producer
    return
  if sys.argv[2] == 'types':
    from openpilot.cereal import log
    state = log.Event.new_message().init('deviceState')
    state.gpuUsagePercent = 6
    state.memoryUsagePercent = 42
    state.fanSpeedPercentDesired = 65535
    state.screenBrightnessPercent = 0
    state.freeSpacePercent = 62.1
    state.cpuUsagePercent = [-128, 127]
    state.cpuTempC = [40.2]
    metrics = [('gpu_usage_percent', state.gpuUsagePercent), ('memory_usage_percent', state.memoryUsagePercent),
               ('fan_speed_percent_desired', state.fanSpeedPercentDesired), ('screen_brightness_percent', state.screenBrightnessPercent),
               ('free_space_percent', state.freeSpacePercent)]
    metrics += [(f'cpu{index}_usage_percent', value) for index, value in enumerate(state.cpuUsagePercent)]
    metrics += [(f'cpu{index}_temperature', value) for index, value in enumerate(state.cpuTempC)]
    for name, value in metrics:
      producer.gauge(name, value)
      assert sys.stdin.readline()
    producer.sample('integer_sample', 6)
    assert sys.stdin.readline()
    producer.sample('float_sample', -0.0)
    assert sys.stdin.readline()
    del producer
    return
  producer._send('parent-before')
  assert sys.stdin.readline()
  child = os.fork()
  if child == 0:
    if sys.argv[2] != 'drop':
      producer._send('child-record')
      assert sys.stdin.readline()
    del producer
    os._exit(0)
  assert os.waitpid(child, 0)[1] == 0
  producer._send('parent-after')
  assert sys.stdin.readline()
  del producer


if __name__ == '__main__':
  main()
