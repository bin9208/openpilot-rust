#!/usr/bin/env python3
"""Real, job-owned host-kernel memory scope on registered or hosted ARM runners.

Root only sets up/enters/removes this job's cgroup. Test commands execute after
dropping to the original runner user. Existing runner processes are never moved.
"""
import json
import os
from pathlib import Path
import pwd
import runpy
import signal
import sys
import time


def identity():
    policy = runpy.run_path(str(Path(__file__).with_name('runner_policy.py')))
    policy['require_resource_scope_environment']()
    if policy['github_native_arm_allowed'](os.environ) and os.uname().machine != 'aarch64':
        raise RuntimeError('hosted ARM resource scope requires an actual AArch64 host')
    if os.geteuid() != 0:
        raise RuntimeError('owned Actions root setup required')
    uid, gid = int(os.environ['SUDO_UID']), int(os.environ['SUDO_GID'])
    if uid <= 0 or gid < 0:
        raise RuntimeError('non-root runner identity required')
    job = Path(os.environ['CI_JOB_TEMP'])
    owner = ':'.join(os.environ[k] for k in ('GITHUB_RUN_ID', 'GITHUB_RUN_ATTEMPT', 'GITHUB_JOB'))
    if (job.parent != Path('/tmp') or not job.name.startswith('openpilot-ci.')
            or job.resolve() != job or job.stat().st_uid != uid
            or (job / 'owner').is_symlink() or (job / 'owner').read_text() != owner):
        raise RuntimeError('refusing another job temporary directory')
    root = Path('/sys/fs/cgroup')
    version = 2 if (root / 'cgroup.controllers').is_file() else 1
    if version == 1:
        root /= 'memory'
    if root.resolve() != root or not (root / 'cgroup.procs').is_file():
        raise RuntimeError('real host memory controller missing')
    path = root / f'openpilot-ci-build-{job.name}-{uid}'
    return uid, gid, job, owner, path, version


def write(path, value):
    if path.is_symlink() or not path.is_file():
        raise RuntimeError(f'missing or redirected kernel controller: {path}')
    path.write_text(str(value))


def members(path):
    return [int(s) for s in (path / 'cgroup.procs').read_text().split()]


def prepare():
    uid, gid, job, owner, path, version = identity()
    if version == 2 and 'memory' not in (path.parent / 'cgroup.subtree_control').read_text().split():
        raise RuntimeError('shared memory controller is not enabled; no global setting changed')
    path.mkdir()  # A pre-existing path is never adopted or removed.
    lease = {'schema': 1, 'owner': owner, 'path': str(path), 'inode': path.stat().st_ino,
             'uid': uid, 'gid': gid, 'version': version, 'source_commit': os.environ['GITHUB_SHA']}
    # Persist ownership before subsequent setup so the always-cleanup step can
    # remove a partial attempt too. This lease is never uploaded with secrets.
    with (job / 'memory-scope-lease.json').open('x') as stream:
        json.dump(lease, stream)
        stream.flush()
        os.fsync(stream.fileno())
    os.chown(path, uid, gid)
    if version == 2:
        write(path / 'cgroup.subtree_control', '+memory')
    else:
        write(path / 'memory.use_hierarchy', 1)
    control = path / 'control'
    control.mkdir()
    for entry in (path / 'cgroup.procs', control, control / 'cgroup.procs'):
        os.chown(entry, uid, gid)
    with (job / 'resource-scope-evidence.json').open('x') as stream:
        # Native ARM VM execution is separate from physical board/device acceptance.
        json.dump({**lease, 'host_kernel': os.uname().release,
                   'host_architecture': os.uname().machine, 'physical_arm_cpu': False,
                   'native_arm_execution': os.uname().machine == 'aarch64',
                   'device_execution': False,
                   'existing_processes_migrated': False, 'shared_controller_modified': False}, stream)
    print(json.dumps(lease), flush=True)


def owned():
    uid, gid, job, owner, path, version = identity()
    lease_file = job / 'memory-scope-lease.json'
    if lease_file.is_symlink():
        raise RuntimeError('redirected memory scope lease')
    lease = json.loads(lease_file.read_text())
    if (lease.get('schema') != 1 or lease.get('owner') != owner
            or lease.get('path') != str(path) or lease.get('uid') != uid
            or lease.get('gid') != gid or lease.get('version') != version
            or lease.get('source_commit') != os.environ['GITHUB_SHA']
            or path.resolve() != path or path.stat().st_ino != lease.get('inode')
            or path.stat().st_uid != uid):
        raise RuntimeError('refusing an unrelated memory scope')
    return uid, gid, job, path, version


def enter(arguments):
    uid, gid, job, path, _ = owned()
    if not arguments or arguments[0] != '--' or len(arguments) < 2:
        raise RuntimeError('explicit unprivileged command required')
    write(path / 'control/cgroup.procs', os.getpid())
    if os.getpid() not in members(path / 'control'):
        raise RuntimeError('new command did not enter its control scope')
    os.setgroups([])
    os.setgid(gid)
    os.setuid(uid)
    os.environ['HOME'] = pwd.getpwuid(uid).pw_dir
    os.environ['NATIVE_TEST_CGROUP_PARENT'] = str(path)
    os.environ['TMPDIR'] = str(job)
    evidence = job / 'native-resource-evidence'
    evidence.mkdir(exist_ok=True)
    if evidence.is_symlink() or not evidence.is_dir() or evidence.stat().st_uid != uid:
        raise RuntimeError('refusing a redirected or unrelated resource evidence directory')
    os.environ['NATIVE_TEST_EVIDENCE_DIRECTORY'] = str(evidence)
    for key in ('SUDO_UID', 'SUDO_GID', 'SUDO_USER', 'SUDO_COMMAND'):
        os.environ.pop(key, None)
    if os.geteuid() == 0 or os.getgroups():
        raise RuntimeError('test credential drop failed')
    os.execvpe(arguments[1], arguments[1:], os.environ)


def cleanup():
    # A failure before preparation created nothing. Any existing lease must pass
    # the exact owner/source/inode checks before a task can be signalled.
    _, _, job, _, _, _ = identity()
    if not (job / 'memory-scope-lease.json').exists():
        return
    _, _, _, path, version = owned()
    if os.getpid() in members(path / 'control'):
        raise RuntimeError('cleanup must run outside its owned scope')
    if version == 2 and (path / 'cgroup.kill').exists():
        write(path / 'cgroup.kill', 1)
    deadline = time.monotonic() + 10
    while True:
        directories = [path] + [p for p in path.rglob('*') if p.is_dir() and not p.is_symlink()]
        if len(directories) > 1024:
            raise RuntimeError('excessive owned memory scope descendants')
        remaining = False
        for directory in directories:
            for pid in members(directory):
                if pid <= 1 or pid == os.getpid():
                    raise RuntimeError('unsafe cleanup process identity')
                remaining = True
                try:
                    fd = os.pidfd_open(pid)
                except ProcessLookupError:
                    continue
                try:
                    if pid in members(directory):
                        signal.pidfd_send_signal(fd, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                finally:
                    os.close(fd)
        if not remaining:
            for directory in sorted(directories, key=lambda p: len(p.parts), reverse=True):
                directory.rmdir()
            print('Only this job memory scope removed; other workloads preserved', flush=True)
            return
        if time.monotonic() >= deadline:
            raise RuntimeError('owned memory scope did not become empty')
        time.sleep(0.05)


if __name__ == '__main__':
    operation, *rest = sys.argv[1:]
    if operation == 'prepare' and not rest:
        prepare()
    elif operation == 'enter':
        enter(rest)
    elif operation == 'cleanup' and not rest:
        cleanup()
    else:
        raise SystemExit('prepare | enter -- UNPRIVILEGED_COMMAND... | cleanup')
