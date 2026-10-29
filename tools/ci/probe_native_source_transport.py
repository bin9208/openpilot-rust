"""Actions admission for real factory ARM sudo in a private user/cgroup namespace.

This is a transport test for the remaining full source-entry check. It does not
run a compiler, publish inputs, start a runtime, or claim physical ARM evidence.
"""
import ctypes
import hashlib
import json
import lzma
import os
from pathlib import Path
import select
import signal
import shutil
import subprocess
import sys
import time

NEWUSER, NEWNS, NEWCGROUP = 0x10000000, 0x00020000, 0x02000000
RAW_BYTES = 4718592000
RAW_SHA = '375c5d22335770ac08750660bb5b29a3331c550d6a9875b35f36b88af792ea44'
SPARSE_SHA = 'a67410a888105ecce7bcc05e17cda77fd10572aa4380e9de242a7c92f3497114'
IMAGE_SHA = '077d42186def98025ffad9723774c96f45da10c87cc6fedbc58d0cf4fa65ffd4'


def source_module(name):
    path = Path(__file__).with_name(name+'.py')
    namespace = dict(__name__='owned_transport_'+name, __file__=str(path))
    exec(compile(path.read_bytes(), str(path), 'exec'), namespace)
    return namespace


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def unshare(flags):
    library = ctypes.CDLL(None, use_errno=True)
    if library.unshare(flags):
        error = ctypes.get_errno()
        raise OSError(error, f'unshare({flags:#x}): '+os.strerror(error))


def command(executable, *arguments, timeout=60):
    assert executable in ('/usr/bin/mount', '/usr/bin/umount', '/usr/sbin/debugfs', '/usr/bin/simg2img')
    flags = ('--no-mtab',) if executable in ('/usr/bin/mount', '/usr/bin/umount') else ()
    argv = list(map(str, (executable, *flags, *arguments)))
    print(json.dumps(argv), file=sys.stderr, flush=True)
    return subprocess.run(argv, check=True, timeout=timeout)


def handlers():
    root = Path('/proc/sys/fs/binfmt_misc')
    return {p.name: p.read_text() for p in root.iterdir() if p.name != 'register'}


def static_emulator():
    path = Path('/usr/bin/qemu-aarch64-static')
    with path.open('rb') as stream:
        header = stream.read(64)
        assert header[:6] == b'\x7fELF\x02\x01' and int.from_bytes(header[18:20], 'little') == 62
        offset = int.from_bytes(header[32:40], 'little')
        size = int.from_bytes(header[54:56], 'little')
        count = int.from_bytes(header[56:58], 'little')
        assert size == 56 and 0 < count <= 4096
        stream.seek(offset)
        for _ in range(count):
            row = stream.read(size)
            assert len(row) == size
            assert int.from_bytes(row[:4], 'little') != 3, 'fixed chroot interpreter must not require a host ELF loader'
    return digest(path)


def prepare_image():
    context = source_module('native_cache')['Context']()
    assert subprocess.check_output(['/usr/bin/git', '-C', context.workspace, 'rev-parse', 'HEAD'],
                                   text=True).strip() == os.environ['GITHUB_SHA']
    assert json.loads((context.workspace/'rust/native-system-sdk.json').read_text())['factory_image_sha256'] == RAW_SHA
    receipt = json.loads((context.job/'native-agnos-assets-first.json').read_text())['agnos-system-19-8-carrot-bt1']
    image = Path(receipt['path'])
    assert image.resolve(strict=True) == image and image.is_relative_to(context.job/'native-agnos-assets')
    assert receipt['sha256'] == IMAGE_SHA and receipt['bytes'] == image.stat().st_size == 1010655092
    assert digest(image) == IMAGE_SHA
    growth = 2*RAW_BYTES+64*1024**2
    assert shutil.disk_usage(context.job).free >= growth, 'insufficient admission for this image operation'
    work = context.job/'native-agnos'
    work.mkdir()
    sparse, raw = work/'system.sparse', work/'system.raw'
    with lzma.open(image, 'rb') as stream, sparse.open('xb') as output:
        total = 0
        while block := stream.read(1024**2):
            total += len(block)
            if total > RAW_BYTES+64*1024**2:
                raise ValueError('factory sparse image exceeds its operation bound')
            output.write(block)
    assert digest(sparse) == SPARSE_SHA
    command('/usr/bin/simg2img', sparse, raw, timeout=600)
    assert raw.stat().st_size == RAW_BYTES and digest(raw) == RAW_SHA
    raw.chmod(0o444)
    sparse.unlink()
    (work/'transport-image.json').write_text(json.dumps(dict(
        source_commit=os.environ['GITHUB_SHA'], raw_sha256=RAW_SHA, raw_bytes=RAW_BYTES,
        image_origin_transferred_bytes=receipt['transferred_bytes'],
        cache_service_traffic_is_separate=True, existing_data_removed=False), indent=2)+'\n')


ROOT_PROBE = r'''
import json, os, platform
from pathlib import Path
assert platform.machine() == 'aarch64' and os.geteuid() == 0
assert Path('/VERSION').read_text().strip() == '19.8-carrot-bt1'
uid = int(os.environ['SUDO_UID'])
assert uid > 0
root = Path('/sys/fs/cgroup')
assert 'memory' in (root/'cgroup.subtree_control').read_text().split()
group = root/'source-entry-admission'
group.mkdir()
try:
    (group/'memory.max').write_text(str(128*1024**2))
    os.chown(group, uid, int(os.environ['SUDO_GID']))
    assert group.stat().st_uid == uid
    assert not (group/'cgroup.procs').read_text().strip()
    print(json.dumps(dict(real_uid=os.getuid(), effective_uid=os.geteuid(),
        sudo_uid=uid, root_cgroup_created_and_chowned=True,
        user_namespace=os.readlink('/proc/self/ns/user'),
        cgroup_namespace=os.readlink('/proc/self/ns/cgroup'))))
finally:
    group.rmdir()
'''

USER_PROBE = r'''
import hashlib, json, os, platform, stat, subprocess, sys
from pathlib import Path
assert platform.machine() == 'aarch64'
assert os.getuid() == os.geteuid() == int(sys.argv[1]) > 0
assert not os.getgroups()
sudo = Path('/usr/bin/sudo')
assert sudo.stat().st_uid == 0 and sudo.stat().st_mode & stat.S_ISUID
with sudo.open('rb') as stream:
    header = stream.read(20)
assert header[:6] == b'\x7fELF\x02\x01' and int.from_bytes(header[18:20], 'little') == 183
result = subprocess.run(['/usr/bin/sudo', '-n', '/usr/bin/python3', '-I', '-B', '-S',
    '-c', sys.argv[2]], check=False, capture_output=True, text=True, timeout=30)
if result.returncode:
    print(result.stdout, file=sys.stderr)
    print(result.stderr, file=sys.stderr)
result.check_returncode()
row = json.loads(result.stdout)
assert row['real_uid'] == row['effective_uid'] == 0
assert row['sudo_uid'] == os.getuid()
row.update(original_user=os.getuid(), proot_used=False,
    factory_sudo_sha256=hashlib.sha256(sudo.read_bytes()).hexdigest(),
    actual_arm_sudo_executed=True, source_entry_full_build_executed=False)
print(json.dumps(row), flush=True)
'''


def child(uid, gid, work, factory, ready, go, expected_userns, group, group_inode, write):
    unshare(NEWUSER)
    os.write(ready, b'U')
    assert os.read(go, 1) == b'M'
    assert os.getuid() == os.geteuid() == 0
    assert os.readlink('/proc/self/ns/user') != expected_userns
    # The child migrates only itself. No numeric PID from another process is
    # ever written by the parent during setup, including PID-reuse races.
    assert group.stat().st_ino == group_inode
    write(group/'cgroup.procs', os.getpid())
    unshare(NEWNS | NEWCGROUP)
    command('/usr/bin/mount', '--make-rprivate', '/')
    registry, memory = work/'binfmt', work/'memory'
    registry.mkdir()
    memory.mkdir()
    command('/usr/bin/mount', '-t', 'binfmt_misc', 'binfmt_misc', registry)
    assert {p.name for p in registry.iterdir()} == {'status', 'register'}
    magic = r'\x7fELF\x02\x01\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x02\x00\xb7\x00'
    mask = r'\xff\xff\xff\xff\xff\xff\xff\x00\xff\xff\xff\xff\xff\xff\xff\xff\xfe\xff\xff\xff'
    (registry/'register').write_text(':owned-arm:M::'+magic+':'+mask+':/usr/bin/qemu-aarch64-static:CF')
    assert 'flags: OCF' in (registry/'owned-arm').read_text()
    command('/usr/bin/mount', '-t', 'cgroup2', 'cgroup2', memory)
    assert memory.stat().st_ino == group_inode
    (memory/'control').mkdir()
    write(memory/'control/cgroup.procs', os.getpid())
    write(memory/'cgroup.subtree_control', '+memory')
    for name in ('dev', 'proc', 'sys'):
        command('/usr/bin/mount', '-t', 'tmpfs', 'tmpfs', factory/name)
    command('/usr/bin/mount', '--bind', '/proc', factory/'proc')
    command('/usr/bin/mount', '-o', 'remount,bind,ro', factory/'proc')
    for name in ('null', 'zero', 'random', 'urandom'):
        target = factory/'dev'/name
        target.touch()
        command('/usr/bin/mount', '--bind', Path('/dev')/name, target)
    (factory/'dev/pts').mkdir()
    command('/usr/bin/mount', '-t', 'devpts', '-o', 'newinstance,ptmxmode=0666,mode=0620,gid=5',
            'devpts', factory/'dev/pts')
    (factory/'dev/ptmx').symlink_to('pts/ptmx')
    (factory/'sys/fs/cgroup').mkdir(parents=True)
    command('/usr/bin/mount', '--bind', memory, factory/'sys/fs/cgroup')
    os.chroot(factory)
    os.chdir('/')
    os.setgroups([])
    os.setgid(gid)
    os.setuid(uid)
    os.execve('/usr/bin/python3', ['/usr/bin/python3', '-I', '-B', '-S', '-c', USER_PROBE,
        str(uid), ROOT_PROBE], {'PATH':'/usr/sbin:/usr/bin:/sbin:/bin', 'LC_ALL':'C', 'HOME':'/home/comma'})


def probe():
    scope_module = source_module('native_resource_scope')
    write = scope_module['write']
    uid, gid, job, scope, version = scope_module['owned']()
    assert version == 2 and max(uid, gid) < 65536
    raw = job/'native-agnos/system.raw'
    assert raw.resolve(strict=True) == raw and raw.stat().st_uid == uid
    assert raw.stat().st_size == RAW_BYTES and digest(raw) == RAW_SHA
    emulator_sha = static_emulator()
    original = handlers()
    global_control = Path('/sys/fs/cgroup/cgroup.subtree_control').read_text()
    outer_userns = os.readlink('/proc/self/ns/user')
    work = job/'source-entry-transport'
    work.mkdir()
    factory = work/'factory'
    factory.mkdir()
    assert shutil.disk_usage(job).free >= RAW_BYTES+64*1024**2
    # Real root extraction retains factory ownership/SUID metadata. This avoids
    # changing global loop devices or requiring an ext4 mount in the host ns.
    command('/usr/sbin/debugfs', '-R', f'rdump / {factory}', raw, timeout=600)
    assert (factory/'VERSION').read_text().strip() == '19.8-carrot-bt1'
    sudo = factory/'usr/bin/sudo'
    assert sudo.stat().st_uid == 0 and sudo.stat().st_mode & 0o4000
    group = scope/'source-entry-transport'
    group.mkdir()
    group_inode = group.stat().st_ino
    read_ready, send_ready = os.pipe()
    read_go, send_go = os.pipe()
    pid = os.fork()
    if pid == 0:
        os.close(read_ready)
        os.close(send_go)
        try:
            with (work/'arm.stdout').open('wb') as output, (work/'arm.stderr').open('wb') as error:
                os.dup2(output.fileno(), 1)
                os.dup2(error.fileno(), 2)
                child(uid, gid, work, factory, send_ready, read_go, outer_userns, group, group_inode, write)
        except BaseException as error:
            os.write(2, f'namespace admission: {type(error).__name__}: {error}\n'.encode())
        finally:
            os._exit(1)
    os.close(send_ready)
    os.close(read_go)
    pidfd = os.pidfd_open(pid)
    # A proc directory fd pins this child even if it exits during setup.
    procfd = os.open(f'/proc/{pid}', os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    reaped = False
    try:
        assert select.select([read_ready], [], [], 30)[0], 'user namespace readiness exceeded 30 seconds'
        assert os.read(read_ready, 1) == b'U', (work/'arm.stderr').read_text()[-8192:]
        for name in ('uid_map', 'gid_map'):
            fd = os.open(name, os.O_WRONLY | os.O_NOFOLLOW, dir_fd=procfd)
            try:
                assert os.write(fd, b'0 0 65536\n') == len(b'0 0 65536\n')
            finally:
                os.close(fd)
        os.write(send_go, b'M')
        deadline = time.monotonic()+120
        while True:
            waited, status = os.waitpid(pid, os.WNOHANG)
            if waited:
                reaped = True
                break
            if time.monotonic() >= deadline:
                raise TimeoutError('actual ARM sudo admission exceeded 120 seconds')
            time.sleep(0.05)
        assert os.waitstatus_to_exitcode(status) == 0, (work/'arm.stderr').read_text()[-8192:]
        assert handlers() == original
        assert Path('/sys/fs/cgroup/cgroup.subtree_control').read_text() == global_control
        result = json.loads((work/'arm.stdout').read_text())
        result.update(source_commit=os.environ['GITHUB_SHA'], host_kernel=os.uname().release,
            host_architecture=os.uname().machine, physical_arm_cpu=False, agnos_kernel=False,
            factory_image_sha256=RAW_SHA, shared_binfmt_modified=False,
            shared_controller_modified=False, namespace_cgroup_inode=group_inode,
            root_cgroup_namespace_verified=True, qemu_sha256=emulator_sha,
            static_fixed_interpreter_verified=True)
    finally:
        assert group.stat().st_ino == group_inode and group.parent == scope
        write(group/'cgroup.kill', 1)
        if not reaped:
            try:
                signal.pidfd_send_signal(pidfd, signal.SIGKILL)
            except ProcessLookupError:
                pass
            os.waitpid(pid, 0)
        for fd in (pidfd, procfd, read_ready, send_go):
            os.close(fd)
        deadline = time.monotonic()+10
        while True:
            directories = [group, *(p for p in group.rglob('*') if p.is_dir() and not p.is_symlink())]
            assert len(directories) <= 1024
            if all(not (p/'cgroup.procs').read_text().strip() for p in directories):
                for path in sorted(directories, key=lambda p: len(p.parts), reverse=True):
                    path.rmdir()
                break
            if time.monotonic() >= deadline:
                raise TimeoutError('owned admission cgroup remained populated')
            time.sleep(0.05)
    assert handlers() == original
    assert Path('/sys/fs/cgroup/cgroup.subtree_control').read_text() == global_control
    result['owned_cgroup_removed'] = True
    (work/'namespace-admission.json').write_text(json.dumps(result, indent=2)+'\n')


if __name__ == '__main__':
    if not (sys.flags.isolated and sys.flags.no_site and sys.flags.dont_write_bytecode):
        raise SystemExit('requires /usr/bin/python3 -I -B -S')
    if sys.argv[1:] == ['--prepare-image']:
        prepare_image()
    elif sys.argv[1:] == ['--probe']:
        probe()
    else:
        raise SystemExit('--prepare-image | --probe')
