#!/usr/bin/env python3
"""Run only under the prepared ARM Python in Actions; no model/GPU/device work."""
import argparse
import _crypt
import _dbm
import _tkinter
import bz2
import ctypes
import encodings
import hashlib
import json
import lzma
import math
import os
from pathlib import Path
import platform
import sqlite3
import ssl
import sys
import sysconfig
import zlib


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--prefix', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  prefix = args.prefix.resolve()
  assert tuple(sys.version_info[:3]) == (3, 12, 16)
  assert sys.implementation.cache_tag == 'cpython-312' and platform.machine() == 'aarch64'
  assert sys.flags.isolated == 1 and sys.flags.dont_write_bytecode == 1 and sys.flags.no_site == 1
  assert sys.getdlopenflags() & os.RTLD_NOW
  assert Path(sys.executable).resolve() == prefix/'bin/python3.12'
  assert Path(sys.prefix).resolve() == Path(sys.base_prefix).resolve() == prefix
  assert sysconfig.get_config_var('SOABI') == 'cpython-312-aarch64-linux-gnu'
  extensions = (_crypt, _dbm, _tkinter)
  for module in (os, encodings, ctypes, bz2, lzma, ssl, sqlite3, *extensions):
    assert Path(module.__file__).resolve().is_relative_to(prefix)
  libc = ctypes.CDLL(None)
  libc.getpid.restype = ctypes.c_int
  assert libc.getpid() == os.getpid()
  data = b'owned native Python standard-library ABI check\x00'*100
  for module in (bz2, lzma, zlib):
    assert module.decompress(module.compress(data)) == data
  assert math.isclose(math.sqrt(2)**2, 2, rel_tol=1e-15)
  assert hashlib.sha256(b'abc').hexdigest() == 'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad'
  with sqlite3.connect(':memory:') as database:
    assert database.execute('select ? + ?', (19, 23)).fetchone() == (42,)
  with Path('/proc/self/maps').open('rb') as stream:
    maps = stream.read(4*1024**2+1)
  assert len(maps) <= 4*1024**2
  args.output.with_suffix('.maps.txt').write_bytes(maps)
  result = dict(schema=1, version=platform.python_version(), architecture=platform.machine(),
      executable=sys.executable, prefix=str(prefix), soabi=sysconfig.get_config_var('SOABI'),
      ssl_version=ssl.OPENSSL_VERSION, sqlite_version=sqlite3.sqlite_version,
      isolated=True, source_checkout_imported=False, bytecode_written=False,
      standard_library_checks=['prefix', 'ctypes-getpid', 'bz2', 'lzma', 'zlib', 'math', 'sha256', 'sqlite3', 'ssl-import'],
      extension_modules=[module.__name__ for module in extensions],
      extension_load_now=True,
      python_model_environment_complete=False, model_generated=False, gpu_executed=False,
      physical_device_execution=False)
  args.output.write_text(json.dumps(result, indent=2)+'\n')


if __name__ == '__main__':
  main()
