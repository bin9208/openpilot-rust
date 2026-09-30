# /// script
# requires-python = ">=3.12"
# dependencies = ["pycapnp==2.1.0", "pyzmq==27.2.0"]
# ///
# Used by check_driving_daemon.py/check_driver_daemon.py with --collector.
"""Capture real daemon -> ZMQ collector -> original msgq/cereal publications."""
from collections import Counter
import json
import os
from pathlib import Path
import signal
import subprocess
import time

from openpilot.cereal import log, messaging


class Collector:
  def __init__(self, binary: Path, output: Path, environment: dict[str, str]):
    self.output = output
    self.output.mkdir()
    self.root = output / 'disk'
    self.prefix = environment['OPENPILOT_PREFIX']
    self.started = time.monotonic_ns()
    self.commit = subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip()
    self.records = {'logMessage': [], 'errorLogMessage': []}
    self.sockets = {}
    self.process = None
    with (output / 'stdout.log').open('wb') as stdout, (output / 'stderr.log').open('wb') as stderr:
      self.process = subprocess.Popen([str(binary), '--log-root', str(self.root)], env=environment, stdout=stdout, stderr=stderr)
    deadline = time.monotonic() + 10
    queues = Path('/dev/shm') / f'msgq_{self.prefix}'
    while not all((queues / name).is_file() for name in self.records):
      assert self.process.poll() is None and time.monotonic() < deadline, (output / 'stderr.log').read_text()
      time.sleep(.01)
    previous = os.environ.get('OPENPILOT_PREFIX')
    os.environ['OPENPILOT_PREFIX'] = self.prefix
    try:
      self.sockets = {name: messaging.sub_sock(name, conflate=False) for name in self.records}
    finally:
      if previous is None:
        del os.environ['OPENPILOT_PREFIX']
      else:
        os.environ['OPENPILOT_PREFIX'] = previous

  def drain(self) -> None:
    for topic, socket in self.sockets.items():
      while (packet := socket.receive(non_blocking=True)) is not None:
        with log.Event.from_bytes(packet) as event:
          assert event.valid and event.which() == topic
          assert self.started <= event.logMonoTime <= time.monotonic_ns()
          raw = getattr(event, topic)
        record = json.loads(raw)
        assert record['ctx']['runtime_language'] == 'rust'
        assert record['ctx']['source_commit'] == self.commit
        source = Path(__file__).resolve().parents[1] / record['pathname']
        assert source.name == record['filename'] and 'log_site!' in source.read_text().splitlines()[record['lineno'] - 1]
        assert record['thread'] == record['process']
        self.records[topic].append(record)
        with (self.output / f'{topic}.bin').open('ab') as stream:
          stream.write(packet)

  def finish(self, component: str, pids: list[int]) -> dict:
    deadline = time.monotonic() + .3
    while time.monotonic() < deadline:
      self.drain()
      time.sleep(.01)
    self.process.send_signal(signal.SIGTERM)
    assert self.process.wait(timeout=5) == 0
    self.drain()
    records = self.records['logMessage']
    assert records and all(record['process'] in pids for record in records)
    errors = [record for record in records if record['levelnum'] >= 40]
    assert Counter(map(json.dumps, errors)) == Counter(map(json.dumps, self.records['errorLogMessage']))
    timing = [record for record in records if isinstance(record['msg'], dict) and record['msg'].get('event') == 'runtimeTiming']
    for record in timing:
      message = record['msg']
      assert set(message) == {'event', 'component', 'pid', 'mono_time', 'seconds', 'frames', 'metrics',
                              'scheduler', 'schedstats_enabled', 'backend', 'usbgpu', 'frame_id'}
      assert record['levelnum'] == 20 and message['component'] == 'modeld'
      assert message['pid'] == record['process'] and message['usbgpu'] is False
      assert message['backend'] == "openpilot_driving_modeld::runtime::DrivingRuntime<'_>"
      metrics = message['metrics']
      assert list(metrics) == ['camera_wait_ms', 'camera_age_at_run_ms', 'inference_ms', 'inference_thread_cpu_ms',
                               'postprocess_ms', 'loop_ms', 'thread_cpu_ms', 'dropped_frames', 'published']
      assert all(value['count'] == message['frames'] for value in metrics.values())
      assert all(list(value) == ['mean', 'max', 'count'] for value in metrics.values())
      assert type(metrics['published']['max']) is int and metrics['published']['max'] in (0, 1)
      assert type(metrics['dropped_frames']['max']) is int and metrics['dropped_frames']['max'] >= 0
      assert 0 <= metrics['inference_thread_cpu_ms']['max'] <= metrics['inference_ms']['max'] + 1
      assert 0 <= metrics['thread_cpu_ms']['max'] <= metrics['loop_ms']['max'] + 1
      assert 'communication' not in message
    match component:
      case 'modeld':
        assert timing, 'real model loop must emit at least one runtimeTiming aggregate'
        assert any(record['levelnum'] == 10 and record['msg'] == 'camera pair unavailable or out of sync' for record in records)
        assert any(record['levelnum'] == 40 and record['msg'].startswith('camera dropped ') for record in records)
        assert any(record['levelnum'] == 20 and record['msg'].startswith('modeld got CarParams:') for record in records if isinstance(record['msg'], str))
      case 'dmonitoringmodeld':
        assert not timing and not errors, 'the driver source has no runtimeTiming or error callsites on this path'
        for text in ['connecting to driver stream', 'models loaded, dmonitoringmodeld starting', 'got SIGINT']:
          assert any(record['msg'] == text and record['levelnum'] == 30 for record in records), text
      case _:
        raise AssertionError(component)
    disk = [json.loads(line) for path in self.root.glob('swaglog.*') for line in path.read_text().splitlines()]
    assert len(disk) == sum(record['levelnum'] >= 20 for record in records)
    report = {'result': 'pass', 'component': component, 'log_messages': len(records), 'error_messages': len(errors),
              'runtime_timing_events': len(timing), 'disk_records': len(disk), 'pids': pids,
              'source_commit': self.commit, 'collector_exit': self.process.returncode}
    for topic, values in self.records.items():
      (self.output / f'{topic}.json').write_text(json.dumps(values, indent=2) + '\n')
    (self.output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    return report

  def close(self) -> None:
    if self.process is not None and self.process.poll() is None:
      self.process.kill()
      self.process.wait(timeout=5)
    self.sockets.clear()
    Path('/tmp/logmessage' + self.prefix).unlink(missing_ok=True)
