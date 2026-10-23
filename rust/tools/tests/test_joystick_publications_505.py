"""#505: deterministic stdlib-only finite-stream and original-AST regressions."""
from __future__ import annotations

import ast
from collections import deque
import json
from pathlib import Path
import signal
import sys
from tempfile import TemporaryDirectory
from types import SimpleNamespace
import unittest

from joystick_publications import Publications

CHANNELS = ('carControl', 'controlsState')


class Clock:
  def __init__(self):
    self.now = 0.0

  def monotonic(self):
    return self.now

  def sleep(self, seconds):
    self.now += seconds


class Message:
  def __init__(self, channel, frame, *, valid=True, timestamp=None):
    self.data = {channel: {'frame': frame}, 'valid': valid,
                 'logMonoTime': 1_000_000_000 + frame * 10_000_000 if timestamp is None else timestamp}

  def to_dict(self):
    return self.data


def validate(channel, event):
  assert event.data['valid'], f'invalid {channel}'
  assert event.data['logMonoTime'] > 0, f'invalid timestamp {channel}'


class Peer:
  def __init__(self, cc=(), cs=(), code=0):
    self.queues = {name: deque(values) for name, values in zip(CHANNELS, (cc, cs), strict=True)}
    self.reads = {name: 0 for name in CHANNELS}
    self.code = code

  def read(self, channel):
    if self.queues[channel]:
      self.reads[channel] += 1
      return self.queues[channel].popleft()
    return None

  def poll(self):
    return self.code


def receiver(peer, clock):
  return Publications(peer.read, peer.poll, validate=validate, serialize=Message.to_dict,
                      clock=clock.monotonic, sleep=clock.sleep)


def original_receive(peer, clock):
  # This is the actual pre-fix nested function, extracted before changing the
  # checker. Compile only that AST: no cereal, msgq, source oracle or process.
  path = Path(__file__).with_name('fixtures') / 'joystick_receive_505_original.py'
  tree = ast.parse(path.read_text(), filename=str(path))
  function = next(node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == 'receive')
  namespace = {'time': clock, 'process': peer,
               'messaging': SimpleNamespace(recv_one_or_none=peer.read),
               'cc_socket': CHANNELS[0], 'cs_socket': CHANNELS[1], 'records': [],
               'snapshot': lambda cc, cs: (cc, cs)}
  exec(compile(ast.Module(body=[function], type_ignores=[]), str(path), 'exec'), namespace)
  return namespace['receive'], namespace['records']


def offset_peer():
  # Controlled startup offset consistent with afed's paired timestamp skew.
  # The old artifact did not retain the final partial message or exit status.
  return Peer([Message(CHANNELS[0], i) for i in range(240)],
              [Message(CHANNELS[1], i) for i in range(1, 240)])


def checker_finalizer(namespace, failure):
  # Exercise the real checker's finite-run finally block without importing its
  # cereal/source dependencies or launching a process. No copied cleanup logic.
  path = Path(__file__).resolve().parents[1] / 'check_joystickd_runtime.py'
  main = next(node for node in ast.parse(path.read_text()).body if isinstance(node, ast.FunctionDef) and node.name == 'main')
  finite_run = next(node for node in main.body if isinstance(node, ast.Try))
  wrapper = ast.parse('def finalize():\n  try:\n    if test_error is not None:\n      raise test_error\n  finally:\n    pass\n')
  wrapper.body[0].body[0].finalbody = finite_run.finalbody
  globals_ = {'test_error': failure, 'json': json, 'sys': sys, 'signal': signal, **namespace}
  exec(compile(ast.fix_missing_locations(wrapper), str(path), 'exec'), globals_)
  return globals_['finalize']


def checker_validator():
  path = Path(__file__).resolve().parents[1] / 'check_joystickd_runtime.py'
  function = next(node for node in ast.parse(path.read_text()).body
                  if isinstance(node, ast.FunctionDef) and node.name == 'validate_publication')
  namespace = {}
  exec(compile(ast.Module(body=[function], type_ignores=[]), str(path), 'exec'), namespace)
  return namespace['validate_publication']


class CleanupProcess:
  def __init__(self, *, code=None, cleanup_failure=None):
    self.code, self.cleanup_failure = code, cleanup_failure
    self.signals, self.waits = [], []

  def poll(self):
    return self.code

  def send_signal(self, value):
    self.signals.append(value)

  def wait(self, timeout):
    self.waits.append(timeout)
    if self.cleanup_failure is not None:
      raise self.cleanup_failure
    self.code = 0
    return self.code


class JoystickPublications505Tests(unittest.TestCase):
  def test_original_nested_receive_times_out_on_retained_final_cc(self):
    peer, clock = offset_peer(), Clock()
    receive, records = original_receive(peer, clock)
    for _ in range(239):
      self.assertIsNotNone(receive())
    with self.assertRaisesRegex(AssertionError, 'publication timeout'):
      receive()
    self.assertEqual(len(records), 239)
    self.assertEqual(peer.reads, {'carControl': 240, 'controlsState': 239})
    self.assertGreaterEqual(clock.now, 2.0)
    self.assertLess(clock.now, 2.002)  # The original deadline was not shortened.

  def test_startup_offset_keeps_every_message_and_pairs_then_tail(self):
    peer, clock = offset_peer(), Clock()
    stream = receiver(peer, clock)
    while stream.receive_pair() is not None:
      pass
    stream.validate_complete()
    self.assertEqual(len(stream.records), 239)
    self.assertEqual(stream.counts, {'carControl': 240, 'controlsState': 239})
    evidence = stream.diagnostics(returncode_before_cleanup=peer.poll())
    self.assertEqual(evidence['unmatched_tails']['carControl'], [Message('carControl', 239).to_dict()])
    self.assertEqual(evidence['unmatched_tails']['controlsState'], [])
    self.assertEqual(evidence['returncode_before_cleanup'], 0)
    self.assertEqual(evidence['producer_returncode_observed'], 0)
    self.assertEqual(clock.now, 0.0)  # No wait for a nonexistent counterpart.
    self.assertEqual(stream.records[0]['controlsState']['controlsState']['frame'], 1)

  def test_reverse_offset_retains_final_controls_state(self):
    peer = Peer([Message('carControl', i) for i in range(1, 240)],
                [Message('controlsState', i) for i in range(240)])
    stream = receiver(peer, Clock())
    while stream.receive_pair() is not None:
      pass
    stream.validate_complete()
    self.assertEqual(stream.counts, {'carControl': 239, 'controlsState': 240})
    self.assertEqual(stream.diagnostics(returncode_before_cleanup=0)['unmatched_tails']['controlsState'],
                     [Message('controlsState', 239).to_dict()])

  def test_delayed_channel_does_not_overwrite_independently_retained_messages(self):
    peer, clock = Peer([Message('carControl', i) for i in range(235)], code=None), Clock()
    stream = receiver(peer, clock)

    def deliver_after_cc_drain(seconds):
      clock.sleep(seconds)
      self.assertEqual(stream.counts['carControl'], 235)
      peer.queues['controlsState'].extend(Message('controlsState', i) for i in range(235))
      peer.code = 0

    stream.sleep = deliver_after_cc_drain
    while stream.receive_pair() is not None:
      pass
    stream.validate_complete()
    for i, pair in enumerate(stream.records):
      self.assertEqual(pair['carControl']['carControl']['frame'], i)
      self.assertEqual(pair['controlsState']['controlsState']['frame'], i)
    self.assertEqual(clock.now, 0.001)

  def test_dead_empty_eof_is_not_successful_count_validation(self):
    clock = Clock()
    stream = receiver(Peer(), clock)
    self.assertIsNone(stream.receive_pair())
    self.assertEqual(clock.now, 0.0)
    with self.assertRaisesRegex(AssertionError, 'paired publications'):
      stream.validate_complete()

  def test_producer_exit_between_reads_drains_remaining_available_pairs(self):
    peer = Peer([Message('carControl', i) for i in range(235)],
                [Message('controlsState', i) for i in range(235)], code=None)
    old_poll = peer.poll
    peer.poll = lambda: 0 if peer.reads['carControl'] >= 1 else old_poll()
    stream = receiver(peer, Clock())
    while stream.receive_pair() is not None:
      pass
    stream.validate_complete()
    self.assertEqual(len(stream.records), 235)
    self.assertFalse(any(peer.queues.values()))

  def test_empty_read_before_observed_exit_requires_another_drain_pass(self):
    peer = Peer(code=None)
    polls = 0

    def exit_after_empty_read():
      nonlocal polls
      polls += 1
      if polls == 1:
        for name in CHANNELS:
          peer.queues[name].extend(Message(name, i) for i in range(235))
        peer.code = 0
      return peer.code

    peer.poll = exit_after_empty_read
    stream = receiver(peer, Clock())
    while stream.receive_pair() is not None:
      pass
    stream.validate_complete()
    self.assertEqual(len(stream.records), 235)
    self.assertFalse(any(peer.queues.values()))

  def test_partial_progress_cannot_extend_receive_deadline(self):
    clock, peer = Clock(), Peer(code=None)

    def slow_one_channel(channel):
      if channel == 'carControl':
        clock.now += 0.5
        return Message(channel, int(clock.now * 2))
      return None

    peer.read = slow_one_channel
    stream = receiver(peer, clock)
    with self.assertRaisesRegex(AssertionError, 'publication timeout'):
      stream.receive_pair()
    self.assertEqual(clock.now, 2.0)
    self.assertEqual(stream.counts['carControl'], 4)

  def test_alive_partial_retains_message_and_times_out_at_original_deadline(self):
    clock = Clock()
    peer = Peer([Message('carControl', 1)], code=None)
    stream = receiver(peer, clock)
    with self.assertRaisesRegex(AssertionError, 'publication timeout'):
      stream.receive_pair()
    evidence = stream.diagnostics(returncode_before_cleanup=peer.poll())
    self.assertIsNone(evidence['returncode_before_cleanup'])
    self.assertEqual(evidence['unmatched_tails']['carControl'], [Message('carControl', 1).to_dict()])
    self.assertGreaterEqual(clock.now, 2.0)
    self.assertLess(clock.now, 2.002)

  def test_alive_empty_is_not_eof(self):
    stream = receiver(Peer(code=None), Clock())
    with self.assertRaisesRegex(AssertionError, 'publication timeout'):
      stream.receive_pair()

  def test_nonzero_exit_is_not_normal_eof_even_with_pairs_and_tail(self):
    peer = offset_peer()
    peer.code = 7
    stream = receiver(peer, Clock())
    with self.assertRaisesRegex(AssertionError, 'producer exited.*7'):
      while stream.receive_pair() is not None:
        pass
    evidence = stream.diagnostics(returncode_before_cleanup=peer.poll())
    self.assertEqual(evidence['returncode_before_cleanup'], 7)
    self.assertEqual(len(stream.records), 239)
    self.assertEqual(len(evidence['unmatched_tails']['carControl']), 1)
    with self.assertRaises(AssertionError):
      stream.validate_complete()

  def test_premature_zero_exit_cannot_hide_count_loss(self):
    peer = Peer([Message('carControl', i) for i in range(234)],
                [Message('controlsState', i) for i in range(234)])
    stream = receiver(peer, Clock())
    while stream.receive_pair() is not None:
      pass
    with self.assertRaisesRegex(AssertionError, 'paired publications'):
      stream.validate_complete()
    self.assertEqual(stream.counts, {'carControl': 234, 'controlsState': 234})

  def test_channel_retention_is_capped_at_requested_240(self):
    peer = Peer([Message('carControl', i) for i in range(241)])
    stream = receiver(peer, Clock())
    with self.assertRaisesRegex(AssertionError, 'carControl.*240'):
      stream.receive_pair()
    self.assertEqual(stream.counts['carControl'], 241)  # An explicit failed overrun.
    self.assertEqual(len(stream.channels['carControl']), 240)
    self.assertEqual(len(stream.diagnostics(returncode_before_cleanup=0)['unmatched_tails']['carControl']), 240)

  def test_invalid_or_zero_timestamp_unmatched_tail_is_not_accepted(self):
    for channel in CHANNELS:
      for values in ({'valid': False}, {'timestamp': 0}):
        with self.subTest(channel=channel, values=values):
          message = Message(channel, 0, **values)
          peer = Peer(**({'cc': [message]} if channel == 'carControl' else {'cs': [message]}))
          stream = receiver(peer, Clock())
          with self.assertRaises(AssertionError):
            stream.receive_pair()
          evidence = stream.diagnostics(returncode_before_cleanup=0)
          self.assertEqual(evidence['unmatched_tails'][channel], [message.to_dict()])
          self.assertEqual(evidence['validated_channel_counts'][channel], 0)

  def test_actual_checker_validator_checks_unmatched_payloads_and_debug_state(self):
    for channel in CHANNELS:
      invalid_values = [{'valid': False}, {'logMonoTime': 0}, {'which': lambda: 'carState'}]
      if channel == 'controlsState':
        invalid_values.append({'controlsState': SimpleNamespace(lateralControlState=SimpleNamespace(which=lambda: 'pidState'))})
      for values in invalid_values:
        with self.subTest(channel=channel, invalid=values):
          event = Message(channel, 0)
          event.valid, event.logMonoTime = True, 1
          event.which = lambda: channel
          event.controlsState = SimpleNamespace(lateralControlState=SimpleNamespace(which=lambda: 'debugState'))
          for name, value in values.items():
            setattr(event, name, value)
          peer = Peer(**({'cc': [event]} if channel == 'carControl' else {'cs': [event]}))
          stream = receiver(peer, Clock())
          stream.validate = checker_validator()
          with self.assertRaises(AssertionError):
            stream.receive_pair()
          evidence = stream.diagnostics(returncode_before_cleanup=0)
          self.assertEqual(evidence['unmatched_tails'][channel], [event.to_dict()])
          self.assertEqual(evidence['validated_channel_counts'][channel], 0)

  def test_checker_failure_saves_real_partial_and_pre_cleanup_running_status(self):
    for cleanup_failure in (None, TimeoutError('stopped producer')):
      with self.subTest(cleanup_failure=cleanup_failure), TemporaryDirectory() as directory:
        peer = Peer([Message('carControl', 0)], code=None)
        stream = receiver(peer, Clock())
        with self.assertRaisesRegex(AssertionError, 'publication timeout') as caught:
          stream.receive_pair()
        process = CleanupProcess(cleanup_failure=cleanup_failure)
        path = Path(directory)
        stdout, stderr = (path / 'stdout').open('w'), (path / 'stderr').open('w')
        finalize = checker_finalizer({'process': process, 'receiver': stream,
                                     'records': stream.records, 'matches': [], 'args': SimpleNamespace(output=path),
                                     'stdout': stdout, 'stderr': stderr}, caught.exception)
        with self.assertRaises(AssertionError) as propagated:
          finalize()
        self.assertIs(propagated.exception, caught.exception)
        evidence = json.loads((path / 'receiver.json').read_text())
        self.assertIsNone(evidence['returncode_before_cleanup'])
        self.assertEqual(evidence['failure'], {'type': 'AssertionError', 'message': 'publication timeout'})
        self.assertEqual(evidence['unmatched_tails']['carControl'], [Message('carControl', 0).to_dict()])
        self.assertEqual(evidence['returncode_after_cleanup'], 0 if cleanup_failure is None else None)
        self.assertEqual('cleanup_failure' in evidence, cleanup_failure is not None)
        self.assertEqual(json.loads((path / 'publications.json').read_text()), [])
        self.assertEqual(json.loads((path / 'channel_publications.json').read_text()), stream.channels)
        self.assertEqual(process.signals, [signal.SIGTERM])
        self.assertEqual(process.waits, [3])
        self.assertTrue(stdout.closed and stderr.closed)

  def test_checker_cleanup_error_without_prior_failure_still_fails(self):
    with TemporaryDirectory() as directory:
      path = Path(directory)
      process = CleanupProcess(cleanup_failure=TimeoutError('stopped producer'))
      stream = receiver(Peer(code=None), Clock())
      stdout, stderr = (path / 'stdout').open('w'), (path / 'stderr').open('w')
      finalize = checker_finalizer({'process': process, 'receiver': stream, 'records': [], 'matches': [],
                                   'args': SimpleNamespace(output=path), 'stdout': stdout, 'stderr': stderr}, None)
      with self.assertRaisesRegex(TimeoutError, 'stopped producer'):
        finalize()
      evidence = json.loads((path / 'receiver.json').read_text())
      self.assertIsNone(evidence['returncode_before_cleanup'])
      self.assertIsNone(evidence['failure'])
      self.assertEqual(evidence['cleanup_failure']['type'], 'TimeoutError')


if __name__ == '__main__':
  unittest.main()
