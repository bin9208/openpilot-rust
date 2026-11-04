"""Pinned, static native ARM PRoot for the existing AGNOS CI guest commands only."""
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import shutil
import stat
import tarfile

PROOT_COMMIT = '25dc6a3134891f98a79f57ce1c2c1b23ff15cad1'
UTHASH_COMMIT = 'e493aa90a2833b4655927598f169c31cfcdf7861'
SOURCES = {
    'proot': dict(commit=PROOT_COMMIT, bytes=253615,
        sha256='6adb0d808dad47d66f503d6d5a699b455ac1a0a519b905378be4ffc0af92f40e',
        url='https://codeload.github.com/proot-me/proot/tar.gz/'+PROOT_COMMIT),
    'uthash': dict(commit=UTHASH_COMMIT, bytes=191431,
        sha256='2ddb6665e82ffd7e87a27b97081bddf8f09520b7a2cfd6ab011541e7cde991fa',
        url='https://codeload.github.com/proot-me/uthash/tar.gz/'+UTHASH_COMMIT),
}
ARM64_FACCESSAT2_PATCH = dict(path='src/syscall/sysnums-arm64.h',
    before_sha256='86a55e5ae211a0838a45a23aa9d8889e41b9b8e6de56df0440d6a72f3ae9e253',
    after_sha256='b850a9a4390554f6b6ec3d3454ab4f7039d1e93006dcbd6be7e5ecd63c9841b1')
MODE = 'native-arm-proot'
GUEST_WORK = '/openpilot-native-build-probe-599'
SOURCE_MOUNT = '/openpilot-native-source-599'


def regular(path):
    if path.resolve(strict=True) != path:
        raise ValueError('native PRoot input is redirected')
    stream = os.fdopen(os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK), 'rb')
    if not stat.S_ISREG(os.fstat(stream.fileno()).st_mode):
        stream.close()
        raise ValueError('native PRoot input must be regular')
    return stream


def file_pin(path):
    with regular(path) as stream:
        return dict(bytes=os.fstat(stream.fileno()).st_size,
                    sha256=hashlib.file_digest(stream, 'sha256').hexdigest())


def static_arm_elf(path):
    with regular(path) as stream:
        info = os.fstat(stream.fileno())
        size = info.st_size
        if stat.S_IMODE(info.st_mode) != 0o555:
            raise ValueError('native PRoot executable mode changed')
        header = stream.read(64)
        if (len(header) != 64 or header[:7] != b'\x7fELF\x02\x01\x01'
                or int.from_bytes(header[18:20], 'little') != 183):
            raise ValueError('native PRoot requires a 64-bit little-endian ARM ELF')
        offset = int.from_bytes(header[32:40], 'little')
        width = int.from_bytes(header[54:56], 'little')
        count = int.from_bytes(header[56:58], 'little')
        if width != 56 or not 0 < count <= 4096 or offset+count*width > size:
            raise ValueError('native PRoot ELF program headers are invalid')
        for number in range(count):
            stream.seek(offset+number*width)
            row = stream.read(width)
            kind = int.from_bytes(row[:4], 'little')
            if kind == 3:
                raise ValueError('native PRoot must not contain PT_INTERP')
            if kind == 2:
                start = int.from_bytes(row[8:16], 'little')
                length = int.from_bytes(row[32:40], 'little')
                if start+length > size or length % 16:
                    raise ValueError('native PRoot ELF dynamic table is invalid')
                stream.seek(start)
                for _ in range(length//16):
                    tag = int.from_bytes(stream.read(16)[:8], 'little')
                    if tag == 0:
                        break
                    if tag == 1:
                        raise ValueError('native PRoot must not contain DT_NEEDED')
    return dict(elf_machine=183, static=True, **file_pin(path))


def build_argv(work, affinity):
    if (not isinstance(affinity, list) or not affinity
            or any(type(cpu) is not int or cpu < 0 for cpu in affinity)
            or affinity != sorted(set(affinity))):
        raise ValueError('native PRoot build affinity is invalid')
    return ['/usr/bin/make', '-C', str(work/'proot-native-source'/('proot-'+PROOT_COMMIT)/'src'),
            '-j'+str(len(affinity)), 'proot', 'WITHOUT_PYTHON=1']


def build_environment(work):
    # Makefile += retains its own -ltalloc when LDFLAGS comes from the environment.
    return dict(PATH='/usr/bin:/bin', LC_ALL='C', HOME=str(work), LDFLAGS='-static')


def unpack(work, name, download, log):
    pin = SOURCES[name]
    archive = work/(name+'-source.tar.gz')
    download(pin['url'], archive, pin['sha256'], log)
    if file_pin(archive) != {k: pin[k] for k in ('bytes', 'sha256')}:
        raise ValueError('native PRoot source archive differs from its pin: '+name)
    destination = work/(name+'-native-source')
    destination.mkdir()
    prefix = name+'-'+pin['commit']
    with tarfile.open(archive, 'r:gz') as bundle:
        members = bundle.getmembers()
        if len(members) > 4096 or sum(m.size for m in members) > 8*1024**2:
            raise ValueError('native PRoot source archive exceeds its extraction bound')
        for member in members:
            path = PurePosixPath(member.name)
            if path.is_absolute() or '..' in path.parts or not path.parts or path.parts[0] != prefix:
                raise ValueError('native PRoot source archive path escapes its root')
            if member.issym():
                if name != 'uthash' or path != PurePosixPath(prefix, 'include') or member.linkname != 'src':
                    raise ValueError('unexpected native PRoot source symlink')
            elif not (member.isdir() or member.isfile()):
                raise ValueError('unexpected native PRoot source member type')
        bundle.extractall(destination, members=members, filter='data')
    return destination/prefix


def patch_arm64_faccessat2(source):
    # 고정 v5.5.0의 누락된 ARM64 커널 syscall 명칭 한 행만 복구한다.
    path = source/ARM64_FACCESSAT2_PATCH['path']
    if path.resolve(strict=True) != path:
        raise ValueError('native PRoot syscall header is redirected')
    with os.fdopen(os.open(path, os.O_RDWR | os.O_NOFOLLOW | os.O_NONBLOCK), 'r+b') as stream:
        info = os.fstat(stream.fileno())
        if not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid() or info.st_nlink != 1:
            raise ValueError('native PRoot syscall header must be an owned regular file with one link')
        original = stream.read()
        anchor = b'    [435] = PR_clone3,\n'
        entry = b'    [439] = PR_faccessat2,\n'
        if (hashlib.sha256(original).hexdigest() != ARM64_FACCESSAT2_PATCH['before_sha256']
                or original.count(anchor) != 1 or b'[439]' in original):
            raise ValueError('native PRoot syscall header differs from the pinned original')
        patched = original.replace(anchor, anchor+entry, 1)
        if (patched.count(entry) != 1
                or hashlib.sha256(patched).hexdigest() != ARM64_FACCESSAT2_PATCH['after_sha256']):
            raise ValueError('native PRoot syscall patch differs from its fixed result')
        stream.seek(0)
        stream.write(patched)
        stream.truncate()
        stream.flush()
        os.fsync(stream.fileno())
    after = path.lstat()
    if ((after.st_dev, after.st_ino, after.st_uid, after.st_gid, after.st_mode, after.st_nlink)
            != (info.st_dev, info.st_ino, info.st_uid, info.st_gid, info.st_mode, info.st_nlink)
            or file_pin(path)['sha256'] != ARM64_FACCESSAT2_PATCH['after_sha256']):
        raise ValueError('native PRoot patched header ownership or bytes changed')
    return dict(ARM64_FACCESSAT2_PATCH)


def build(work, log, download, run):
    if (os.uname().machine != 'aarch64' or os.geteuid() == 0
            or work.resolve(strict=True) != work or work.stat().st_uid != os.geteuid()
            or work.parent != Path(os.environ['CI_JOB_TEMP']) or work.name != 'native-agnos'):
        raise ValueError('native PRoot build requires a real unprivileged ARM host and owned work')
    # Fail with real CI evidence if Ubuntu does not supply this static library.
    talloc = Path('/usr/lib/aarch64-linux-gnu/libtalloc.a')
    talloc_pin = file_pin(talloc)
    source = unpack(work, 'proot', download, log)
    source_patch = patch_arm64_faccessat2(source)
    uthash = unpack(work, 'uthash', download, log)
    target = source/'lib/uthash'
    if target.is_symlink() or target.parent.resolve(strict=True) != source/'lib':
        raise ValueError('native PRoot gitlink directory is redirected')
    target.mkdir(exist_ok=True)
    if not target.is_dir() or any(target.iterdir()):
        raise ValueError('native PRoot gitlink directory is not empty')
    target.rmdir()
    uthash.rename(target)
    affinity = sorted(os.sched_getaffinity(0))
    argv, environment = build_argv(work, affinity), build_environment(work)
    result = run(argv, log, env=environment)
    if (result['exit_code'] != 0 or file_pin(talloc) != talloc_pin
            or file_pin(source/source_patch['path'])['sha256'] != source_patch['after_sha256']):
        raise ValueError('native PRoot build failed or its static talloc changed')
    binary = work/'proot'
    with regular(source/'src/proot') as stream, binary.open('xb') as output:
        shutil.copyfileobj(stream, output)
    binary.chmod(0o555)
    proof = dict(schema=1, source_commit=os.environ['GITHUB_SHA'], execution_mode=MODE,
        host_architecture='aarch64', sources=SOURCES, source_patches=[source_patch], build_affinity=affinity,
        build_argv=argv, build_environment=environment,
        build_exit_code=result['exit_code'], static_talloc=dict(path=str(talloc), **talloc_pin),
        executable=dict(path=str(binary), **static_arm_elf(binary)))
    receipt = work/'native-bootstrap-proot-build.json'
    with receipt.open('x') as stream:
        json.dump(proof, stream, indent=2)
        stream.write('\n')
    receipt.chmod(0o444)
    return dict(proot_sha256=proof['executable']['sha256'], proot_build_receipt_sha256=file_pin(receipt)['sha256'],
                execution_mode=MODE, proot_source_sha256=SOURCES['proot']['sha256'],
                uthash_source_sha256=SOURCES['uthash']['sha256'])


def expected_guest_command(work):
    # Reconstruct from the trusted Actions paths, never from context-supplied argv.
    owned = Path(os.environ['CI_JOB_TEMP'])
    source = Path(os.environ['GITHUB_WORKSPACE']).resolve(strict=True)
    if (owned.resolve(strict=True) != owned or work != owned/'native-agnos'
            or work.resolve(strict=True) != work):
        raise ValueError('native PRoot guest paths differ from the owned Actions job')
    proot, rootfs = work/'proot', work/'rootfs'
    guest_work, source_mount = GUEST_WORK, SOURCE_MOUNT
    guest = [proot, '-r', rootfs, '-b', '/proc', '-b', '/dev', '-b', '/etc/resolv.conf',
             '-b', '/etc/hosts', '-b', f'{work}:{guest_work}', '-b', f'{owned / "native-rust-toolchains"}:{guest_work}/pinned-rust',
             '-b', f'{owned / "native-capnp"}:{guest_work}/capnp-sdk',
             '-b', f'{owned / "native-capnp-assets"}:{guest_work}/capnp-assets',
             '-b', f'{owned / "native-cargo-assets"}:{guest_work}/cargo-assets',
             '-b', f'{owned / "native-cargo-sources"}:{guest_work}/cargo-sources',
             '-b', f'{owned / "native-rust-assets"}:{guest_work}/rust-assets',
             '-b', f'{owned / "native-codec-assets"}:{guest_work}/codec-assets',
             '-b', f'{owned / "native-eigen-assets"}:{guest_work}/eigen-assets',
             '-b', f'{owned / "native-ffmpeg-source"}:{guest_work}/ffmpeg-source',
             '-b', f'{owned / "native-provider-components"}:{guest_work}/provider-assets',
             '-b', f'{owned / "native-provider-export"}:{guest_work}/provider-export',
             '-b', f'{owned / "native-companion-components"}:{guest_work}/companion-components',
             '-b', f'{owned / "native-companion-exports"}:{guest_work}/companion-exports',
             '-b', f'{owned / "native-source-lfs-assets"}:{guest_work}/source-lfs-assets',
             '-b', f'{owned / "native-python-assets"}:{guest_work}/python-assets',
             '-b', f'{owned / "native-model-wheel-assets"}:{guest_work}/model-wheel-assets',
             '-b', f'{owned / "native-model-assets"}:{guest_work}/model-assets',
             '-b', f'{owned / "native-mpc-components"}:{guest_work}/mpc-components',
             '-b', f'{owned / "native-mpc-export"}:{guest_work}/mpc-export',
             '-b', f'{owned / "native-panda-components"}:{guest_work}/panda-components',
             '-b', f'{owned / "native-panda-export"}:{guest_work}/panda-export',
             '-b', f'{owned / "native-dbc-components"}:{guest_work}/dbc-components',
             '-b', f'{owned / "native-dbc-export"}:{guest_work}/dbc-export',
             '-b', f'{owned / "native-usbgpu-components"}:{guest_work}/usbgpu-components',
             '-b', f'{owned / "native-usbgpu-export"}:{guest_work}/usbgpu-export',
             '-b', f'{owned / "native-usbgpu-model-assets"}:{guest_work}/usbgpu-model-assets',
             '-b', f'{owned / "native-raylib-assets"}:{guest_work}/raylib-assets',
             '-b', f'{owned / "native-raylib-components"}:{guest_work}/raylib-components',
             '-b', f'{owned / "native-raylib-provider-export"}:{guest_work}/raylib-provider-export',
             '-b', f'{owned / "native-opencv-source"}:{guest_work}/opencv-source',
             '-b', f'{owned / "native-opencv-components"}:{guest_work}/opencv-components',
             '-b', f'{owned / "native-opencv-provider-export"}:{guest_work}/opencv-provider-export',
             '-b', f'{source}:{source_mount}', '-w', guest_work, '/usr/bin/env', '-i', 'HOME=/root',
             f'CARGO_HOME={guest_work}/cargo', f'RUSTUP_HOME={guest_work}/rustup', 'CARGO_INCREMENTAL=0',
             'CARGO_PROFILE_DEV_DEBUG=0', 'CARGO_PROFILE_TEST_DEBUG=0', 'GIT_CONFIG_NOSYSTEM=1',
             f'GIT_CONFIG_GLOBAL={guest_work}/probe.gitconfig', 'GIT_OPTIONAL_LOCKS=0',
             f'PATH={guest_work}/cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin']
    return list(map(str, guest))


def verify_context(context, work):
    if (context['schema'] != 1 or context['source_commit'] != os.environ['GITHUB_SHA']
            or context['work'] != str(work) or os.uname().machine != 'aarch64'
            or context['execution_mode'] != MODE or context['guest_work'] != GUEST_WORK
            or context['source_mount'] != SOURCE_MOUNT
            or context['proot_source_sha256'] != SOURCES['proot']['sha256']
            or context['uthash_source_sha256'] != SOURCES['uthash']['sha256']):
        raise ValueError('native PRoot context differs from its source, host or pinned inputs')
    receipt = work/'native-bootstrap-proot-build.json'
    if file_pin(receipt)['sha256'] != context['proot_build_receipt_sha256']:
        raise ValueError('native PRoot build receipt changed')
    with regular(receipt) as stream:
        proof = json.load(stream)
    expected = dict(path=str(work/'proot'), **static_arm_elf(work/'proot'))
    if (proof['schema'] != 1 or proof['source_commit'] != context['source_commit']
            or proof['execution_mode'] != MODE or proof['host_architecture'] != 'aarch64'
            or proof['sources'] != SOURCES or proof['build_argv'] != build_argv(work, proof['build_affinity'])
            or proof['build_environment'] != build_environment(work) or proof['build_exit_code'] != 0
            or proof['executable'] != expected or context['proot_sha256'] != expected['sha256']):
        raise ValueError('native PRoot executable or build provenance changed')
    header = work/'proot-native-source'/('proot-'+PROOT_COMMIT)/ARM64_FACCESSAT2_PATCH['path']
    if (proof.get('source_patches') != [ARM64_FACCESSAT2_PATCH]
            or file_pin(header)['sha256'] != ARM64_FACCESSAT2_PATCH['after_sha256']
            or header.stat().st_uid != os.getuid() or header.stat().st_nlink != 1):
        raise ValueError('native PRoot ARM64 syscall patch provenance changed')
    for name, pin in SOURCES.items():
        if file_pin(work/(name+'-source.tar.gz')) != {k: pin[k] for k in ('bytes', 'sha256')}:
            raise ValueError('native PRoot source archive changed: '+name)
    talloc = proof['static_talloc']
    if (talloc['path'] != '/usr/lib/aarch64-linux-gnu/libtalloc.a'
            or file_pin(Path(talloc['path'])) != {k: talloc[k] for k in ('bytes', 'sha256')}):
        raise ValueError('native PRoot static talloc input changed')
    guest = context['guest']
    if not isinstance(guest, list) or not all(isinstance(value, str) for value in guest):
        raise ValueError('native PRoot guest command is not a string list')
    if guest != expected_guest_command(work):
        raise ValueError('native PRoot guest command differs from the fixed bind/environment structure')
    return guest
