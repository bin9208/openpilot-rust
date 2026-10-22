"""Actions tests for native APT configuration; no install or shared-state writes."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / 'apt-native'


class NativeAptTests(unittest.TestCase):
  def setUp(self):
    temporary = tempfile.TemporaryDirectory()
    self.addCleanup(temporary.cleanup)
    self.root = Path(temporary.name)
    self.bin = self.root / 'bin'
    self.bin.mkdir()
    self.work = self.root / 'job temp'
    self.work.mkdir()
    self.log = self.root / 'calls.json'
    self.foreign = self.root / 'foreign.conf'
    self.foreign.write_text('APT::Architecture "amd64";\nAPT::Architectures { "amd64"; "arm64"; };\n')
    self.write_tool('dpkg', """import os,sys
assert sys.argv[1:] == ['--print-architecture'], sys.argv
print(os.environ['TEST_NATIVE_ARCH'])
""")
    self.write_tool('sudo', """import json,os,re,subprocess,sys
from pathlib import Path
args=sys.argv[1:]
assert args[:3] == ['-n','apt-get','-c'], args
config=Path(args[3])
dump=subprocess.run(['/usr/bin/apt-config','-c',str(config),'dump'],text=True,capture_output=True,check=True)
architectures=re.findall(r'^APT::Architectures:: "([^"]+)";$',dump.stdout,re.M)
Path(os.environ['TEST_APT_LOG']).write_text(json.dumps({
 'arguments':args[4:], 'config_path':str(config), 'config':config.read_text(),
 'architectures':architectures}))
sys.exit(int(os.environ.get('TEST_APT_EXIT','0')))
""")
    self.env = dict(os.environ, GITHUB_ACTIONS='true', RUNNER_TEMP=str(self.work),
                    PATH=str(self.bin) + os.pathsep + os.environ['PATH'],
                    APT_CONFIG=str(self.foreign), TEST_NATIVE_ARCH='amd64', TEST_APT_LOG=str(self.log))

  def write_tool(self, name, code):
    path = self.bin / name
    path.write_text('#!' + sys.executable + '\n' + code)
    path.chmod(0o755)

  def invoke(self, *arguments):
    return subprocess.run(['bash', str(SCRIPT), *arguments], env=self.env, cwd=self.root,
                          text=True, capture_output=True, timeout=10)

  def test_real_apt_config_clears_foreign_defaults_and_preserves_arguments(self):
    self.root.joinpath('libavcodec-dev=7:6.1.1-matched-file').touch()
    result = self.invoke('install', '-y', 'g++-aarch64-linux-gnu', 'libavcodec-dev=7:6.1.1-*')
    self.assertEqual(result.returncode, 0, result.stderr)
    call = json.loads(self.log.read_text())
    self.assertEqual(call['architectures'], ['amd64'])
    self.assertEqual(call['arguments'], ['install', '-y', 'g++-aarch64-linux-gnu', 'libavcodec-dev=7:6.1.1-*'])
    self.assertIn('#clear APT::Architectures;', call['config'])
    self.assertIn('APT::Get::List-Cleanup "false";', call['config'])
    self.assertFalse(Path(call['config_path']).exists())
    self.assertEqual(self.foreign.read_text(), 'APT::Architecture "amd64";\nAPT::Architectures { "amd64"; "arm64"; };\n')

  def test_arm64_package_context_remains_arm64(self):
    self.env['TEST_NATIVE_ARCH'] = 'arm64'
    result = self.invoke('update')
    self.assertEqual(result.returncode, 0, result.stderr)
    self.assertEqual(json.loads(self.log.read_text())['architectures'], ['arm64'])
    self.assertFalse(list(self.work.glob('apt-native.*')))

  def test_apt_failure_is_returned_and_private_configuration_is_removed(self):
    self.env['TEST_APT_EXIT'] = '73'
    result = self.invoke('update')
    self.assertEqual(result.returncode, 73)
    self.assertFalse(list(self.work.glob('apt-native.*')))

  def test_non_actions_unsupported_architecture_and_operation_fail_before_apt(self):
    for change, arguments in (({'GITHUB_ACTIONS': 'false'}, ['update']),
                              ({'TEST_NATIVE_ARCH': 'riscv64'}, ['update']),
                              ({}, ['purge', 'anything'])):
      with self.subTest(change=change, arguments=arguments):
        env = self.env | change
        result = subprocess.run(['bash', str(SCRIPT), *arguments], env=env, cwd=self.root,
                                text=True, capture_output=True, timeout=10)
        self.assertEqual(result.returncode, 2)
        self.assertFalse(self.log.exists())
        self.assertFalse(list(self.work.glob('apt-native.*')))


if __name__ == '__main__':
  unittest.main()
