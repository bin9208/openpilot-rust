#!/usr/bin/env python3
import argparse
import json
from pathlib import Path
import shutil
import socket
import subprocess

from check_cweb_policy import module

ROOT = Path(__file__).resolve().parents[2]
CRATE = ROOT / 'rust/crates/cweb-push'


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output = args.output.resolve()
  args.output.mkdir(parents=True, exist_ok=False)
  assert shutil.disk_usage(args.output).free >= 26 * 1024**3
  flags = ['clang++', '-std=c++17', '-fsanitize=address,undefined', '-fno-omit-frame-pointer', '-g', '-I' + str(CRATE)]
  object_file = args.output / 'address.o'
  build = [*flags, '-Dsocket=fixture_socket', '-Dioctl=fixture_ioctl', '-Dclose=fixture_close',
           '-c', str(CRATE / 'native/address_io.cc'), '-o', str(object_file)]
  subprocess.run(build, check=True)
  executable = args.output / 'address-sanitized'
  subprocess.run([*flags, str(CRATE / 'native/address_test.cc'), str(object_file), '-o', str(executable)], check=True)
  subprocess.run([executable], check=True)
  names = ['lo', 'cweb-nonexistent-interface', '', '한글없는인터페이스']
  source = module()
  source['socket'] = socket
  expected = [source['_iface_ipv4'](name) for name in names]
  actual = json.loads(subprocess.check_output([args.binary.resolve(), *names]))
  assert actual == expected, (actual, expected)
  result = {'source_native_equal': True, 'read_only_host_interfaces': names, 'values': actual, 'ioctl_ownership_asan_ubsan': True}
  (args.output / 'result.json').write_text(json.dumps(result, ensure_ascii=False, indent=2))
  print(json.dumps(result, ensure_ascii=False))


if __name__ == '__main__':
  main()
