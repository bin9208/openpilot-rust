"""Reproduce missing hosted-image tools at actual local workflow boundaries."""
import hashlib
from pathlib import Path
import re
import shlex
import subprocess
import unittest

import yaml

ROOT = Path(__file__).resolve().parents[3]
BASE = '2765b4ab1c15a05c72d23c4ca32c033011c14543'
BLOBS = {'jetlink.yml': 'cf53198ffdfa6fb3ebe15d94626d97604c6542ff',
         'naver-ci.yml': '11ce7192b149633ad49b954b4cb5b5f78739a5ce'}
SDK_ACTION = 'android-actions/setup-android@be39fa834029ff78f1a44aa3bb0819b8fc2bd8fd'


def workflow(name, historical=False):
    path = '.github/workflows/' + name
    if historical:
        raw = subprocess.check_output(['git', 'show', BASE + ':' + path], cwd=ROOT)
        identity = hashlib.sha1(b'blob ' + str(len(raw)).encode() + b'\0' + raw).hexdigest()
        if identity != BLOBS[name]:
            raise AssertionError('Historical workflow identity changed')
    else:
        raw = (ROOT / path).read_bytes()
    return yaml.load(raw, Loader=yaml.BaseLoader)['jobs']


def runs(job):
    return [(index, step['run']) for index, step in enumerate(job['steps']) if 'run' in step]


class LocalJavaToolingTests(unittest.TestCase):
    def test_real_2765_failures_lack_explicit_tool_preparation(self):
        android = workflow('jetlink.yml', historical=True)['android-core']
        self.assertFalse(any(step.get('uses') == SDK_ACTION for step in android['steps']))
        self.assertFalse(any('ANDROID_HOME=' in command or 'sdkmanager ' in command
                             for _, command in runs(android)))
        mapping = workflow('naver-ci.yml', historical=True)['phone-mapping-contract']
        self.assertTrue(any(command.startswith('mvn ') for _, command in runs(mapping)))
        self.assertFalse(any('apt-native' in command and 'install' in command and 'maven' in command
                             for _, command in runs(mapping)))

    def test_declared_sdk_is_prepared_in_owned_temp_before_unchanged_app_checks(self):
        job = workflow('jetlink.yml')['android-core']
        prior = workflow('jetlink.yml', historical=True)['android-core']
        app = (ROOT / 'tools/jetlink_android/app/build.gradle.kts').read_text()
        platform = re.search(r'compileSdk\s*=\s*(\d+)', app).group(1)
        build_tools = re.search(r'buildToolsVersion\s*=\s*"([^"]+)"', app).group(1)
        steps = job['steps']
        sdk = next(i for i, step in enumerate(steps) if step.get('uses') == SDK_ACTION)
        prep, script = next((i, text) for i, text in runs(job) if 'ANDROID_HOME=' in text)
        install, command = next((i, text) for i, text in runs(job) if text.startswith('sdkmanager '))
        consumer, _ = next((i, text) for i, text in runs(job) if 'gradlew ' in text)
        self.assertLess(prep, sdk)
        self.assertLess(sdk, install)
        self.assertLess(install, consumer)
        self.assertIn('df --output=avail -B1 "$RUNNER_TEMP"', script)
        self.assertIn('exit 1', script)
        self.assertIn('4 * 1024 * 1024 * 1024', script)
        self.assertIn('ANDROID_HOME=$RUNNER_TEMP/jetlink-android-sdk', script)
        self.assertIn('ANDROID_SDK_ROOT=$RUNNER_TEMP/jetlink-android-sdk', script)
        self.assertEqual(shlex.split(command), ['sdkmanager', 'platforms;android-' + platform,
                                               'build-tools;' + build_tools])
        python = next(i for i, step in enumerate(steps) if step.get('uses', '').startswith('actions/setup-python@'))
        inspect = next(i for i, text in runs(job) if 'check_apk.py' in text)
        self.assertLess(python, inspect)
        self.assertEqual(job['timeout-minutes'], prior['timeout-minutes'])
        old_checks = [text for _, text in runs(prior)]
        self.assertEqual([text for _, text in runs(job) if text in old_checks], old_checks)
        self.assertEqual(steps[-1], {'uses': './.github/actions/job-temp', 'if': 'always()',
                                    'with': {'cleanup': 'true'}})

    def test_native_maven_install_precedes_unchanged_jvm_and_wire_checks(self):
        job = workflow('naver-ci.yml')['phone-mapping-contract']
        prior = workflow('naver-ci.yml', historical=True)['phone-mapping-contract']
        install, command = next((i, text) for i, text in runs(job) if 'apt-native' in text)
        consumer, _ = next((i, text) for i, text in runs(job) if text.startswith('mvn -B '))
        self.assertLess(install, consumer)
        self.assertIn('df --output=avail -B1 "$RUNNER_TEMP"', command)
        self.assertIn('exit 1', command)
        self.assertIn('1024 * 1024 * 1024', command)
        self.assertIn('"$GITHUB_WORKSPACE/tools/ci/apt-native" update', command)
        self.assertIn('"$GITHUB_WORKSPACE/tools/ci/apt-native" install -y --no-install-recommends maven', command)
        self.assertIn('mvn --version', command)
        old_checks = [text for _, text in runs(prior)]
        self.assertEqual([text for _, text in runs(job) if text in old_checks], old_checks)
        self.assertEqual(job['timeout-minutes'], prior['timeout-minutes'])


if __name__ == '__main__':
    unittest.main()
