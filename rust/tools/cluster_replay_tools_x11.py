from __future__ import annotations
import ctypes
from ctypes import c_int, c_uint, c_ulong, c_char_p, c_void_p, POINTER, byref
from dataclasses import dataclass
import time


@dataclass(frozen=True, slots=True)
class Window:
  identifier: int
  title: str
  x: int
  y: int
  width: int
  height: int


class Client:
  def __init__(self, display: str) -> None:
    self.x = ctypes.CDLL('libX11.so.6')
    self.xt = ctypes.CDLL('libXtst.so.6')
    signatures = {
      'XOpenDisplay': ([c_char_p], c_void_p),
      'XCloseDisplay': ([c_void_p], c_int),
      'XDefaultRootWindow': ([c_void_p], c_ulong),
      'XQueryTree': ([c_void_p, c_ulong, POINTER(c_ulong), POINTER(c_ulong), POINTER(POINTER(c_ulong)), POINTER(c_uint)], c_int),
      'XFetchName': ([c_void_p, c_ulong, POINTER(c_char_p)], c_int),
      'XFree': ([c_void_p], c_int),
      'XGetWindowProperty': (
        [
          c_void_p,
          c_ulong,
          c_ulong,
          ctypes.c_long,
          ctypes.c_long,
          c_int,
          c_ulong,
          POINTER(c_ulong),
          POINTER(c_int),
          POINTER(c_ulong),
          POINTER(c_ulong),
          POINTER(POINTER(ctypes.c_ubyte)),
        ],
        c_int,
      ),
      'XGetGeometry': (
        [c_void_p, c_ulong, POINTER(c_ulong), POINTER(c_int), POINTER(c_int), POINTER(c_uint), POINTER(c_uint), POINTER(c_uint), POINTER(c_uint)],
        c_int,
      ),
      'XTranslateCoordinates': ([c_void_p, c_ulong, c_ulong, c_int, c_int, POINTER(c_int), POINTER(c_int), POINTER(c_ulong)], c_int),
      'XSetInputFocus': ([c_void_p, c_ulong, c_int, c_ulong], c_int),
      'XFlush': ([c_void_p], c_int),
      'XResizeWindow': ([c_void_p, c_ulong, c_uint, c_uint], c_int),
      'XStringToKeysym': ([c_char_p], c_ulong),
      'XKeysymToKeycode': ([c_void_p, c_ulong], ctypes.c_ubyte),
      'XInternAtom': ([c_void_p, c_char_p, c_int], c_ulong),
      'XSendEvent': ([c_void_p, c_ulong, c_int, ctypes.c_long, c_void_p], c_int),
    }
    for name, (arguments, result) in signatures.items():
      function = getattr(self.x, name)
      function.argtypes = arguments
      function.restype = result
    for name, arguments in [
      ('XTestFakeMotionEvent', [c_void_p, c_int, c_int, c_int, c_ulong]),
      ('XTestFakeButtonEvent', [c_void_p, c_uint, c_int, c_ulong]),
      ('XTestFakeKeyEvent', [c_void_p, c_uint, c_int, c_ulong]),
    ]:
      function = getattr(self.xt, name)
      function.argtypes = arguments
      function.restype = c_int
    self.display = self.x.XOpenDisplay(display.encode())
    if not self.display:
      raise RuntimeError(f'X11 display unavailable: {display}')
    self.root = self.x.XDefaultRootWindow(self.display)

  def close(self) -> None:
    self.x.XCloseDisplay(self.display)

  def windows(self) -> list[Window]:
    root = c_ulong()
    parent = c_ulong()
    children = POINTER(c_ulong)()
    count = c_uint()
    self.x.XQueryTree(self.display, self.root, byref(root), byref(parent), byref(children), byref(count))
    result = []
    try:
      for index in range(count.value):
        identifier = children[index]
        name = c_char_p()
        if self.x.XFetchName(self.display, identifier, byref(name)):
          try:
            title = name.value.decode(errors='replace') if name.value else ''
          finally:
            self.x.XFree(name)
        else:
          title = self.property(identifier, '_NET_WM_NAME')
        width = c_uint()
        height = c_uint()
        border = c_uint()
        depth = c_uint()
        x = c_int()
        y = c_int()
        self.x.XGetGeometry(self.display, identifier, byref(root), byref(x), byref(y), byref(width), byref(height), byref(border), byref(depth))
        child = c_ulong()
        self.x.XTranslateCoordinates(self.display, identifier, self.root, 0, 0, byref(x), byref(y), byref(child))
        result.append(Window(identifier, title, x.value, y.value, width.value, height.value))
    finally:
      self.x.XFree(children)
    return result

  def property(self, identifier: int, name: str) -> str:
    kind = c_ulong()
    fmt = c_int()
    count = c_ulong()
    after = c_ulong()
    value = POINTER(ctypes.c_ubyte)()
    atom = self.x.XInternAtom(self.display, name.encode(), 0)
    self.x.XGetWindowProperty(self.display, identifier, atom, 0, 4096, 0, 0, byref(kind), byref(fmt), byref(count), byref(after), byref(value))
    try:
      return ctypes.string_at(value, count.value).decode(errors='replace') if value and fmt.value == 8 else ''
    finally:
      self.x.XFree(value)

  def wait(self, title: str, timeout: float = 5.0) -> Window:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
      for window in self.windows():
        if window.title == title:
          return window
      time.sleep(0.025)
    raise RuntimeError(f'Window {title!r} not found: {self.windows()}')

  def focus(self, window: Window) -> None:
    self.x.XSetInputFocus(self.display, window.identifier, 2, 0)
    self.x.XFlush(self.display)

  def motion(self, x: int, y: int) -> None:
    self.xt.XTestFakeMotionEvent(self.display, -1, x, y, 0)
    self.x.XFlush(self.display)

  def button(self, down: bool) -> None:
    self.xt.XTestFakeButtonEvent(self.display, 1, int(down), 0)
    self.x.XFlush(self.display)

  def click(self, window: Window, x: int, y: int) -> None:
    self.motion(window.x + x, window.y + y)
    self.button(True)
    time.sleep(0.015)
    self.button(False)

  def key(self, name: str, down: bool) -> None:
    code = self.x.XKeysymToKeycode(self.display, self.x.XStringToKeysym(name.encode()))
    if not code:
      raise RuntimeError(f'Unknown key {name}')
    self.xt.XTestFakeKeyEvent(self.display, code, int(down), 0)
    self.x.XFlush(self.display)

  def press(self, name: str) -> None:
    self.key(name, True)
    self.key(name, False)

  def type_path(self, value: str) -> None:
    for character in value:
      name = {'/': 'slash', '.': 'period', '-': 'minus', '_': 'underscore', ' ': 'space'}.get(character, character)
      if character == '_':
        self.key('Shift_L', True)
      self.press(name)
      if character == '_':
        self.key('Shift_L', False)

  def resize(self, window: Window, width: int, height: int) -> None:
    self.x.XResizeWindow(self.display, window.identifier, width, height)
    self.x.XFlush(self.display)

  def wm_close(self, window: Window) -> None:
    class Data(ctypes.Union):
      _fields_ = [('longs', ctypes.c_long * 5), ('bytes', ctypes.c_char * 20)]

    class Message(ctypes.Structure):
      _fields_ = [
        ('type', c_int),
        ('serial', c_ulong),
        ('send_event', c_int),
        ('display', c_void_p),
        ('window', c_ulong),
        ('message_type', c_ulong),
        ('format', c_int),
        ('data', Data),
      ]

    class Event(ctypes.Union):
      _fields_ = [('message', Message), ('padding', ctypes.c_long * 24)]

    event = Event()
    event.message.type = 33
    event.message.display = self.display
    event.message.window = window.identifier
    event.message.message_type = self.x.XInternAtom(self.display, b'WM_PROTOCOLS', 0)
    event.message.format = 32
    event.message.data.longs[0] = self.x.XInternAtom(self.display, b'WM_DELETE_WINDOW', 0)
    self.x.XSendEvent(self.display, window.identifier, 0, 0, byref(event))
    self.x.XFlush(self.display)
