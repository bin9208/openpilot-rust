"""Bounded independent subscriptions for the finite joystick runtime check (#505)."""
from __future__ import annotations

from collections import deque
import time


class Publications:
  CHANNELS = ('carControl', 'controlsState')
  REQUESTED = 240
  MINIMUM = 235
  RECEIVE_TIMEOUT = 2

  def __init__(self, read, poll, *, validate, serialize, clock=time.monotonic, sleep=time.sleep):
    self.read, self.poll = read, poll
    self.validate, self.serialize = validate, serialize
    self.clock, self.sleep = clock, sleep
    self.channels = {name: [] for name in self.CHANNELS}
    self.counts = dict.fromkeys(self.CHANNELS, 0)
    self.validated_counts = dict.fromkeys(self.CHANNELS, 0)
    self.pending = {name: deque() for name in self.CHANNELS}
    self.records = []
    self.producer_returncode_observed = None
    self.drained = False

  def receive_pair(self):
    if self.drained:
      return None
    deadline = self.clock() + self.RECEIVE_TIMEOUT
    while True:
      exited_before_reads = self.producer_returncode_observed is not None
      progress = False
      for name in self.CHANNELS:
        event = self.read(name)
        if event is not None:
          progress = True
          self.counts[name] += 1
          assert self.counts[name] <= self.REQUESTED, f'{name} exceeded requested {self.REQUESTED} publications'
          saved = self.serialize(event)
          self.channels[name].append(saved)
          self.pending[name].append((event, saved))
          # Retain failed messages too; validation includes unmatched tails.
          self.validate(name, event)
          self.validated_counts[name] += 1
      self.producer_returncode_observed = self.poll()
      assert self.clock() < deadline, 'publication timeout'
      if all(self.pending.values()):
        pair = [self.pending[name].popleft() for name in self.CHANNELS]
        self.records.append({name: saved for name, (_, saved) in zip(self.CHANNELS, pair, strict=True)})
        return tuple(event for event, _ in pair)
      if self.producer_returncode_observed is not None and not progress:
        # Exit may race an empty read. Only an empty pass AFTER observing exit
        # proves that no queued message remains on either subscription.
        if exited_before_reads:
          assert self.producer_returncode_observed == 0, f'producer exited with code {self.producer_returncode_observed}'
          self.drained = True
          return None
        continue
      if not progress:
        self.sleep(0.001)

  def validate_complete(self):
    assert self.drained and self.producer_returncode_observed == 0, 'producer did not complete a successful drain'
    assert len(self.records) >= self.MINIMUM, f'paired publications: {len(self.records)} < {self.MINIMUM}'
    for name in self.CHANNELS:
      assert self.MINIMUM <= self.counts[name] <= self.REQUESTED, (name, self.counts[name])
      assert self.validated_counts[name] == self.counts[name], f'unvalidated {name} publications'

  def diagnostics(self, *, returncode_before_cleanup):
    tails = {name: [saved for _, saved in self.pending[name]] for name in self.CHANNELS}
    return {'requested_per_channel': self.REQUESTED, 'receive_timeout_seconds': self.RECEIVE_TIMEOUT,
            'paired_count': len(self.records), 'channel_counts': dict(self.counts),
            'validated_channel_counts': dict(self.validated_counts),
            'retained_channel_counts': {name: len(saved) for name, saved in self.channels.items()},
            'unmatched_tail_counts': {name: len(saved) for name, saved in tails.items()},
            'unmatched_tails': tails, 'drained': self.drained,
            'producer_returncode_observed': self.producer_returncode_observed,
            'returncode_before_cleanup': returncode_before_cleanup}
