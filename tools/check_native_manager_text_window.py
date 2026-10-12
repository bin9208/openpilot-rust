"""Capture and dismiss the real native Text Viewer on an owned X11 display."""

import ctypes
import sys
import time
from pathlib import Path

from PIL import ImageGrab


def main() -> None:
  display_name, screenshot = sys.argv[1:]
  x11 = ctypes.CDLL("libX11.so.6")
  xtst = ctypes.CDLL("libXtst.so.6")
  display_type = ctypes.c_void_p
  window_type = ctypes.c_ulong
  x11.XOpenDisplay.argtypes = [ctypes.c_char_p]
  x11.XOpenDisplay.restype = display_type
  x11.XDefaultRootWindow.argtypes = [display_type]
  x11.XDefaultRootWindow.restype = window_type
  x11.XQueryTree.argtypes = [
    display_type,
    window_type,
    ctypes.POINTER(window_type),
    ctypes.POINTER(window_type),
    ctypes.POINTER(ctypes.POINTER(window_type)),
    ctypes.POINTER(ctypes.c_uint),
  ]
  x11.XFetchName.argtypes = [display_type, window_type, ctypes.POINTER(ctypes.c_char_p)]
  x11.XGetGeometry.argtypes = [
    display_type,
    window_type,
    ctypes.POINTER(window_type),
    ctypes.POINTER(ctypes.c_int),
    ctypes.POINTER(ctypes.c_int),
    ctypes.POINTER(ctypes.c_uint),
    ctypes.POINTER(ctypes.c_uint),
    ctypes.POINTER(ctypes.c_uint),
    ctypes.POINTER(ctypes.c_uint),
  ]
  x11.XFree.argtypes = [ctypes.c_void_p]
  x11.XFlush.argtypes = [display_type]
  x11.XCloseDisplay.argtypes = [display_type]
  xtst.XTestFakeMotionEvent.argtypes = [display_type, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_ulong]
  xtst.XTestFakeButtonEvent.argtypes = [display_type, ctypes.c_uint, ctypes.c_int, ctypes.c_ulong]
  display = x11.XOpenDisplay(display_name.encode())
  if not display:
    raise RuntimeError("owned X11 display unavailable")
  try:
    root = x11.XDefaultRootWindow(display)
    deadline = time.monotonic() + 45
    while time.monotonic() < deadline:
      parent = window_type()
      returned_root = window_type()
      children = ctypes.POINTER(window_type)()
      count = ctypes.c_uint()
      if not x11.XQueryTree(display, root, ctypes.byref(returned_root), ctypes.byref(parent), ctypes.byref(children), ctypes.byref(count)):
        raise RuntimeError("X11 tree lookup failed")
      windows = [children[index] for index in range(count.value)]
      if children:
        x11.XFree(children)
      for window in windows:
        name = ctypes.c_char_p()
        x11.XFetchName(display, window, ctypes.byref(name))
        title = name.value
        if name:
          x11.XFree(name)
        if title != b"Text Viewer":
          continue
        x = ctypes.c_int()
        y = ctypes.c_int()
        width = ctypes.c_uint()
        height = ctypes.c_uint()
        border = ctypes.c_uint()
        depth = ctypes.c_uint()
        if not x11.XGetGeometry(
          display,
          window,
          ctypes.byref(returned_root),
          ctypes.byref(x),
          ctypes.byref(y),
          ctypes.byref(width),
          ctypes.byref(height),
          ctypes.byref(border),
          ctypes.byref(depth),
        ):
          raise RuntimeError("X11 window geometry unavailable")
        image = ImageGrab.grab(bbox=(x.value, y.value, x.value + width.value, y.value + height.value), xdisplay=display_name).convert("RGB")
        scale = width.value / 536
        body = tuple(round(value * scale) for value in (20, 48, 330, 130))
        button = tuple(round(value * scale) for value in (336, 140, 487, 220))
        if image.crop(body).getbbox() is None or image.crop(button).getbbox() is None:
          continue
        image.save(Path(screenshot))
        click_x = x.value + round(411 * scale)
        click_y = y.value + round(180 * scale)
        xtst.XTestFakeMotionEvent(display, 0, click_x, click_y, 0)
        x11.XFlush(display)
        xtst.XTestFakeButtonEvent(display, 1, 1, 0)
        x11.XFlush(display)
        time.sleep(0.15)
        xtst.XTestFakeButtonEvent(display, 1, 0, 0)
        x11.XFlush(display)
        print(f"Captured Text Viewer {window} ({width.value}x{height.value}) and clicked Exit at {click_x},{click_y}")
        return
      time.sleep(0.1)
    raise TimeoutError("native Text Viewer did not appear")
  finally:
    x11.XCloseDisplay(display)


if __name__ == "__main__":
  main()
