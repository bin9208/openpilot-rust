import argparse
import hashlib
import json
from pathlib import Path
import runpy
import subprocess


def main():
  parser = argparse.ArgumentParser(description='Compile unchanged original pandad against an owned libusb fixture.')
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--generated-root', type=Path, required=True)
  parser.add_argument('--capnp-prefix', type=Path, required=True)
  parser.add_argument('--json11-prefix', type=Path, required=True)
  parser.add_argument('--libusb-include', type=Path, required=True)
  parser.add_argument('--zmq-root', type=Path, required=True)
  parser.add_argument('--fixture', type=Path, required=True)
  parser.add_argument('--sanitize', action='store_true')
  args = parser.parse_args()
  root = Path(__file__).resolve().parents[2]
  output = args.output.resolve()
  output.mkdir(parents=True, exist_ok=False)
  (output / 'cereal').mkdir()
  service_source = root / 'openpilot/cereal/services.py'
  (output / 'cereal/services.h').write_text(runpy.run_path(str(service_source))['build_header']())
  source_names = [
    'openpilot/selfdrive/pandad/main.cc', 'openpilot/selfdrive/pandad/pandad.cc',
    'openpilot/selfdrive/pandad/panda.cc', 'openpilot/selfdrive/pandad/panda_safety.cc',
    'openpilot/selfdrive/pandad/panda_comms.cc', 'openpilot/selfdrive/pandad/spi.cc',
    'openpilot/common/params.cc', 'openpilot/common/util.cc', 'openpilot/common/ratekeeper.cc',
    'openpilot/common/swaglog.cc', 'openpilot/cereal/messaging/socketmaster.cc',
    'msgq_repo/msgq/ipc.cc', 'msgq_repo/msgq/event.cc', 'msgq_repo/msgq/impl_msgq.cc',
    'msgq_repo/msgq/impl_fake.cc', 'msgq_repo/msgq/msgq.cc',
  ]
  sources = [root / name for name in source_names]
  generated = args.generated_root / 'cereal/gen/cpp'
  generated_sources = sorted(generated.glob('*.c++'))
  assert len(generated_sources) == 4
  binary = output / 'pandad-source'
  library = args.capnp_prefix / 'lib/x86_64-linux-gnu'
  command = ['clang++', '-std=c++17', '-O1', '-g', '-ffunction-sections', '-fdata-sections', '-Wl,--gc-sections',
             f'-I{root}', f'-I{root / "openpilot"}', f'-I{root / "msgq_repo"}', f'-I{output}', f'-I{args.generated_root}', f'-I{generated}',
             f'-I{args.capnp_prefix / "include"}', f'-I{args.json11_prefix / "include"}', f'-I{args.libusb_include}',
             f'-I{args.zmq_root / "source/include"}', *map(str, sources), *map(str, generated_sources),
             str(args.json11_prefix / 'lib/libjson11.a'), str(args.zmq_root / 'lib/libzmq.a'),
             f'-L{args.fixture.parent}', f'-Wl,-rpath,{args.fixture.parent}', '-l:libusb-1.0.so.0',
             f'-L{library}', f'-Wl,-rpath,{library}', '-lcapnp', '-lkj', '-pthread', '-ldl', '-o', str(binary)]
  if args.sanitize:
    command[1:1] = ['-fsanitize=address,undefined', '-fno-omit-frame-pointer']
  result = subprocess.run(command, text=True, capture_output=True, check=False)
  (output / 'command.json').write_text(json.dumps({'argv': command, 'exit_code': result.returncode,
                                                'stdout': result.stdout, 'stderr': result.stderr}, indent=2) + '\n')
  result.check_returncode()
  headers = [root / name for name in (
    'openpilot/selfdrive/pandad/panda.h', 'openpilot/selfdrive/pandad/pandad.h', 'openpilot/selfdrive/pandad/panda_comms.h',
    'openpilot/selfdrive/pandad/spi_alert.h', 'openpilot/common/params.h', 'openpilot/common/params_keys.h',
    'openpilot/common/ratekeeper.h', 'openpilot/common/util.h', 'openpilot/common/swaglog.h',
    'openpilot/cereal/messaging/messaging.h', 'openpilot/system/hardware/pc/hardware.h',
  )]
  manifest = {'result': 'PASS', 'binary': str(binary), 'sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
              'source_sha256': {str(path): hashlib.sha256(path.read_bytes()).hexdigest()
                                for path in sources + generated_sources + headers + [service_source]},
              'fixture_sha256': hashlib.sha256(args.fixture.read_bytes()).hexdigest(), 'sanitize': args.sanitize,
              'scope': 'unchanged C++ pandad/main/state/safety/peripheral/serial/CAN loops and msgq, fixture USB, host hardware path'}
  (output / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
  print(json.dumps(manifest))


if __name__ == '__main__':
  main()
