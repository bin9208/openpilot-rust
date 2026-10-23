"""One-off #516 runner maintenance; do not invoke from ordinary product flows."""

import hashlib
import json
import os
from pathlib import Path
import re
import resource
import shutil
import signal
import stat
import subprocess


WORKSPACE = Path(os.environ['GITHUB_WORKSPACE'])
OUTPUT = Path(os.environ['CI_JOB_TEMP'])
ENV = dict(os.environ, GIT_OPTIONAL_LOCKS='0', GIT_NO_LAZY_FETCH='1',
           GIT_TERMINAL_PROMPT='0')
METADATA_LIMIT = 64 * 1024**2
LOG_LIMIT = 8 * 1024**2
REQUIRED_GROWTH = 12 * 1024**3


def bounded_file():
    resource.setrlimit(resource.RLIMIT_FSIZE, (METADATA_LIMIT,) * 2)


def git_output(*args):
    return subprocess.check_output(['git', *args], cwd=WORKSPACE, env=ENV, timeout=30)


def git_log(label, seconds, *args):
    path = OUTPUT / f'git-recovery-{label}.log'
    truncated = False
    def expired(_number, _frame):
        raise TimeoutError(f'{label} outer watchdog expired')
    previous_handler = signal.signal(signal.SIGALRM, expired)
    try:
        with path.open('xb') as stream:
            # No Popen context manager: its implicit unbounded wait would defeat
            # the watchdog if pack I/O remains uninterruptible after SIGKILL.
            process = subprocess.Popen(['/usr/bin/timeout', '--signal=TERM', '--kill-after=2s',
                                        f'{seconds}s', 'git', *args], cwd=WORKSPACE, env=ENV,
                                       stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                                       stderr=subprocess.STDOUT, start_new_session=True)
            signal.setitimer(signal.ITIMER_REAL, seconds + 5)
            try:
                retained = 0
                while chunk := process.stdout.read(64 * 1024):
                    remaining = LOG_LIMIT - retained
                    stream.write(chunk[:remaining])
                    retained += min(remaining, len(chunk))
                    truncated |= len(chunk) > remaining
                status = process.wait(timeout=5)
            except BaseException:
                try:
                    os.killpg(process.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired as error:
                    raise RuntimeError(f'{label} cleanup incomplete for owned PID {process.pid}') from error
                raise
            finally:
                signal.setitimer(signal.ITIMER_REAL, 0)
                process.stdout.close()
    finally:
        signal.setitimer(signal.ITIMER_REAL, 0)
        signal.signal(signal.SIGALRM, previous_handler)
    if status != 0 or truncated:
        raise RuntimeError(f'{label} failed: exit={status}, log_truncated={truncated}; see owned log')


def file_identity(path):
    info = path.lstat()
    if not stat.S_ISREG(info.st_mode):
        raise RuntimeError(f'Non-regular Git metadata: {path.name}')
    return {'device': info.st_dev, 'inode': info.st_ino, 'bytes': info.st_size,
            'allocated_bytes': info.st_blocks * 512, 'links': info.st_nlink,
            'mtime_ns': info.st_mtime_ns}


def metadata(label, common):
    path = OUTPUT / f'git-recovery-{label}-objects.log'
    with path.open('xb') as stream:
        result = subprocess.run(['/usr/bin/timeout', '--signal=TERM', '--kill-after=2s',
                                 '120s', 'git', 'cat-file', '--batch-all-objects',
                                 '--batch-check=%(objectname) %(objecttype) %(objectsize)',
                                 '--buffer'], cwd=WORKSPACE, env=ENV,
                                stdin=subprocess.DEVNULL, stdout=stream,
                                stderr=subprocess.DEVNULL, timeout=125,
                                preexec_fn=bounded_file, check=False)
    if result.returncode != 0:
        raise RuntimeError(f'{label} object enumeration failed: {result.returncode}')
    digest = hashlib.sha256()
    count, raw_bytes = 0, 0
    with path.open('rb') as stream:
        for row in stream:
            fields = row.split()
            if (not row.endswith(b'\n') or len(fields) != 3
                or not re.fullmatch(rb'[0-9a-f]{40,64}', fields[0])
                or fields[1] not in (b'blob', b'tree', b'commit', b'tag')
                or not fields[2].isdigit()):
                raise RuntimeError(f'{label} malformed object metadata')
            digest.update(row)
            count += 1
            raw_bytes += int(fields[2])
    refs = git_output('show-ref', '--head')
    index = common / 'index'
    shallow = common / 'shallow'
    result = {
        'object_count': count, 'object_raw_bytes': raw_bytes,
        'objects_sha256': digest.hexdigest(),
        'refs_sha256': hashlib.sha256(refs).hexdigest(),
        'index_sha256': hashlib.sha256(index.read_bytes()).hexdigest(),
        'shallow_sha256': hashlib.sha256(shallow.read_bytes()).hexdigest() if shallow.exists() else None,
        'reflogs_sha256': hashlib.sha256(git_output('reflog', 'show', '--all', '--format=%H %gD')).hexdigest(),
        'tracked_status_sha256': hashlib.sha256(git_output('status', '--porcelain=v1', '-z', '--untracked-files=no')).hexdigest(),
        'count_objects': git_output('count-objects', '-v').decode(),
        'pack_files': {}, 'other_pack_files': {},
    }
    for entry in sorted((common / 'objects/pack').iterdir()):
        group = 'pack_files' if entry.name.endswith(('.pack', '.idx', '.rev', '.bitmap')) else 'other_pack_files'
        result[group][entry.name] = file_identity(entry)
    return result


def require_quiet_git(common):
    locks = [path for path in common.glob('*.lock')]
    locks.extend((common / 'refs').rglob('*.lock'))
    locks.extend((common / 'objects/pack').glob('*.lock'))
    if locks or (common / 'gc.pid').exists():
        raise RuntimeError('Preserve existing Git lock/maintenance state')
    for path in Path('/proc').glob('[0-9]*/comm'):
        try:
            name = path.read_text().strip()
        except FileNotFoundError:
            continue
        if name.startswith('git') or name in ('pack-objects', 'index-pack'):
            raise RuntimeError(f'Preserve concurrent Git process: {path.parent.name}/{name}')


def recover(report):
    if os.environ['RUNNER_NAME'] != 'bin-actions' or os.getuid() == 0:
        raise RuntimeError('Expected registered unprivileged runner')
    common = Path(git_output('rev-parse', '--git-common-dir').decode().strip())
    if not common.is_absolute():
        common = WORKSPACE / common
    if common.resolve() != WORKSPACE / '.git' or common.is_symlink():
        raise RuntimeError('Unexpected shared Git directory')
    if (common / 'objects/info/alternates').exists():
        raise RuntimeError('Alternate object stores need separate ownership review')
    if any((common / 'objects/pack').glob('*.promisor')):
        raise RuntimeError('Partial-clone packs need separate preservation review')
    require_quiet_git(common)
    if shutil.disk_usage(WORKSPACE).free < 2 * METADATA_LIMIT:
        raise RuntimeError('Insufficient space for bounded metadata')
    report['before'] = metadata('before', common)
    # Complete uncompressed objects plus >1% compression overhead, per-object
    # header/index allowance and 256 MiB bounded metadata/log bookkeeping
    # bound the new pack growth. Pack writes do not inherit a log-file limit.
    before = report['before']
    growth = ((before['object_raw_bytes'] * 101 + 99) // 100
              + before['object_count'] * 1024 + 256 * 1024**2)
    report['repack_growth_upper_bound_bytes'] = growth
    report['free_before_bytes'] = shutil.disk_usage(WORKSPACE).free
    if report['free_before_bytes'] < growth:
        raise RuntimeError('Insufficient actual capacity for source-preserving repack')
    git_log('fsck-before', 120, 'fsck', '--full', '--no-progress', '--no-dangling')
    require_quiet_git(common)
    report['repack_started'] = True
    # -k retains packed AND loose unreachable objects. No gc, expire or prune
    # command is used; -d removes only redundant representations after packing.
    git_log('repack', 240, '-c', 'pack.threads=1', '-c', 'pack.windowMemory=64m',
            '-c', 'repack.writeBitmaps=false', 'repack', '-a', '-d',
            '--keep-unreachable', '--window=0', '--threads=1', '--quiet', '-n')
    report['after'] = metadata('after', common)
    for key in ('object_count', 'object_raw_bytes', 'objects_sha256', 'refs_sha256',
                'index_sha256', 'shallow_sha256', 'reflogs_sha256',
                'tracked_status_sha256', 'other_pack_files'):
        if before[key] != report['after'][key]:
            raise RuntimeError(f'Git preservation mismatch: {key}')
    git_log('fsck-after', 120, 'fsck', '--full', '--no-progress', '--no-dangling')
    report['preservation_verified'] = True


report = {'schema': 1, 'source_commit': os.environ['GITHUB_SHA'],
          'failed_source_commit': '10919538d31b04d28d9a23b383d925441434d579',
          'runner': os.environ['RUNNER_NAME'], 'uid': os.getuid(),
          'repack_started': False, 'preservation_verified': False,
          'required_arm_growth_bytes': REQUIRED_GROWTH,
          'project_temp_or_toolchain_deletions': False}
try:
    recover(report)
except Exception as error:
    report['error'] = type(error).__name__ + ': ' + str(error)
finally:
    report['free_after_bytes'] = shutil.disk_usage(WORKSPACE).free
    report['arm_capacity_admitted'] = report['free_after_bytes'] >= REQUIRED_GROWTH
    if 'free_before_bytes' in report:
        report['net_free_byte_change'] = report['free_after_bytes'] - report['free_before_bytes']
    output = OUTPUT / 'runner-git-recovery.json'
    with os.fdopen(os.open(output, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600), 'w') as stream:
        json.dump(report, stream, indent=2)
        stream.write('\n')
    print(json.dumps({key: value for key, value in report.items() if key not in ('before', 'after')}))
if not report['preservation_verified']:
    raise SystemExit('Git recovery did not pass preservation verification; inspect receipt')
