#!/usr/bin/env python3
import argparse
from pathlib import Path
import shutil

from Cython.Build import cythonize
from setuptools import Distribution, Extension
from setuptools.command.build_ext import build_ext

ROOT = Path(__file__).resolve().parents[2]


def build(output: Path) -> None:
  source = output / "source/msgq"
  source.mkdir(parents=True, exist_ok=True)
  for name in ["__init__.py", "ipc.pxd", "ipc_pyx.pyx"]:
    shutil.copyfile(ROOT / "msgq_repo/msgq" / name, source / name)
  native = [str(ROOT / "msgq_repo/msgq" / name) for name in ["ipc.cc", "event.cc", "impl_msgq.cc", "impl_fake.cc", "msgq.cc"]]
  extension = Extension("msgq.ipc_pyx", [str(source / "ipc_pyx.pyx"), *native],
                        include_dirs=[str(ROOT / "msgq_repo")], language="c++", extra_compile_args=["-std=c++17", "-UNDEBUG"])
  distribution = Distribution({"ext_modules": cythonize([extension], build_dir=str(output / "cython"), quiet=True)})
  command = build_ext(distribution)
  command.ensure_finalized()
  command.build_lib = str(output)
  command.build_temp = str(output / "build")
  command.run()
  shutil.copyfile(source / "__init__.py", output / "msgq/__init__.py")


if __name__ == "__main__":
  parser = argparse.ArgumentParser(description="Build the original msgq Python binding in an isolated test directory")
  parser.add_argument("--output", required=True, type=Path)
  build(parser.parse_args().output.resolve())
