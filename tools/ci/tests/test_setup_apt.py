"""Keep host setup independent of foreign architectures on persistent runners."""

import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[3]
SETUP = ROOT / 'tools/setup_dependencies.sh'


class NativeAptSetupTests(unittest.TestCase):
  def run_setup(self, arch='amd64', sudo=False, update_code=0):
    source = SETUP.read_text()
    function = 'function install_native_apt_deps() (' + source.split(
      'function install_native_apt_deps() (', 1)[1].split('\n)\n', 1)[0] + '\n)\n'
    with tempfile.TemporaryDirectory(prefix='setup-apt-') as temporary:
      root = Path(temporary)
      bindir = root / 'bin'
      bindir.mkdir()
      foreign_config = root / 'foreign.conf'
      foreign_contents = ('APT::Architecture "amd64";\n'
                          'APT::Architectures { "amd64"; "arm64"; "i386"; };\n')
      foreign_config.write_text(foreign_contents)
      commands = {
        'dpkg': '#!/bin/sh\n[ "$1" = --print-architecture ] || exit 91\nprintf "%s\\n" "$TEST_ARCH"\n',
        'sudo': '#!/bin/sh\nexec "$@"\n',
        'apt-get': '''#!/usr/bin/env python3
import json, os, subprocess, sys
from pathlib import Path
args = sys.argv[1:]
config = args[args.index('--config-file') + 1]
dump = subprocess.run(['apt-config', '--config-file', config, 'dump'],
                      capture_output=True, text=True, check=True).stdout
with Path(os.environ['TEST_RECORD']).open('a') as out:
  out.write(json.dumps({'args': args, 'config': config, 'dump': dump}) + '\\n')
if 'update' in args:
  sys.exit(int(os.environ['TEST_UPDATE_CODE']))
''',
      }
      for name, contents in commands.items():
        path = bindir / name
        path.write_text(contents)
        path.chmod(0o755)
      record = root / 'calls.jsonl'
      environment = {**os.environ, 'PATH': f'{bindir}:{os.environ["PATH"]}',
                     'TMPDIR': str(root), 'APT_CONFIG': str(foreign_config),
                     'TEST_ARCH': arch, 'TEST_RECORD': str(record),
                     'TEST_UPDATE_CODE': str(update_code), 'SUDO': 'sudo' if sudo else ''}
      result = subprocess.run(['bash', '-eu', '-c', function + '\ninstall_native_apt_deps build-essential git'],
                              env=environment, capture_output=True, text=True)
      calls = [json.loads(line) for line in record.read_text().splitlines()] if record.exists() else []
      for call in calls:
        self.assertFalse(Path(call['config']).exists(), 'per-invocation config must be removed')
      self.assertEqual(list(root.glob('openpilot-apt.*')), [])
      self.assertEqual(foreign_config.read_text(), foreign_contents)
      return result, calls

  def test_native_userspace_overrides_foreign_list_for_update_and_install(self):
    for arch, sudo in [('amd64', True), ('arm64', False)]:
      with self.subTest(arch=arch, sudo=sudo):
        result, calls = self.run_setup(arch, sudo)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(len(calls), 2)
        self.assertEqual(calls[0]['args'][2:], ['update'])
        self.assertEqual(calls[1]['args'][2:],
                         ['install', '-y', '--no-install-recommends', 'build-essential', 'git'])
        for call in calls:
          architectures = [line for line in call['dump'].splitlines() if line.startswith('APT::Architectures:: ')]
          self.assertEqual(architectures, [f'APT::Architectures:: "{arch}";'])
          self.assertIn(f'APT::Architecture "{arch}";', call['dump'])
          self.assertIn('APT::Get::List-Cleanup "false";', call['dump'])

  def test_failed_update_stops_install_and_removes_temporary_config(self):
    result, calls = self.run_setup(update_code=74)
    self.assertEqual(result.returncode, 74, result.stderr)
    self.assertEqual(len(calls), 1)
    self.assertEqual(calls[0]['args'][2:], ['update'])

  def test_invalid_architecture_never_reaches_apt(self):
    result, calls = self.run_setup(arch='amd64"; malicious')
    self.assertNotEqual(result.returncode, 0)
    self.assertEqual(calls, [])


if __name__ == '__main__':
  unittest.main()
