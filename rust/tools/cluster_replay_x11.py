# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Owned X11 input adapter for the replay-window comparator."""
from __future__ import annotations

import ctypes as c
import os


class XInput:
    def __init__(self) -> None:
        self.x = c.CDLL("libX11.so.6")
        self.t = c.CDLL("libXtst.so.6")
        self.x.XOpenDisplay.argtypes = [c.c_char_p]
        self.x.XOpenDisplay.restype = c.c_void_p
        self.display = self.x.XOpenDisplay(os.environ["DISPLAY"].encode())
        assert self.display
        self.x.XDefaultRootWindow.argtypes = [c.c_void_p]
        self.x.XDefaultRootWindow.restype = c.c_ulong
        self.root = self.x.XDefaultRootWindow(self.display)
        self.x.XQueryTree.argtypes = [c.c_void_p, c.c_ulong, c.POINTER(c.c_ulong), c.POINTER(c.c_ulong), c.POINTER(c.POINTER(c.c_ulong)), c.POINTER(c.c_uint)]
        self.x.XFetchName.argtypes = [c.c_void_p, c.c_ulong, c.POINTER(c.c_char_p)]
        self.x.XFree.argtypes = [c.c_void_p]
        self.x.XSync.argtypes = [c.c_void_p, c.c_int]
        self.x.XSetInputFocus.argtypes = [c.c_void_p, c.c_ulong, c.c_int, c.c_ulong]
        self.x.XTranslateCoordinates.argtypes = [c.c_void_p, c.c_ulong, c.c_ulong, c.c_int, c.c_int, c.POINTER(c.c_int), c.POINTER(c.c_int), c.POINTER(c.c_ulong)]
        self.x.XStringToKeysym.argtypes = [c.c_char_p]
        self.x.XStringToKeysym.restype = c.c_ulong
        self.x.XKeysymToKeycode.argtypes = [c.c_void_p, c.c_ulong]
        self.x.XKeysymToKeycode.restype = c.c_uint
        self.x.XCloseDisplay.argtypes = [c.c_void_p]
        self.t.XTestFakeMotionEvent.argtypes = [c.c_void_p, c.c_int, c.c_int, c.c_int, c.c_ulong]
        self.t.XTestFakeButtonEvent.argtypes = [c.c_void_p, c.c_uint, c.c_int, c.c_ulong]
        self.t.XTestFakeKeyEvent.argtypes = [c.c_void_p, c.c_uint, c.c_int, c.c_ulong]

    def window(self, title: str) -> int:
        root, parent, children, count = c.c_ulong(), c.c_ulong(), c.POINTER(c.c_ulong)(), c.c_uint()
        assert self.x.XQueryTree(self.display, self.root, c.byref(root), c.byref(parent), c.byref(children), c.byref(count))
        found = []
        try:
            for index in range(count.value):
                name = c.c_char_p()
                try:
                    if self.x.XFetchName(self.display, children[index], c.byref(name)) and name.value == title.encode():
                        found.append(int(children[index]))
                finally:
                    if name:
                        self.x.XFree(name)
        finally:
            if children:
                self.x.XFree(children)
        assert len(found) == 1, (title, found)
        self.x.XSetInputFocus(self.display, found[0], 1, 0)
        self.sync()
        return found[0]

    def input(self, window: int, scale: float, point: tuple[float, float], down: bool, keys: tuple[str, ...]) -> None:
        px, py, child = c.c_int(), c.c_int(), c.c_ulong()
        assert self.x.XTranslateCoordinates(self.display, window, self.root, 0, 0, c.byref(px), c.byref(py), c.byref(child))
        assert self.t.XTestFakeMotionEvent(self.display, 0, px.value + round(point[0] * scale), py.value + round(point[1] * scale), 0)
        assert self.t.XTestFakeButtonEvent(self.display, 1, int(down), 0)
        for key in keys:
            code = self.x.XKeysymToKeycode(self.display, self.x.XStringToKeysym(key.encode()))
            assert code and self.t.XTestFakeKeyEvent(self.display, code, 1, 0)
        self.sync()

    def release_keys(self, keys: tuple[str, ...]) -> None:
        for key in keys:
            code = self.x.XKeysymToKeycode(self.display, self.x.XStringToKeysym(key.encode()))
            assert self.t.XTestFakeKeyEvent(self.display, code, 0, 0)
        self.sync()

    def sync(self) -> None:
        self.x.XSync(self.display, 0)

    def close(self) -> None:
        self.t.XTestFakeButtonEvent(self.display, 1, 0, 0)
        self.sync()
        self.x.XCloseDisplay(self.display)
