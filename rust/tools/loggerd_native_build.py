"""Build unmodified original loggerd with host-only diagnostic output."""
from __future__ import annotations

import argparse
from hashlib import sha256
import json
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[2]


def build(output: Path, capnp_prefix: Path | None, native_prefix: Path | None) -> Path:
  output.mkdir(parents=True, exist_ok=True)
  schema = output / 'schema'
  generated = output / 'cereal/gen/cpp'
  schema.mkdir(exist_ok=True)
  generated.mkdir(parents=True, exist_ok=True)
  for name in ('log', 'custom', 'deprecated'):
    shutil.copyfile(ROOT / f'openpilot/cereal/{name}.capnp', schema / f'{name}.capnp')
  shutil.copyfile(ROOT / 'opendbc_repo/opendbc/car/car.capnp', schema / 'car.capnp')
  shutil.copytree(ROOT / 'openpilot/cereal/include', schema / 'include', dirs_exist_ok=True)
  subprocess.run(['capnp', 'compile', f'-I{schema}', f'--src-prefix={schema}', f'-oc++:{generated}',
                  *map(str, schema.glob('*.capnp'))], check=True)
  with (output / 'cereal/services.h').open('wb') as target:
    subprocess.run(['python3', str(ROOT / 'openpilot/cereal/services.py')], stdout=target, check=True)
  sources = [
    'openpilot/system/loggerd/loggerd.cc', 'openpilot/system/loggerd/logger.cc',
    'openpilot/system/loggerd/zstd_writer.cc', 'openpilot/system/loggerd/video_writer.cc',
    'openpilot/common/params.cc', 'openpilot/common/util.cc',
    'msgq_repo/msgq/ipc.cc', 'msgq_repo/msgq/event.cc', 'msgq_repo/msgq/impl_msgq.cc',
    'msgq_repo/msgq/impl_fake.cc', 'msgq_repo/msgq/msgq.cc', 'rust/tools/loggerd_native_log.cc',
  ]
  command = ['g++', '-std=c++17', '-O1', '-pthread', f'-I{ROOT}', f'-I{ROOT / "openpilot"}',
             f'-I{ROOT / "msgq_repo"}', f'-I{output}', f'-I{generated}']
  if capnp_prefix:
    command += [f'-I{capnp_prefix / "include"}', f'-L{capnp_prefix / "lib/x86_64-linux-gnu"}',
                f'-Wl,--disable-new-dtags,-rpath,{capnp_prefix / "lib/x86_64-linux-gnu"}']
  if native_prefix:
    command += [f'-I{native_prefix / "include"}', f'-I{native_prefix / "include/x86_64-linux-gnu"}',
                f'-L{native_prefix / "lib/x86_64-linux-gnu"}']
  binary = output / 'original-loggerd'
  command += [str(ROOT / name) for name in sources]
  command += [str(path) for path in generated.glob('*.c++')]
  command += ['-lavformat', '-lavcodec', '-lavutil', '-lzstd', '-lcapnp', '-lkj', '-o', str(binary)]
  (output / 'build-command.json').write_text(json.dumps(command, indent=2) + '\n')
  with (output / 'build.log').open('wb') as capture:
    subprocess.run(command, stdout=capture, stderr=subprocess.STDOUT, check=True)
  producer_sources = [
    'openpilot/system/loggerd/encoder/encoder.cc', 'openpilot/system/loggerd/encoder/ffmpeg_encoder.cc',
    'openpilot/cereal/messaging/socketmaster.cc', 'rust/tools/loggerd_native_producer.cc',
  ]
  producer_command = [part for part in command[:-2] if part != str(ROOT / 'openpilot/system/loggerd/loggerd.cc') and not part.startswith('-l')]
  producer_command += [str(ROOT / name) for name in producer_sources]
  producer_command += ['-lyuv', '-lavformat', '-lavcodec', '-lavutil', '-lzstd', '-lcapnp', '-lkj', '-o', str(output / 'original-encoder-producer')]
  with (output / 'producer-build.log').open('wb') as capture:
    subprocess.run(producer_command, stdout=capture, stderr=subprocess.STDOUT, check=True)
  (output / 'producer-build-command.json').write_text(json.dumps(producer_command, indent=2) + '\n')
  sources += producer_sources
  (output / 'source-hashes.json').write_text(json.dumps({name: sha256((ROOT / name).read_bytes()).hexdigest() for name in sources}, indent=2) + '\n')
  return binary


if __name__ == '__main__':
  parser = argparse.ArgumentParser()
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--capnp-prefix', type=Path)
  parser.add_argument('--native-prefix', type=Path)
  arguments = parser.parse_args()
  print(build(arguments.output.resolve(), arguments.capnp_prefix, arguments.native_prefix))
