#!/usr/bin/env python3
# /// script
# requires-python = ">=3.12"
# dependencies = ["Cython==3.2.4", "setuptools==82.0.1"]
# ///
# Run: python rust/tools/build_visionipc_python.py --output /tmp/model-native-python
"""Build the original message and camera Python bindings for native startup QA."""
import argparse
from pathlib import Path
import shutil

from Cython.Build import cythonize
from setuptools import Distribution, Extension
from setuptools.command.build_ext import build_ext

from build_msgq_python import ROOT, build as build_msgq


def build(output: Path) -> None:
  build_msgq(output)
  source = output / 'source/msgq/visionipc'
  source.mkdir(parents=True)
  for name in ['__init__.py', 'visionipc.pxd', 'visionipc_pyx.pxd', 'visionipc_pyx.pyx']:
    shutil.copyfile(ROOT / 'msgq_repo/msgq/visionipc' / name, source / name)
  native = [str(ROOT / 'msgq_repo/msgq' / name) for name in ['ipc.cc', 'event.cc', 'impl_msgq.cc', 'impl_fake.cc', 'msgq.cc',
    'visionipc/visionipc.cc', 'visionipc/visionipc_client.cc', 'visionipc/visionipc_server.cc', 'visionipc/visionbuf.cc']]
  extension = Extension('msgq.visionipc.visionipc_pyx', [str(source / 'visionipc_pyx.pyx'), *native],
                        include_dirs=[str(ROOT / 'msgq_repo')], language='c++', extra_compile_args=['-std=c++17', '-UNDEBUG'])
  distribution = Distribution({'ext_modules': cythonize([extension], build_dir=str(output / 'cython-vision'), quiet=True)})
  command = build_ext(distribution)
  command.ensure_finalized()
  command.build_lib = str(output)
  command.build_temp = str(output / 'build-vision')
  command.run()
  shutil.copyfile(source / '__init__.py', output / 'msgq/visionipc/__init__.py')


if __name__ == '__main__':
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument('--output', required=True, type=Path)
  build(parser.parse_args().output.resolve())
