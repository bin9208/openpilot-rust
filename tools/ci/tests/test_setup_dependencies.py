"""Native bootstrap must not inherit a persistent runner's foreign apt targets."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[3]
SETUP = (ROOT / 'tools/setup_dependencies.sh').read_text()
FUNCTIONS = SETUP[SETUP.index('function retry()'):].split('# --- Main ---', 1)[0]
PACKAGES = '''ca-certificates build-essential curl libcurl4-openssl-dev locales git
qtbase5-dev libgles-dev libqt5charts5-dev libqt5svg5-dev libqt5websockets5-dev
libqt5serialport5-dev libqt5opengl5-dev libqt5x11extras5-dev qtwayland5
libprotoc-dev libzmq3-dev liblz4-dev libzstd-dev python3-dev'''.split()

MOCKS = r'''
id() { printf '0\n'; }
is_ubuntu_pc() { return 0; }
dpkg() {
  # Any attempt to add/remove architectures is a regression.
  [[ "$*" == --print-architecture ]] || return 99
  printf '%s\n' "$NATIVE_ARCH"
}
dpkg-query() {
  local package="${@: -1}"
  printf '%s\n' "$package" >> "$QUERY_LOG"
  if [[ "$INSTALLED" == native && "$package" == *":$NATIVE_ARCH" ]] ||
     [[ "$INSTALLED" == foreign && "$package" != *":$NATIVE_ARCH" ]]; then
    printf 'install ok installed\n'
  else
    return 1
  fi
}
apt-get() { "$TEST_PYTHON" "$APT_MOCK" "$@"; }
# Do not touch host udev rules, even if the test runs as root in Actions.
tee() { cat > /dev/null; }
rm() { :; }
udevadm() { :; }
sudo() { return 99; }
gcc() { :; }
g++() { :; }
make() { :; }
curl() { :; }
curl-config() { :; }
git() { :; }
install_linux_deps
'''

APT_MOCK = '''import json, os, sys
from pathlib import Path
args = sys.argv[1:]
with open(os.environ['APT_LOG'], 'a') as file:
    file.write(json.dumps(args) + '\\n')
# apt 2.8.1: apt-pkg/contrib/configuration.cc FindVector prefers the scalar;
# apt-pkg/aptconfiguration.cc getArchitectures uses dpkg only for an empty vector.
# https://github.com/Debian/apt/blob/2.8.1/apt-pkg/contrib/configuration.cc#L399-L417
registered = [os.environ['NATIVE_ARCH'], os.environ['FOREIGN_ARCH']]
scalar = ','.join(registered) if os.environ['APT_INHERITANCE'] == 'scalar' else ''
children = registered.copy() if os.environ['APT_INHERITANCE'] == 'list' else []
while args[:1] == ['-o']:
    option = args[1]
    if option.startswith('APT::Architectures='):
        scalar = option.split('=', 1)[1]
    elif option.startswith('APT::Architectures::='):
        children.append(option.split('=', 1)[1])
    args = args[2:]
architectures = scalar.split(',') if scalar else (children or registered)
assert Path(os.environ['FOREIGN_REGISTRATION']).read_text() == os.environ['FOREIGN_ARCH']
if architectures != [os.environ['NATIVE_ARCH']]:
    sys.exit('404 Not Found: inherited foreign binary-arm64/Packages on archive.ubuntu.com')
if args == ['update'] and os.environ['FAIL_UPDATE'] == '1':
    sys.exit('fixture update failed')
assert args == ['update'] or args[:3] == ['install', '-y', '--no-install-recommends'], args
'''


class SetupDependenciesTests(unittest.TestCase):
    def invoke(self, native='amd64', installed='missing', fail_update=False, inheritance='list'):
        with tempfile.TemporaryDirectory(prefix='native apt ') as temporary:
            root = Path(temporary)
            foreign = 'arm64' if native == 'amd64' else 'amd64'
            registration = root / 'foreign-architecture'
            registration.write_text(foreign)
            apt_mock = root / 'apt.py'
            apt_mock.write_text(APT_MOCK)
            environment = dict(os.environ, NATIVE_ARCH=native, FOREIGN_ARCH=foreign,
                               FOREIGN_REGISTRATION=str(registration), INSTALLED=installed,
                               APT_MOCK=str(apt_mock), TEST_PYTHON=sys.executable,
                               APT_LOG=str(root / 'apt-log'), QUERY_LOG=str(root / 'query-log'),
                               FAIL_UPDATE='1' if fail_update else '0', APT_INHERITANCE=inheritance)
            environment.pop('BASH_ENV', None)
            result = subprocess.run(['bash', '-e', '-c', FUNCTIONS + MOCKS], cwd=ROOT,
                                    env=environment, capture_output=True, text=True)
            self.assertEqual(registration.read_text(), foreign)
            apt_log = root / 'apt-log'
            calls = [json.loads(line) for line in apt_log.read_text().splitlines()] if apt_log.exists() else []
            queries = (root / 'query-log').read_text().splitlines()
            return result, calls, queries

    def test_native_and_guest_apt_replace_inherited_architectures_without_losing_packages(self):
        for native in ('amd64', 'arm64'):
            for inheritance in ('dpkg', 'list', 'scalar'):
                with self.subTest(native=native, inheritance=inheritance):
                    result, calls, _ = self.invoke(native=native, inheritance=inheritance)
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertEqual(calls, [
                        ['-o', f'APT::Architectures={native}', 'update'],
                        ['-o', f'APT::Architectures={native}', 'install', '-y', '--no-install-recommends', *PACKAGES],
                    ])

    def test_foreign_installed_tools_do_not_satisfy_native_admission(self):
        result, calls, queries = self.invoke(installed='foreign')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(len(calls), 2)
        self.assertTrue(all(package.endswith(':amd64') for package in queries), queries)

    def test_installed_native_tools_skip_apt(self):
        result, calls, queries = self.invoke(installed='native')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(calls, [])
        self.assertEqual(queries, [package + ':amd64' for package in PACKAGES[6:]])

    def test_failed_update_is_not_ignored_or_followed_by_install(self):
        result, calls, _ = self.invoke(fail_update=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('fixture update failed', result.stderr)
        self.assertEqual(len(calls), 1)


if __name__ == '__main__':
    unittest.main()
