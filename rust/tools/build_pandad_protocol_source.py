import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[2]


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--capnp-prefix', type=Path, required=True)
  parser.add_argument('--json11-prefix', type=Path, required=True)
  parser.add_argument('--sanitize', action='store_true')
  parser.add_argument('--safety', action='store_true')
  args = parser.parse_args()
  output = args.output.resolve()
  output.mkdir(parents=True, exist_ok=False)
  schema = output / 'schema'
  generated = output / 'cereal/gen/cpp'
  schema.mkdir()
  generated.mkdir(parents=True)
  sources = [ROOT / 'rust/tools/pandad_protocol_source.cc', ROOT / 'openpilot/selfdrive/pandad/panda.cc',
             ROOT / 'openpilot/selfdrive/pandad/panda.h', ROOT / 'openpilot/selfdrive/pandad/spi_alert.h']
  extra = []
  if args.safety:
    sources[0] = ROOT / 'rust/tools/pandad_safety_source.cc'
    extra = [ROOT / path for path in ('openpilot/selfdrive/pandad/panda_safety.cc', 'openpilot/common/params.cc', 'openpilot/common/util.cc')]
    sources += extra + [ROOT / path for path in ('openpilot/selfdrive/pandad/pandad.h', 'openpilot/common/params.h', 'openpilot/common/params_keys.h',
                                               'openpilot/cereal/messaging/messaging.h')]
  sources += [ROOT / name for name in ('openpilot/selfdrive/pandad/panda_comms.h', 'panda/board/health.h', 'panda/board/can.h',
                                      'openpilot/common/swaglog.h', 'openpilot/common/timing.h', 'openpilot/common/util.h')]
  for name in ('log', 'custom', 'deprecated'):
    path = ROOT / f'openpilot/cereal/{name}.capnp'
    shutil.copyfile(path, schema / path.name)
    sources.append(path)
  car = ROOT / 'opendbc_repo/opendbc/car/car.capnp'
  shutil.copyfile(car, schema / 'car.capnp')
  sources.append(car)
  shutil.copytree(ROOT / 'openpilot/cereal/include', schema / 'include')
  commands = []

  def run(command):
    command = list(map(str, command))
    result = subprocess.run(command, text=True, capture_output=True)
    commands.append({'argv': command, 'exit_code': result.returncode, 'stdout': result.stdout, 'stderr': result.stderr})
    (output / 'commands.json').write_text(json.dumps(commands, indent=2) + '\n')
    result.check_returncode()

  run(['capnp', 'compile', f'-I{schema}', f'--src-prefix={schema}', f'-oc++:{generated}', *sorted(schema.glob('*.capnp'))])
  library = args.capnp_prefix / 'lib/x86_64-linux-gnu'
  binary = output / 'pandad-protocol-source'
  command = ['clang++', '-std=c++17', '-O1', '-g', '-ffunction-sections', '-fdata-sections', '-Wl,--gc-sections',
             f'-I{ROOT}', f'-I{ROOT / "openpilot"}', f'-I{ROOT / "msgq_repo"}', f'-I{output}', f'-I{generated}',
             f'-I{args.capnp_prefix / "include"}', f'-I{args.json11_prefix / "include"}',
             sources[0], sources[1], *extra, *sorted(generated.glob('*.c++')), args.json11_prefix / 'lib/libjson11.a',
             f'-L{library}', f'-Wl,-rpath,{library}', '-lcapnp', '-lkj', '-pthread', '-o', binary]
  if args.sanitize:
    command[1:1] = ['-fsanitize=address,undefined', '-fno-omit-frame-pointer']
  run(command)
  report = {'binary': str(binary), 'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
            'sources': {str(path.relative_to(ROOT)): hashlib.sha256(path.read_bytes()).hexdigest() for path in sources},
            'scope': ('unchanged PandaSafety, native Params and control command methods; fixture transport and logging sinks' if args.safety else
                      'unchanged original CAN pack/unpack and SPI alert methods; fixture only replaces transport reset and logging sinks'),
            'sanitize': args.sanitize}
  (output / 'manifest.json').write_text(json.dumps(report, indent=2) + '\n')
  print(json.dumps(report))


if __name__ == '__main__':
  main()
