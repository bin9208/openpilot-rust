import argparse
import hashlib
import json
from pathlib import Path
import subprocess


def main():
  parser = argparse.ArgumentParser(description='Build an owned libusb ABI fixture that never forwards to physical USB.')
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--json11-prefix', type=Path, required=True)
  parser.add_argument('--libusb-include', type=Path, required=True)
  parser.add_argument('--compiler', default='clang++')
  args = parser.parse_args()
  root = Path(__file__).resolve().parents[2]
  args.output.mkdir(parents=True, exist_ok=False)
  binary = args.output.resolve() / 'libusb-1.0.so.0'
  source = root / 'rust/tools/pandad_runtime_usb.cc'
  command = [args.compiler, '-std=c++17', '-O1', '-g', '-shared', '-fPIC', '-Wall', '-Wextra', '-Werror',
             '-Wl,-soname,libusb-1.0.so.0', f'-I{root}', f'-I{args.json11_prefix / "include"}',
             f'-I{args.libusb_include}', str(source), str(args.json11_prefix / 'lib/libjson11.a'), '-pthread', '-o', str(binary)]
  result = subprocess.run(command, text=True, capture_output=True, check=False)
  (args.output / 'command.json').write_text(json.dumps({'argv': command, 'exit_code': result.returncode,
                                                      'stdout': result.stdout, 'stderr': result.stderr}, indent=2) + '\n')
  result.check_returncode()
  sources = [source, root / 'panda/board/health.h', args.libusb_include / 'libusb-1.0/libusb.h']
  manifest = {'result': 'PASS', 'binary': str(binary), 'sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
              'source_sha256': {str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in sources},
              'scope': 'owned virtual libusb device, captured control/bulk calls, no physical USB delegation'}
  (args.output / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
  print(json.dumps(manifest))


if __name__ == '__main__':
  main()
