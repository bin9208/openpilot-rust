import argparse
import hashlib
import json
from pathlib import Path
import subprocess


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--json11-prefix', type=Path, required=True)
  parser.add_argument('--sanitize', action='store_true')
  parser.add_argument('--compiler', default='clang++')
  args = parser.parse_args()
  root = Path(__file__).resolve().parents[2]
  args.output.mkdir(parents=True, exist_ok=False)
  binary = args.output.resolve() / 'pandad-spi-source'
  sources = [root / name for name in ('rust/tools/pandad_spi_source.cc', 'openpilot/selfdrive/pandad/spi.cc', 'openpilot/common/util.cc')]
  command = [args.compiler, '-std=c++17', '-O1', '-g', '-ffunction-sections', '-fdata-sections', '-Wl,--gc-sections',
             f'-I{root}', f'-I{root / "openpilot"}', f'-I{args.json11_prefix / "include"}', *sources,
             args.json11_prefix / 'lib/libjson11.a', '-pthread', '-o', binary]
  command += [f'-Wl,--wrap={name}' for name in ('clock_gettime', 'open', 'stat', 'close', 'flock', 'ioctl', 'sched_yield', 'usleep')]
  if args.sanitize:
    command[1:1] = ['-fsanitize=address,undefined', '-fno-omit-frame-pointer']
  result = subprocess.run(list(map(str, command)), text=True, capture_output=True)
  (args.output / 'command.json').write_text(json.dumps({'argv': list(map(str, command)), 'exit_code': result.returncode,
                                                      'stdout': result.stdout, 'stderr': result.stderr}, indent=2) + '\n')
  result.check_returncode()
  sources += [root / name for name in ('openpilot/selfdrive/pandad/panda_comms.h', 'openpilot/common/util.h',
                                     'openpilot/common/timing.h', 'openpilot/common/swaglog.h', 'panda/board/comms_definitions.h')]
  manifest = {'binary': str(binary), 'sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
              'sources': {str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in sources},
              'sanitize': args.sanitize, 'scope': 'unchanged SPI and utility implementation; scripted Linux syscalls only; no hardware access'}
  (args.output / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
  print(json.dumps(manifest))


if __name__ == '__main__':
  main()
