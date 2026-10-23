"""One-off #516 cleanup of APT downloads; installed packages are preserved."""

import hashlib
import json
import os
from pathlib import Path
import shutil
import stat
import subprocess


WORKSPACE = Path(os.environ['GITHUB_WORKSPACE'])
OUTPUT = Path(os.environ['CI_JOB_TEMP'])
CACHE = Path('/var/cache/apt/archives')
REQUIRED_GROWTH = 12 * 1024**3


def installed_state():
    sources = hashlib.sha256()
    paths = [Path('/etc/apt/sources.list')]
    paths += sorted(Path('/etc/apt/sources.list.d').glob('*.list'))
    paths += sorted(Path('/etc/apt/sources.list.d').glob('*.sources'))
    for path in paths:
        if path.exists():
            sources.update(str(path).encode() + b'\0' + path.read_bytes() + b'\0')
    return {
        'dpkg_status_sha256': hashlib.sha256(Path('/var/lib/dpkg/status').read_bytes()).hexdigest(),
        'apt_sources_sha256': sources.hexdigest(),
        'native_arch': subprocess.check_output(['dpkg', '--print-architecture'], timeout=5).decode().strip(),
        'foreign_arches': subprocess.check_output(['dpkg', '--print-foreign-architectures'], timeout=5).decode().splitlines(),
    }


def cache_bytes():
    command = ['sudo', '-n', '/usr/bin/timeout', '--signal=TERM', '--kill-after=2s',
               '15s', '/usr/bin/du', '--one-file-system', '--summarize',
               '--block-size=1', '--', str(CACHE)]
    value = subprocess.check_output(command, timeout=20).decode().split('\t', 1)[0]
    return int(value)


def clean(report):
    if os.environ['RUNNER_NAME'] != 'bin-actions' or os.getuid() == 0:
        raise RuntimeError('Expected registered unprivileged runner')
    if shutil.disk_usage(WORKSPACE).free < 1024**2:
        raise RuntimeError('Insufficient room for the small recovery receipt')
    info = CACHE.lstat()
    if (not stat.S_ISDIR(info.st_mode) or CACHE.resolve() != CACHE
        or info.st_uid != 0 or info.st_dev != WORKSPACE.stat().st_dev):
        raise RuntimeError('Unexpected package-download cache identity/filesystem')
    report['cache_identity'] = {'device': info.st_dev, 'inode': info.st_ino, 'uid': info.st_uid}
    report['before_state'] = installed_state()
    report['before_download_bytes'] = cache_bytes()
    report['free_before_bytes'] = shutil.disk_usage(WORKSPACE).free
    report['cleanup_needed'] = report['free_before_bytes'] < REQUIRED_GROWTH
    if report['cleanup_needed']:
        # clean also has index-partial/binary-cache targets. Confine those to
        # fresh owned job-temp paths; persistent indexes are not recovery data.
        private_lists = OUTPUT / 'apt-clean-lists'
        private_lists.mkdir(mode=0o700)
        (private_lists / 'partial').mkdir(mode=0o700)
        # APT owns its archive lock. clean fetches/installs/removes no packages;
        # it removes retrieved .deb files, not interrupted project build state.
        command = ['sudo', '-n', '/usr/bin/timeout', '--signal=TERM', '--kill-after=2s',
                   '45s', '/usr/bin/apt-get', '-o', 'Debug::NoLocking=false',
                   '-o', 'Dir::Cache=/var/cache/apt', '-o', 'Dir::Cache::archives=archives/',
                   '-o', 'APT::Get::List-Cleanup=false',
                   '-o', 'Dir::State::lists=' + str(private_lists) + '/',
                   '-o', 'Dir::Cache::pkgcache=' + str(OUTPUT / 'apt-clean-pkgcache.bin'),
                   '-o', 'Dir::Cache::srcpkgcache=' + str(OUTPUT / 'apt-clean-srcpkgcache.bin'), 'clean']
        with (OUTPUT / 'apt-download-clean.log').open('xb') as stream:
            result = subprocess.run(command, stdout=stream, stderr=subprocess.STDOUT,
                                    timeout=50, check=False)
        report['cleanup_exit'] = result.returncode
        if result.returncode != 0:
            raise RuntimeError(f'APT download cleanup failed: {result.returncode}')
    report['after_download_bytes'] = cache_bytes()
    report['after_state'] = installed_state()
    if report['before_state'] != report['after_state']:
        raise RuntimeError('Installed packages, source configuration or architecture state changed')
    after_info = CACHE.lstat()
    if (after_info.st_dev, after_info.st_ino, after_info.st_uid) != (info.st_dev, info.st_ino, info.st_uid):
        raise RuntimeError('Package-download cache root changed')
    report['installed_state_preserved'] = True


report = {'schema': 1, 'source_commit': os.environ['GITHUB_SHA'],
          'runner': os.environ['RUNNER_NAME'], 'target': str(CACHE),
          'required_arm_growth_bytes': REQUIRED_GROWTH,
          'installed_state_preserved': False,
          'project_temp_toolchain_or_git_deletions': False}
try:
    clean(report)
except Exception as error:
    report['error'] = type(error).__name__ + ': ' + str(error)
finally:
    report['free_after_bytes'] = shutil.disk_usage(WORKSPACE).free
    report['arm_capacity_admitted'] = report['free_after_bytes'] >= REQUIRED_GROWTH
    if 'free_before_bytes' in report:
        report['net_free_byte_change'] = report['free_after_bytes'] - report['free_before_bytes']
    output = OUTPUT / 'runner-download-cleanup.json'
    with os.fdopen(os.open(output, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600), 'w') as stream:
        json.dump(report, stream, indent=2)
        stream.write('\n')
    print(json.dumps(report))
if not report['installed_state_preserved'] or not report['arm_capacity_admitted']:
    raise SystemExit('ARM capacity/preserved package-state acceptance is incomplete; inspect receipt')
