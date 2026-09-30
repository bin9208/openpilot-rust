#!/usr/bin/env python3
"""Build the unchanged original Params Cython/C++ getter for differential tests."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import sysconfig

from native_logging_build import stage_json11

ROOT = Path(__file__).resolve().parents[2]


def build(output: Path, capnp_prefix: Path | None = None, zmq_include: Path | None = None) -> Path:
  output = output.resolve()
  output.mkdir(parents=True, exist_ok=True)
  install, dependency = stage_json11(ROOT, output / 'dependencies')
  commands = []

  def run(command):
    result = subprocess.run(list(map(str, command)), text=True, capture_output=True)
    commands.append({'argv': list(map(str, command)), 'exit_code': result.returncode, 'stdout': result.stdout, 'stderr': result.stderr})
    (output / 'commands.json').write_text(json.dumps(commands, indent=2) + '\n')
    result.check_returncode()

  schema = output / 'schema'
  generated = output / 'cereal/gen/cpp'
  schema.mkdir(exist_ok=True)
  generated.mkdir(parents=True, exist_ok=True)
  source_files = []
  for name in ['log', 'custom', 'deprecated']:
    path = ROOT / f'openpilot/cereal/{name}.capnp'
    source_files.append(path)
    shutil.copyfile(path, schema / path.name)
  car = ROOT / 'opendbc_repo/opendbc/car/car.capnp'
  source_files.append(car)
  shutil.copyfile(car, schema / 'car.capnp')
  shutil.copytree(ROOT / 'openpilot/cereal/include', schema / 'include', dirs_exist_ok=True)
  run(['capnp', 'compile', f'-I{schema}', f'--src-prefix={schema}', f'-oc++:{generated}', *sorted(schema.glob('*.capnp'))])
  cython_source = output / 'params_pyx.cpp'
  pyx = ROOT / 'openpilot/common/params_pyx.pyx'
  run([sys.executable, '-m', 'cython', '--cplus', '-3', '--module-name', 'openpilot.common.params_pyx', '-o', cython_source, pyx])
  target = output / ('params_pyx' + sysconfig.get_config_var('EXT_SUFFIX'))
  command = [
    os.environ.get('CXX', 'c++'),
    '-std=c++17',
    '-O2',
    '-g',
    '-shared',
    '-fPIC',
    '-pthread',
    f'-I{ROOT / "openpilot"}',
    f'-I{output}',
    f'-I{generated}',
    f'-I{install / "include"}',
    f'-I{sysconfig.get_paths()["include"]}',
  ]
  if capnp_prefix is not None:
    command.append(f'-I{capnp_prefix / "include"}')
  if zmq_include is None:
    candidates = sorted((ROOT / 'rust/target/debug/build').glob('zmq-sys-*/out/source/include/zmq.h'))
    if candidates:
      zmq_include = candidates[0].parent
  if zmq_include is not None:
    command.append(f'-I{zmq_include}')
  implementations = [ROOT / f'openpilot/common/{name}.cc' for name in ['params', 'util', 'swaglog']]
  command += [cython_source, *implementations, install / 'lib/libjson11.a', '-l:libzmq.so.5', '-o', target]
  run(command)
  source_files += [pyx, *implementations]
  source_files += [ROOT / f'openpilot/common/{name}.h' for name in ['params', 'params_keys', 'util', 'swaglog', 'timing', 'version', 'queue']]
  source_files += [ROOT / f'openpilot/system/hardware/{name}' for name in ['hw.h', 'base.h', 'pc/hardware.h']]
  provenance = {
    'result': 'pass',
    'module': str(target),
    'module_sha256': hashlib.sha256(target.read_bytes()).hexdigest(),
    'source_commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
    'sources': {str(path.relative_to(ROOT)): hashlib.sha256(path.read_bytes()).hexdigest() for path in source_files},
    **dependency,
    'source_changes': 'None. Original Cython, Params, util, C++ logging and PC hardware headers are compiled unchanged.',
    'python': sys.version,
    'commands': str(output / 'commands.json'),
  }
  (output / 'provenance.json').write_text(json.dumps(provenance, indent=2) + '\n')
  return target


if __name__ == '__main__':
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--capnp-prefix', type=Path)
  parser.add_argument('--zmq-include', type=Path)
  args = parser.parse_args()
  print(build(args.output, args.capnp_prefix, args.zmq_include))
