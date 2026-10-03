#!/usr/bin/env python3
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import sysconfig

import numpy as np


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--eigen-include', type=Path, required=True)
  parser.add_argument('--reuse-wrapper', type=Path)
  args = parser.parse_args()
  root = Path(__file__).resolve().parents[2]
  output = args.output.resolve()
  output.mkdir(parents=True, exist_ok=True)
  wrapper = output / ('ekf_sym_pyx' + sysconfig.get_config_var('EXT_SUFFIX'))
  commands = [[sys.executable, str(root / 'openpilot/selfdrive/locationd/models/car_kf.py'), 'car', str(output)],
              ['clang++', '-std=c++17', '-O2', '-shared', '-fPIC', '-I' + str(root / 'rednose_repo'), '-I' + str(args.eigen_include),
               str(output / 'car.cpp'), '-o', str(output / 'libcar.so')]]
  if args.reuse_wrapper is not None:
    if shutil.disk_usage(output).free < 26 * 1024**3:
      raise RuntimeError('25 GiB floor plus 1 GiB growth unavailable')
    wrapper.symlink_to(args.reuse_wrapper.resolve())
    (output / 'reused-wrapper.json').write_text(json.dumps({'path': str(args.reuse_wrapper.resolve()),
      'sha256': hashlib.sha256(args.reuse_wrapper.read_bytes()).hexdigest()}, indent=2) + '\n')
  else:
    commands += [
      [sys.executable, '-m', 'cython', '--cplus', '-3', str(root / 'rednose_repo/rednose/helpers/ekf_sym_pyx.pyx'), '-o', str(output / 'ekf_sym_pyx.cpp')],
      ['clang++', '-std=c++17', '-O1', '-shared', '-fPIC', '-I' + str(root / 'rednose_repo'), '-I' + str(root / 'rednose_repo/rednose'),
       '-I' + str(args.eigen_include), '-I' + sysconfig.get_paths()['include'], '-I' + np.get_include(), str(output / 'ekf_sym_pyx.cpp'),
       str(root / 'rednose_repo/rednose/helpers/ekf_sym.cc'), str(root / 'rednose_repo/rednose/helpers/ekf_load.cc'), '-ldl', '-o', str(wrapper)],
    ]
  environment = os.environ | {'PYTHONPATH': str(root) + os.pathsep + os.environ.get('PYTHONPATH', '')}
  with (output / 'build.log').open('w') as log:
    for command in commands:
      free = shutil.disk_usage(output).free
      if free < 26 * 1024**3:
        raise RuntimeError('25 GiB floor plus 1 GiB build estimate unavailable')
      log.write(json.dumps({'command': command, 'free_bytes': free}) + '\n')
      log.flush()
      result = subprocess.run(command, cwd=root, env=environment, stdout=log, stderr=subprocess.STDOUT)
      log.write(f'EXIT {result.returncode}\n')
      log.flush()
      result.check_returncode()
  print('Built unchanged CarKalman oracle:', output)


if __name__ == '__main__':
  main()
