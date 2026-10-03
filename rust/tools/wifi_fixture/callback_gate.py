from __future__ import annotations

from threading import Event


class ForgottenCallbackGate:
  def __init__(self, manager):
    self.original = manager._enqueue_callbacks
    self.callbacks = manager._forgotten
    self.armed = False
    self.approaching = Event()
    self.entry_allowed = Event()
    self.blocked = Event()
    self.released = Event()
    manager._enqueue_callbacks = self.enqueue

  def enqueue(self, callbacks, *args):
    if self.armed and callbacks is self.callbacks and args == ('B',):
      self.armed = False
      self.approaching.set()
      if not self.entry_allowed.wait(8):
        raise TimeoutError('fixture final Forgotten callback gate entry was not permitted')
      self.blocked.set()
      if not self.released.wait(8):
        raise TimeoutError('fixture final Forgotten callback was not released')
    self.original(callbacks, *args)

  def snapshot(self):
    return {'armed': self.armed, 'approaching': self.approaching.is_set(), 'entry_allowed': self.entry_allowed.is_set(),
            'blocked': self.blocked.is_set(), 'released': self.released.is_set()}
