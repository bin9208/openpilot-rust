import argparse
import hashlib
import json
from pathlib import Path
import subprocess


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--json11-prefix', type=Path, required=True)
  parser.add_argument('--libusb-include', type=Path, required=True)
  parser.add_argument('--sanitize', action='store_true')
  parser.add_argument('--compiler', default='clang++')
  args = parser.parse_args()
  root = Path(__file__).resolve().parents[2]
  args.output.mkdir(parents=True, exist_ok=False)
  binary = args.output.resolve() / 'pandad-usb-source'
  sources = [root / 'rust/tools/pandad_usb_source.cc', root / 'openpilot/selfdrive/pandad/panda_comms.cc']
  command = [args.compiler, '-std=c++17', '-O1', '-g', '-ffunction-sections', '-fdata-sections', '-Wl,--gc-sections',
             '-Wl,--wrap=clock_gettime', f'-I{root / "openpilot"}', f'-I{args.json11_prefix / "include"}',
             f'-I{args.libusb_include}', *sources, args.json11_prefix / 'lib/libjson11.a', '-pthread', '-o', binary]
  if args.sanitize:
    command[1:1] = ['-fsanitize=address,undefined', '-fno-omit-frame-pointer']
  commands = []
  shared = args.output.resolve() / 'libpanda_usb_fixture.so'
  shared_command = [args.compiler, '-std=c++17', '-O1', '-g', '-shared', '-fPIC', '-DPANDA_USB_ABI_ONLY',
                    f'-I{root / "openpilot"}', f'-I{args.json11_prefix / "include"}', f'-I{args.libusb_include}', sources[0],
                    args.json11_prefix / 'lib/libjson11.a', '-pthread', '-o', shared]
  if args.sanitize:
    shared_command[1:1] = ['-fsanitize=address,undefined', '-fno-omit-frame-pointer']
  for argv in (command, shared_command):
    result = subprocess.run(list(map(str, argv)), text=True, capture_output=True)
    commands.append({'argv': list(map(str, argv)), 'exit_code': result.returncode, 'stdout': result.stdout, 'stderr': result.stderr})
    (args.output / 'command.json').write_text(json.dumps(commands, indent=2) + '\n')
    result.check_returncode()
  sources += [root / path for path in ('openpilot/selfdrive/pandad/panda_comms.h', 'openpilot/common/swaglog.h', 'openpilot/common/timing.h')]
  sources.append(args.libusb_include / 'libusb-1.0/libusb.h')
  manifest = {'binary': str(binary), 'sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
              'fixture': str(shared), 'fixture_sha256': hashlib.sha256(shared.read_bytes()).hexdigest(),
              'sources': {str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in sources},
              'sanitize': args.sanitize, 'scope': 'unchanged C++ USB lifecycle and retry policies with scripted libusb ABI and clock; no physical USB'}
  (args.output / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
  print(json.dumps(manifest))


if __name__ == '__main__':
  main()
