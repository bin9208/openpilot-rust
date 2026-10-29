#!/usr/bin/env python3
"""Actions-only full production entry check, using real factory ARM credentials.

No fixture index, provider archive, sudo replacement, or cross-job Cargo target
is accepted. Hardware startup is deliberately outside --verify-package.
"""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import select
import signal
import shutil
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[2]
FIELDS = {'schema', 'maximum_memory_bytes', 'minimum_available_memory_bytes',
          'maximum_growth_bytes', 'minimum_free_bytes', 'expected_source_growth_bytes',
          'expected_compile_growth_bytes', 'remaining_object_growth_bytes',
          'compiler_growth_bytes', 'build_timeout_seconds', 'budgets'}


def module(path, name):
    spec = importlib.util.spec_from_file_location(name, path)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


def checksum(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def save(path, value):
    with path.open('x') as stream:
        json.dump(value, stream, indent=2)
        stream.write('\n')


def policy(raw, source):
    entry = module(source/'tools/native/source_entry.py', 'entry_policy')
    value = json.loads(raw, object_pairs_hook=entry.runtime.pairs)
    if (set(value) != FIELDS or type(value['schema']) is not int or value['schema'] != 1
            or any(not entry.positive(value[k]) for k in FIELDS-{'schema', 'budgets'})
            or not 120 <= value['build_timeout_seconds'] <= 18000):
        raise ValueError('explicit, bounded operation estimates are required')
    catalog, _ = entry.git_json(source, 'rust/native-runtime-catalog.json')
    budgets = value['budgets']
    if (set(budgets) != {'schema', 'group_growth_bytes', 'expected_source_growth_bytes',
                         'remaining_object_growth_bytes', 'estimate_basis'}
            or type(budgets['schema']) is not int or budgets['schema'] != 1
            or set(budgets['group_growth_bytes']) != set(catalog['groups'])
            or any(not entry.positive(v) for v in budgets['group_growth_bytes'].values())
            or any(not entry.positive(budgets[k]) for k in ('expected_source_growth_bytes', 'remaining_object_growth_bytes'))
            or not isinstance(budgets['estimate_basis'], str)
            or not 1 <= len(budgets['estimate_basis'].strip()) <= 4096):
        raise ValueError('explicit operation estimates must cover every current runtime group')
    return value


def preflight():
    transport = module(ROOT/'tools/ci/probe_native_source_transport.py', 'entry_transport')
    context = transport.source_module('native_cache')['Context']()
    source = context.workspace
    entry = module(source/'tools/native/source_entry.py', 'entry_preflight')
    identity = entry.runtime.source_identity(source)
    if identity['commit'] != os.environ['GITHUB_SHA'] or not (source/'.git').is_dir():
        raise ValueError('an independent exact Actions checkout is required')
    # Read the exact committed Git blob; no candidate artifact or local fixture.
    index, pin = entry.input_index(source)
    if pin != os.environ['NATIVE_ENTRY_INDEX']:
        raise ValueError('committed public input index differs from dispatch pin')
    estimates = policy(os.environ['NATIVE_ENTRY_BUDGETS'], source)
    metadata = context.job/'source-entry-public-metadata'
    metadata.mkdir()
    transferred = {}
    assets = [('seed', index['seed']), ('system_sdk', index['system_sdk']), *index['providers'].items()]
    assert shutil.disk_usage(context.job).free >= sum(a['bytes']+65536 for _, a in assets)+estimates['minimum_free_bytes']
    for role, asset in assets:
        target = metadata/(role+'.json')
        count = entry.seed.fetch(asset, target)
        if target.stat().st_size != asset['bytes'] or checksum(target) != asset['sha256']:
            raise ValueError('public metadata body differs from committed pin: '+role)
        transferred[role] = count
        if role == 'seed':
            seed, _ = entry.seed.read_plan(target, asset['sha256'])
            if seed.get('protocols') != dict(current_tools=1, request_owner=2, generation_boot=1):
                raise ValueError('published boot seed lacks the required production protocols')
        else:
            descriptor = json.loads(target.read_bytes(), object_pairs_hook=entry.runtime.pairs)
            plan, _ = entry.git_json(source, 'rust/native-system-sdk.json' if role == 'system_sdk'
                                     else f'rust/native-{role}-provider.json')
            if role == 'system_sdk':
                if descriptor['plan'] != plan:
                    raise ValueError('public SDK descriptor differs from committed SDK plan')
            elif (descriptor['profile'] != plan['profile']
                  or descriptor['provider_source_commit'] != plan['source_commit']):
                raise ValueError('public provider descriptor differs from committed profile: '+role)
    if json.loads((source/'rust/native-system-sdk.json').read_text())['factory_image_sha256'] != transport.RAW_SHA:
        raise ValueError('factory image differs from the source SDK pin')
    save(context.job/'source-entry-request.json', dict(schema=1, source=identity,
        index_sha256=pin, seed=index['seed'], estimates=estimates,
        public_metadata_transferred_bytes=transferred))
    print(json.dumps(dict(source=identity, index_sha256=pin, public_metadata_verified=True,
        source_entry_full_build_executed=False, cross_job_cargo_cache_reused=False)))


def arm():
    import platform
    import pwd
    import grp
    assert platform.machine() == 'aarch64' and os.getuid() == os.geteuid() == 1000
    assert not os.getgroups() and Path('/VERSION').read_text().strip() == '19.8-carrot-bt1'
    assert pwd.getpwuid(1000).pw_name == 'comma' and pwd.getpwuid(1000).pw_gid == os.getgid()
    assert grp.getgrgid(os.getgid()).gr_name == 'comma'
    source, state = Path('/native/source'), Path('/native/state')
    request = json.loads((state/'request.json').read_text())
    estimates = request['estimates']
    transport = module(source/'tools/ci/probe_native_source_transport.py', 'factory_transport')
    # Require the unchanged factory sudoers to authorize the actual original uid.
    with (state/'sudoers.stdout').open('xb') as out, (state/'sudoers.stderr').open('xb') as err:
        subprocess.run(['/usr/bin/sudo', '-n', '-l'], stdout=out, stderr=err, check=True, timeout=30)
    completed = subprocess.run(['/usr/bin/python3', '-I', '-B', '-S', '-c',
        transport.USER_PROBE, '1000', transport.ROOT_PROBE], capture_output=True, text=True,
        check=True, timeout=120)
    admission = json.loads(completed.stdout)
    assert admission['factory_sudo_sha256'] == request['factory_sudo']['sha256']
    save(state/'factory-admission.json', admission)
    entry = module(source/'tools/native/source_entry.py', 'factory_entry')
    assert entry.runtime.source_identity(source) == request['source']
    index, pin = entry.input_index(source)
    assert pin == request['index_sha256'] and index['seed'] == request['seed']
    stores = {name: str(state/name) for name in ('sources', 'objects', 'generations', 'params')}
    for name in (*stores, 'cache', 'seed', 'updater'):
        (state/name).mkdir()
    (state/'params/d').mkdir()
    (state/'params/d/IsOnroad').write_bytes(b'0')
    machine = dict(schema=1, stores=stores, cache=str(state/'cache'), seed_cache=str(state/'seed'),
        updater_staging=str(state/'updater'), budgets=estimates['budgets'], boot_seed=index['seed'],
        **{k: estimates[k] for k in ('expected_source_growth_bytes', 'expected_compile_growth_bytes',
                                   'remaining_object_growth_bytes', 'compiler_growth_bytes')},
        resources=dict(schema=1, memory_cgroup_parent='/sys/fs/cgroup',
            maximum_memory_bytes=estimates['maximum_memory_bytes'],
            minimum_available_memory_bytes=estimates['minimum_available_memory_bytes'],
            filesystems={str(state): {k: estimates[k] for k in ('maximum_growth_bytes', 'minimum_free_bytes')}}))
    machine_path = state/'machine.json'
    save(machine_path, machine)
    entry.validate_machine(machine, source)
    machine_pin = checksum(machine_path)
    paths = state/'launch-paths.json'
    save(paths, dict(data=str(state), temporary='/tmp', system='/'))
    base = ['/usr/bin/python3', '-I', '-B', '-S', str(source/'tools/native/source_entry.py'),
        '--source', str(source), '--machine-config', str(machine_path),
        '--machine-configuration-sha256', machine_pin]
    rows = {}
    for phase in ('initial', 'reused', 'launch-package'):
        arguments = base + (['--launch', '--verify-package', '--paths', str(paths)]
                            if phase == 'launch-package' else [])
        with (state/(phase+'.stdout')).open('xb') as out, (state/(phase+'.stderr')).open('xb') as err:
            subprocess.run(arguments, env=entry.seed.ENV, stdout=out, stderr=err, check=True,
                           timeout=estimates['build_timeout_seconds'])
        if phase != 'launch-package':
            row = entry.runtime.read(state/(phase+'.stdout'))
            assert row['source'] == request['source'] and row['input_index_sha256'] == pin
            assert row['full_runtime_build'] and not row['runtime_release_downloaded']
            assert not row['runtime_started'] and not row['runtime_activated'] and not row['staged_for_boot']
            native = entry.runtime.read(Path(row['prepared_configuration']), row['build_configuration_sha256'])
            assert native['cache'] == str(state/'cache/cargo-build')
            assert native['stores'] == stores
            rows[phase] = row
    # --verify-package returns before normal manager/hardware startup. Retained
    # startup config is checked by the production launcher, not manufactured here.
    startup = entry.startup_record(machine)
    sealed = entry.runtime.read(state/'generations'/(startup['initial_generation']+'.json'))
    assert sealed['runtime']['source'] == request['source']
    assert entry.runtime.source_identity(source) == request['source']
    save(state/'summary.json', dict(schema=1, source=request['source'], index_sha256=pin,
        generation_id=startup['initial_generation'], factory_sudo_sha256=admission['factory_sudo_sha256'],
        actual_arm_sudo_executed=True, source_entry_full_build_executed=True,
        production_three_sudo_build_gates_executed=True, production_launch_verify_package_executed=True,
        generation_ids={k: row['generation_id'] for k, row in rows.items()},
        cargo_cache_identity_path=str(state/'cache/cargo-build'),
        public_metadata_verified=True, warm_same_path_entry_completed=True,
        public_metadata_preflight_transferred_bytes=request['public_metadata_transferred_bytes'],
        cross_job_cargo_cache_reused=False, warm_zero_download_claimed=False,
        docs_and_small_change_reuse_tested=False, update_and_recovery_tested=False,
        physical_arm_cpu=False, agnos_kernel=False, proot_used=False,
        manager_started=False, hardware_model_record_upload_tested=False,
        resource_policy_is_ci_fixture_not_device_budget=True))


def namespace_child(transport, uid, gid, work, factory, source, ready, go, group, inode, write):
    # This copy of namespace wiring is intentionally isolated from the admission
    # helper. See the scoped helper-extension proposal in the accompanying note.
    assert group.stat().st_ino == inode
    write(group/'cgroup.procs', os.getpid())
    transport.unshare(transport.NEWUSER)
    os.write(ready, b'U')
    assert os.read(go, 1) == b'M'
    assert os.getuid() == os.geteuid() == 0
    transport.unshare(transport.NEWNS | transport.NEWCGROUP)
    mount = lambda *args: transport.command('/usr/bin/mount', *args)
    mount('--make-rprivate', '/')
    registry, memory = work/'binfmt', work/'memory'
    registry.mkdir()
    memory.mkdir()
    mount('-t', 'binfmt_misc', 'binfmt_misc', registry)
    assert {p.name for p in registry.iterdir()} == {'status', 'register'}
    magic = r'\x7fELF\x02\x01\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x02\x00\xb7\x00'
    mask = r'\xff\xff\xff\xff\xff\xff\xff\x00\xff\xff\xff\xff\xff\xff\xff\xff\xfe\xff\xff\xff'
    (registry/'register').write_text(':owned-arm:M::'+magic+':'+mask+':/usr/bin/qemu-aarch64-static:CF')
    assert 'flags: OCF' in (registry/'owned-arm').read_text()
    mount('-t', 'cgroup2', 'cgroup2', memory)
    assert memory.stat().st_ino == inode
    (memory/'control').mkdir()
    write(memory/'control/cgroup.procs', os.getpid())
    write(memory/'cgroup.subtree_control', '+memory')
    for name in ('dev', 'proc', 'sys'):
        mount('-t', 'tmpfs', 'tmpfs', factory/name)
    mount('--bind', '/proc', factory/'proc')
    mount('-o', 'remount,bind,ro', factory/'proc')
    for name in ('null', 'zero', 'random', 'urandom'):
        target = factory/'dev'/name
        target.touch()
        mount('--bind', Path('/dev')/name, target)
    (factory/'dev/pts').mkdir()
    mount('-t', 'devpts', '-o', 'newinstance,ptmxmode=0666,mode=0620,gid=5', 'devpts', factory/'dev/pts')
    (factory/'dev/ptmx').symlink_to('pts/ptmx')
    (factory/'sys/fs/cgroup').mkdir(parents=True)
    mount('--bind', memory, factory/'sys/fs/cgroup')
    (factory/'native').mkdir()
    for name, path in (('source', source), ('state', work/'state')):
        target = factory/'native'/name
        target.mkdir()
        mount('--bind', path, target)
        if name == 'source':
            mount('-o', 'remount,bind,ro', target)
    os.chroot(factory)
    os.chdir('/')
    os.setgroups([])
    os.setgid(gid)
    os.setuid(uid)
    os.execve('/usr/bin/python3', ['/usr/bin/python3', '-I', '-B', '-S',
        '/native/source/tools/ci/verify_native_source_entry.py', '--inside-factory'],
        {'PATH': '/usr/sbin:/usr/bin:/sbin:/bin', 'LC_ALL': 'C', 'HOME': '/home/comma'})


def run():
    transport = module(ROOT/'tools/ci/probe_native_source_transport.py', 'entry_transport')
    scope_module = transport.source_module('native_resource_scope')
    uid, gid, job, scope, version = scope_module['owned']()
    assert version == 2 and uid == gid == 1000
    request = json.loads((job/'source-entry-request.json').read_text())
    source = Path(os.environ['GITHUB_WORKSPACE'])
    assert source == ROOT and source.resolve() == source
    # Git is checked again by the actual uid1000 factory caller before sudo,
    # avoiding root safe.directory overrides for the runner-owned checkout.
    assert request['source']['commit'] == os.environ['GITHUB_SHA']
    raw = job/'native-agnos/system.raw'
    assert raw.resolve(strict=True) == raw and raw.stat().st_uid == uid
    assert raw.stat().st_size == transport.RAW_BYTES and checksum(raw) == transport.RAW_SHA
    qemu_sha = transport.static_emulator()
    expected = request['estimates']['maximum_growth_bytes']+request['estimates']['minimum_free_bytes']
    assert shutil.disk_usage(job).free >= transport.RAW_BYTES+expected
    work = job/'source-entry-full'
    work.mkdir()
    factory, state = work/'factory', work/'state'
    factory.mkdir()
    state.mkdir()
    os.chown(state, uid, gid)
    transport.command('/usr/sbin/debugfs', '-R', f'rdump / {factory}', raw, timeout=600)
    assert (factory/'VERSION').read_text().strip() == '19.8-carrot-bt1'
    request['factory_sudo'] = transport.restore_factory_sudo(raw, factory/'usr/bin/sudo')
    # Only the private extracted image's resolver is updated. The host is read.
    resolver = factory/'etc/resolv.conf'
    if resolver.is_symlink():
        resolver.unlink()
    resolver.write_bytes(Path('/etc/resolv.conf').read_bytes())
    save(state/'request.json', request)
    group = scope/'source-entry-full'
    group.mkdir()
    inode = group.stat().st_ino
    write = scope_module['write']
    original = transport.handlers()
    global_control = Path('/sys/fs/cgroup/cgroup.subtree_control').read_text()
    ready, send_ready = os.pipe()
    go, send_go = os.pipe()
    pid = os.fork()
    if pid == 0:
        os.close(ready)
        os.close(send_go)
        try:
            with (work/'namespace.stdout').open('xb') as out, (work/'namespace.stderr').open('xb') as err:
                os.dup2(out.fileno(), 1)
                os.dup2(err.fileno(), 2)
                namespace_child(transport, uid, gid, work, factory, source, send_ready, go, group, inode, write)
        except BaseException as error:
            os.write(2, f'full entry namespace: {type(error).__name__}: {error}\n'.encode())
        finally:
            os._exit(1)
    os.close(send_ready)
    os.close(go)
    pidfd = os.pidfd_open(pid)
    procfd = os.open(f'/proc/{pid}', os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    reaped = False
    try:
        assert select.select([ready], [], [], 30)[0]
        assert os.read(ready, 1) == b'U', 'new user namespace did not become ready'
        for name in ('uid_map', 'gid_map'):
            fd = os.open(name, os.O_WRONLY | os.O_NOFOLLOW, dir_fd=procfd)
            try:
                assert os.write(fd, b'0 0 65536\n') == len(b'0 0 65536\n')
            finally:
                os.close(fd)
        os.write(send_go, b'M')
        deadline = time.monotonic()+request['estimates']['build_timeout_seconds']
        while True:
            waited, status = os.waitpid(pid, os.WNOHANG)
            if waited:
                reaped = True
                break
            if time.monotonic() >= deadline:
                raise TimeoutError('full entry operation deadline exceeded; original startup deadline unchanged')
            time.sleep(0.2)
        if os.waitstatus_to_exitcode(status):
            raise RuntimeError('factory entry failed; see source-entry-full namespace and state stderr')
        result = json.loads((state/'summary.json').read_text())
        assert result['source'] == request['source'] and result['index_sha256'] == request['index_sha256']
        result.update(host_kernel=os.uname().release, host_architecture=os.uname().machine,
            factory_image_sha256=transport.RAW_SHA, qemu_sha256=qemu_sha)
        save(work/'result.json', result)
    finally:
        assert group.stat().st_ino == inode and group.parent == scope
        write(group/'cgroup.kill', 1)
        if not reaped:
            try:
                signal.pidfd_send_signal(pidfd, signal.SIGKILL)
            except ProcessLookupError:
                pass
            os.waitpid(pid, 0)
        for fd in (pidfd, procfd, ready, send_go):
            os.close(fd)
        deadline = time.monotonic()+10
        while True:
            groups = [group, *(p for p in group.rglob('*') if p.is_dir() and not p.is_symlink())]
            assert len(groups) <= 1024
            if all(not (p/'cgroup.procs').read_text().strip() for p in groups):
                for path in sorted(groups, key=lambda p: len(p.parts), reverse=True):
                    path.rmdir()
                break
            if time.monotonic() >= deadline:
                raise TimeoutError('owned full entry scope remained populated')
            time.sleep(0.05)
        assert transport.handlers() == original, 'shared binfmt changed'
        assert Path('/sys/fs/cgroup/cgroup.subtree_control').read_text() == global_control, 'shared controller changed'


if __name__ == '__main__':
    if not (sys.flags.isolated and sys.flags.no_site and sys.flags.dont_write_bytecode):
        raise SystemExit('requires /usr/bin/python3 -I -B -S')
    if sys.argv[1:] == ['--preflight']:
        preflight()
    elif sys.argv[1:] == ['--run']:
        run()
    elif sys.argv[1:] == ['--inside-factory']:
        arm()
    else:
        raise SystemExit('--preflight | --run | --inside-factory')
