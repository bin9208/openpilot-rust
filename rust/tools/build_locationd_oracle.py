#!/usr/bin/env python3
import argparse
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
  args = parser.parse_args()
  root = Path(__file__).resolve().parents[2]
  output = args.output.resolve()
  output.mkdir(parents=True, exist_ok=True)
  environment = os.environ | {'PYTHONPATH': str(root) + os.pathsep + os.environ.get('PYTHONPATH', '')}
  commands = [
    [sys.executable, str(root / 'openpilot/selfdrive/locationd/models/pose_kf.py'), 'pose', str(output)],
    [sys.executable, '-m', 'cython', '--cplus', '-3', str(root / 'rednose_repo/rednose/helpers/ekf_sym_pyx.pyx'), '-o', str(output / 'ekf_sym_pyx.cpp')],
    [
      'clang++',
      '-std=c++17',
      '-O2',
      '-shared',
      '-fPIC',
      '-I' + str(root / 'rednose_repo'),
      '-I' + str(args.eigen_include),
      str(output / 'pose.cpp'),
      '-o',
      str(output / 'libpose.so'),
    ],
    [
      'clang++',
      '-std=c++17',
      '-O1',
      '-shared',
      '-fPIC',
      '-I' + str(root / 'rednose_repo'),
      '-I' + str(root / 'rednose_repo/rednose'),
      '-I' + str(args.eigen_include),
      '-I' + sysconfig.get_paths()['include'],
      '-I' + np.get_include(),
      str(output / 'ekf_sym_pyx.cpp'),
      str(root / 'rednose_repo/rednose/helpers/ekf_sym.cc'),
      str(root / 'rednose_repo/rednose/helpers/ekf_load.cc'),
      '-ldl',
      '-o',
      str(output / ('ekf_sym_pyx' + sysconfig.get_config_var('EXT_SUFFIX'))),
    ],
  ]
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
  print('Built unchanged source PoseKalman model and actual rednose Cython oracle:', output)


if __name__ == '__main__':
  main()
