from __future__ import annotations

from Xlib import X, XK, display
from Xlib.ext import xtest
from Xlib.xobject.drawable import Window
from check_ui_product_runtime import Driver, Frame


class Input:
  def __init__(self, connection: display.Display, window: Window, driver: Driver) -> None:
    self.connection, self.window, self.driver = connection, window, driver

  def frames(self, count: int = 3) -> Frame:
    start = self.driver.wait(lambda frame: frame.rendered)
    return self.driver.wait(lambda frame: frame.draw >= start.draw + count)

  def motion(self, x: int, y: int) -> None:
    position = self.connection.screen().root.translate_coords(self.window, x, y)
    xtest.fake_input(self.connection, X.MotionNotify, x=position.x, y=position.y)
    self.connection.sync()

  def button(self, pressed: bool) -> None:
    xtest.fake_input(self.connection, X.ButtonPress if pressed else X.ButtonRelease, 1)
    self.connection.sync()

  def click(self, x: int, y: int) -> None:
    self.motion(x, y)
    self.button(True)
    self.frames()
    self.button(False)
    self.frames()

  def drag(self, start: tuple[int, int], end: tuple[int, int]) -> Frame:
    self.motion(*start)
    self.button(True)
    self.frames(2)
    for fraction in range(1, 9):
      self.motion(round(start[0] + (end[0] - start[0]) * fraction / 8), round(start[1] + (end[1] - start[1]) * fraction / 8))
      self.frames(2)
    self.frames(4)
    self.button(False)
    return self.frames(15)

  def key(self, name: str) -> None:
    code = self.connection.keysym_to_keycode(XK.string_to_keysym(name))
    xtest.fake_input(self.connection, X.KeyPress, code)
    self.connection.sync()
    self.frames(2)
    xtest.fake_input(self.connection, X.KeyRelease, code)
    self.connection.sync()
    self.frames(2)

  def dismiss(self, stack: int) -> None:
    self.drag((260, 12), (260, 220))
    self.driver.wait(lambda frame: frame.stack == stack)
    self.driver.wait(lambda frame: frame.navigation_y is None or abs(frame.navigation_y) < 1)
