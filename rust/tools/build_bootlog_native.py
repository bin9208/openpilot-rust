"""Compile unchanged C++ bootlog with only controlled external file-path seams."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[2]


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('output', type=Path)
  parser.add_argument('--identifier', action='store_true')
  parser.add_argument('--capnp-prefix', type=Path, required=True)
  parser.add_argument('--json11-prefix', type=Path, required=True)
  parser.add_argument('--native-prefix', type=Path, required=True)
  parser.add_argument('--zmq-include', type=Path, required=True)
  args = parser.parse_args()
  output = args.output.resolve()
  output.mkdir(parents=True, exist_ok=True)
  schema = output / 'schema'
  generated = output / 'cereal/gen/cpp'
  schema.mkdir()
  generated.mkdir(parents=True)
  for name in ['log', 'custom', 'deprecated']:
    shutil.copyfile(ROOT / f'openpilot/cereal/{name}.capnp', schema / f'{name}.capnp')
  shutil.copyfile(ROOT / 'opendbc_repo/opendbc/car/car.capnp', schema / 'car.capnp')
  shutil.copytree(ROOT / 'openpilot/cereal/include', schema / 'include')
  compiler_env = dict(
    os.environ, LD_LIBRARY_PATH=str(args.capnp_prefix / 'lib/x86_64-linux-gnu'), PATH=str(args.capnp_prefix / 'bin') + ':' + os.environ['PATH']
  )
  subprocess.run(
    [str(args.capnp_prefix / 'bin/capnp'), 'compile', f'-I{schema}', f'--src-prefix={schema}', f'-oc++:{generated}', *map(str, schema.glob('*.capnp'))],
    env=compiler_env,
    check=True,
  )
  sources = [
    'openpilot/system/loggerd/bootlog.cc',
    'openpilot/system/loggerd/logger.cc',
    'openpilot/system/loggerd/zstd_writer.cc',
    'openpilot/common/params.cc',
    'openpilot/common/util.cc',
    'openpilot/common/swaglog.cc',
    'msgq_repo/msgq/ipc.cc',
    'msgq_repo/msgq/event.cc',
    'msgq_repo/msgq/impl_msgq.cc',
    'msgq_repo/msgq/impl_fake.cc',
    'msgq_repo/msgq/msgq.cc',
    'rust/tools/bootlog_paths.cc',
  ]
  if args.identifier:
    sources[0] = 'rust/tools/logger_identifier_source.cc'
  binary = output / ('original-identifier' if args.identifier else 'original-bootlog')
  command = [
    'g++',
    '-std=c++17',
    '-O1',
    '-pthread',
    f'-I{ROOT}',
    f'-I{ROOT / "openpilot"}',
    f'-I{ROOT / "msgq_repo"}',
    f'-I{output}',
    f'-I{generated}',
    f'-I{args.native_prefix / "include"}',
    f'-L{args.native_prefix / "lib/x86_64-linux-gnu"}',
    f'-I{args.json11_prefix / "include"}',
    f'-I{args.zmq_include}',
    f'-I{args.capnp_prefix / "include"}',
    f'-L{args.capnp_prefix / "lib/x86_64-linux-gnu"}',
    f'-Wl,--disable-new-dtags,-rpath,{args.capnp_prefix / "lib/x86_64-linux-gnu"}',
    *[str(ROOT / source) for source in sources],
    *map(str, generated.glob('*.c++')),
    str(args.json11_prefix / 'lib/libjson11.a'),
    '-l:libzmq.so.5',
    '-lzstd',
    '-lcapnp',
    '-lkj',
    '-Wl,--wrap=_ZN4util9read_fileERKNSt7__cxx1112basic_stringIcSt11char_traitsIcESaIcEEE',
    '-Wl,--wrap=_ZN4util17read_files_in_dirERKNSt7__cxx1112basic_stringIcSt11char_traitsIcESaIcEEE',
    '-o',
    str(binary),
  ]
  (output / 'command.json').write_text(json.dumps(command, indent=2))
  with (output / 'build.log').open('w') as log:
    subprocess.run(command, stdout=log, stderr=subprocess.STDOUT, check=True)
  (output / 'source-hashes.json').write_text(json.dumps({source: hashlib.sha256((ROOT / source).read_bytes()).hexdigest() for source in sources}, indent=2))
  print(binary)


if __name__ == '__main__':
  main()
