"""Oracle Params boundary: preserve native stoi/stof conversion and record operations."""

import ctypes
import errno
from dataclasses import dataclass, field


class FatalParameter(RuntimeError):
  pass


LIBC = ctypes.CDLL(None, use_errno=True)
LIBC.strtof.argtypes = [ctypes.c_char_p, ctypes.POINTER(ctypes.c_void_p)]
LIBC.strtof.restype = ctypes.c_float
LIBC.strtol.argtypes = [ctypes.c_char_p, ctypes.POINTER(ctypes.c_void_p), ctypes.c_int]
LIBC.strtol.restype = ctypes.c_long


def number(data, floating):
  if not data:
    return 0.0 if floating else 0
  buffer = ctypes.create_string_buffer(data)
  end = ctypes.c_void_p()
  ctypes.set_errno(0)
  value = LIBC.strtof(buffer, ctypes.byref(end)) if floating else LIBC.strtol(buffer, ctypes.byref(end), 10)
  if end.value == ctypes.addressof(buffer) or ctypes.get_errno() == errno.ERANGE or (not floating and not -(2**31) <= value < 2**31):
    raise FatalParameter('source native conversion failed')
  return value


@dataclass
class Store:
  """Mutable fixture storage with a chronological Params operation ledger."""

  values: dict[str, bytes] = field(default_factory=dict)
  operations: list = field(default_factory=list)

  def raw(self, key):
    return self.values.get(key, b'')

  def get(self, key, block=False):
    self.operations.append(['get', key])
    value = self.raw(key)
    if not value:
      return None
    return value if key == 'CarParams' else value.decode('utf-8')

  def get_int(self, key, block=False):
    self.operations.append(['get_int', key])
    return number(self.raw(key), False)

  def get_float(self, key, block=False):
    self.operations.append(['get_float', key])
    return number(self.raw(key), True)

  def get_bool(self, key, block=False):
    self.operations.append(['get_bool', key])
    return self.raw(key) == b'1'

  def put_int(self, key, value):
    self.operations.append(['put_int', key, value])
    self.values[key] = str(value).encode()

  def put_bool(self, key, value):
    self.operations.append(['put_bool', key, value])
    self.values[key] = b'1' if value else b'0'

  def put_nonblocking(self, key, value):
    self.operations.append(['put_nonblocking', key, value])
    self.values[key] = value.encode() if isinstance(value, str) else value
