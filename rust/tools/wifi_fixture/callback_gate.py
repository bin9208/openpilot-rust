from __future__ import annotations

from threading import Event


class ForgottenCallbackGate:
  def __init__(self, manager):
    self.original = manager._enqueue_callbacks
    self.callbacks = manager._forgotten
    self.armed = False
    self.blocked = Event()
    self.released = Event()
    manager._enqueue_callbacks = self.enqueue

  def enqueue(self, callbacks, *args):
    if self.armed and callbacks is self.callbacks and args == ('B',):
      self.armed = False
      self.blocked.set()
      if not self.released.wait(8):
        raise TimeoutError('fixture final Forgotten callback was not released')
    self.original(callbacks, *args)

  def snapshot(self):
    return {'armed': self.armed, 'blocked': self.blocked.is_set(), 'released': self.released.is_set()}
