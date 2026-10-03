import argparse
from hashlib import sha256
import json
from pathlib import Path
import subprocess
import sysconfig


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--json11-prefix', type=Path, required=True)
  parser.add_argument('--spidev-source', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  root = Path(__file__).resolve().parents[2]
  fixture = args.output / 'libpandad_firmware_spi.so'
  binding = args.output / ('spidev' + sysconfig.get_config_var('EXT_SUFFIX'))
  source = root / 'rust/tools/pandad_firmware_spidev.cc'
  commands = [
    ['clang++', '-std=c++17', '-O1', '-g', '-shared', '-fPIC', '-Wall', '-Wextra', '-Werror',
     f'-I{args.json11_prefix / "include"}', str(source), str(args.json11_prefix / 'lib/libjson11.a'), '-ldl', '-o', str(fixture)],
    ['cc', '-O1', '-g', '-shared', '-fPIC', f'-I{sysconfig.get_paths()["include"]}', str(args.spidev_source / 'spidev_module.c'), '-o', str(binding)],
  ]
  logs = []
  for command in commands:
    result = subprocess.run(command, text=True, capture_output=True)
    logs.append({'argv': command, 'exit_code': result.returncode, 'stdout': result.stdout, 'stderr': result.stderr})
    (args.output / 'commands.json').write_text(json.dumps(logs, indent=2) + '\n')
    result.check_returncode()
  report = {'result': 'PASS', 'fixture': str(fixture.resolve()), 'binding': str(binding.resolve()),
            'sha256': {str(path): sha256(path.read_bytes()).hexdigest() for path in
                       (source, args.spidev_source / 'spidev_module.c', args.spidev_source / 'LICENSE', fixture, binding)},
            'limits': 'Owned spidev syscall fixture backed by /dev/null; original spidev 3.8 binding; no physical SPI.'}
  (args.output / 'manifest.json').write_text(json.dumps(report, indent=2) + '\n')
  print(json.dumps(report))


if __name__ == '__main__':
  main()
