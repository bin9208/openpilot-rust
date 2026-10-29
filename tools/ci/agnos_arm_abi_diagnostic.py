#!/usr/bin/env python3
"""Reproduce the failed native PRoot loader audit without deep input preparation."""
import hashlib
import json
import lzma
import os
from pathlib import Path
import platform
import runpy
import shutil
import subprocess
import tempfile
import time

SOURCE = 'f62e361d59afa2e12125d416f372a1042bcbdfb9'


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def main():
    assert os.environ['GITHUB_ACTIONS'] == 'true'
    assert os.environ['RUNNER_ENVIRONMENT'] == 'github-hosted'
    assert os.environ['RUNNER_ARCH'] == 'ARM64' and platform.machine() == 'aarch64'
    cpus = sorted(os.sched_getaffinity(0))
    assert len(cpus) > 1
    work = Path(tempfile.mkdtemp(prefix='agnos-arm-abi-', dir=os.environ['RUNNER_TEMP']))
    with open(os.environ['GITHUB_ENV'], 'a') as stream:
        stream.write(f'AGNOS_ABI_REPORT={work}\n')
    assert shutil.disk_usage(work).free >= 16 * 1024**3
    cache = Path(os.environ['AGNOS_ABI_CACHE'])
    cache.mkdir(parents=True, exist_ok=True)
    report = dict(source_commit=SOURCE, workflow_commit=os.environ['GITHUB_SHA'],
                  allowed_cpus=cpus, diagnostic_only=True, full_runtime_build=False, commands=[])

    def run(argv, name, *, required=True, env=None, timeout=600):
        print('START ' + name, flush=True)
        started = time.monotonic()
        out, err = work/(name+'.stdout'), work/(name+'.stderr')
        with out.open('wb') as stdout, err.open('wb') as stderr:
            result = subprocess.run(list(map(str, argv)), stdout=stdout, stderr=stderr,
                                    env=env, timeout=timeout, check=False)
        row = dict(name=name, argv=list(map(str, argv)), exit_code=result.returncode,
                   elapsed_seconds=time.monotonic()-started)
        report['commands'].append(row)
        print(json.dumps(row), flush=True)
        if result.returncode and required:
            print(err.read_text(errors='replace')[-12000:], flush=True)
            raise RuntimeError(name+' failed')
        return result.returncode

    def download(url, path, sha):
        if path.exists() and digest(path) == sha:
            return
        run(['curl', '--fail', '--location', '--retry', '5', '--retry-all-errors',
             '--connect-timeout', '20', '--max-time', '1800', '--output', path, url],
            path.name+'-download', timeout=1900)
        assert digest(path) == sha

    try:
        source = work/'source'
        env = dict(os.environ, GIT_LFS_SKIP_SMUDGE='1')
        run(['git', 'init', source], 'source-init')
        run(['git', '-C', source, 'remote', 'add', 'origin',
             'https://github.com/bin9208/openpilot-rust.git'], 'source-origin')
        run(['git', '-C', source, 'fetch', '--depth=1', 'origin', SOURCE], 'source-fetch', env=env)
        run(['git', '-C', source, 'checkout', '--detach', SOURCE], 'source-checkout', env=env)
        pins = runpy.run_path(str(source/'tools/ci/probe_native_agnos.py'))
        image_plan = json.loads((source/'tools/ci/fixtures/native-agnos-assets-599.json').read_text())
        asset = image_plan['assets']['agnos-system-19-8-carrot-bt1']
        image = cache/'system.img.xz'
        download(asset['url'], image, asset['sha256'])
        assert image.stat().st_size == asset['bytes']
        sparse, raw, rootfs = work/'system.sparse', work/'system.raw', work/'rootfs'
        with lzma.open(image, 'rb') as src, sparse.open('wb') as dst:
            copied = 0
            while block := src.read(1024**2):
                copied += len(block)
                assert copied <= pins['RAW_BYTES'] + 64*1024**2
                dst.write(block)
        assert digest(sparse) == pins['SPARSE_SHA']
        run(['simg2img', sparse, raw], 'sparse-to-raw')
        assert raw.stat().st_size == pins['RAW_BYTES'] and digest(raw) == pins['RAW_SHA']
        sparse.unlink()
        rootfs.mkdir()
        run(['debugfs', '-R', f'rdump / {rootfs}', raw], 'extract-image')
        raw.unlink()
        assert (rootfs/'VERSION').read_text().strip() == '19.8-carrot-bt1'
        rustup = cache/'rustup-init'
        download('https://static.rust-lang.org/rustup/archive/1.29.1/aarch64-unknown-linux-gnu/rustup-init',
                 rustup, pins['RUSTUP_SHA'])
        rustup.chmod(0o700)
        for name in ('cargo', 'rustup'):
            (cache/name).mkdir(exist_ok=True)
        for path in ('/diag', '/source'):
            (rootfs/path.lstrip('/')).mkdir()
        (work/'gitconfig').write_text('[safe]\n\tdirectory = /source\n[core]\n\thooksPath = /dev/null\n')
        proot = Path(os.environ['AGNOS_PROOT_PATH'])
        report['proot_sha256'] = digest(proot)
        report['proot_source_commit'] = '25dc6a3134891f98a79f57ce1c2c1b23ff15cad1'
        report['proot_patch_sha256'] = os.environ.get('AGNOS_PROOT_PATCH_SHA256')
        guest = [proot, '-r', rootfs, '-b', '/proc', '-b', '/dev', '-b', '/etc/resolv.conf',
                 '-b', '/etc/hosts', '-b', f'{work}:/diag', '-b', f'{source}:/source',
                 '-b', f'{cache}/cargo:/diag/cargo', '-b', f'{cache}/rustup:/diag/rustup',
                 '-b', f'{rustup}:/diag/rustup-init', '-w', '/diag', '/usr/bin/env', '-i',
                 'HOME=/root', 'CARGO_HOME=/diag/cargo', 'RUSTUP_HOME=/diag/rustup',
                 'RUSTUP_TOOLCHAIN=1.94.0', 'CARGO_INCREMENTAL=0', 'CARGO_PROFILE_DEV_DEBUG=0',
                 f'CARGO_BUILD_JOBS={len(cpus)}', 'GIT_CONFIG_NOSYSTEM=1',
                 'GIT_CONFIG_GLOBAL=/diag/gitconfig',
                 'PATH=/diag/cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin']
        if not (cache/'rustup/toolchains/1.94.0-aarch64-unknown-linux-gnu/bin/rustc').exists():
            run([*guest, '/diag/rustup-init', '-y', '--default-toolchain', '1.94.0',
                 '--profile', 'minimal', '--no-modify-path'], 'rust-setup')
        run([*guest, 'cargo', 'fetch', '--locked', '--manifest-path', '/source/rust/Cargo.toml',
             '--target', 'aarch64-unknown-linux-gnu'], 'locked-fetch')
        run([*guest, 'cargo', 'build', '--locked', '--offline', '--manifest-path', '/source/rust/Cargo.toml',
             '-p', 'openpilot-runtime-build', '--bin', 'openpilot-runtime-inputs',
             '--target-dir', '/diag/target'], 'build-original-seed')
        binary = '/diag/target/debug/openpilot-runtime-inputs'
        inspect = ['/usr/bin/env', '-i', 'PATH=/usr/bin:/bin', 'LC_ALL=C']
        cases = [
            ('readelf', ['/usr/bin/readelf', '--wide', '-l', binary]),
            ('seed-help', [binary, '--help']),
            ('ldd-seed', ['/usr/bin/ldd', binary]),
            ('ldd-trace', ['/bin/bash', '-x', '/usr/bin/ldd', binary]),
            ('loader-verify', ['/lib/ld-linux-aarch64.so.1', '--verify', binary]),
            ('loader-list', ['/lib/ld-linux-aarch64.so.1', '--list', binary]),
            ('default-loader-trace', ['/usr/bin/env', 'LD_TRACE_LOADED_OBJECTS=1', binary]),
            ('ldd-system', ['/usr/bin/ldd', '/bin/true']),
            ('seed-abi', ['/usr/bin/python3', '-c',
                "import json,runpy; from pathlib import Path; "
                "m=runpy.run_path('/source/tools/native/bootstrap_seed.py'); "
                f"print(json.dumps(m['inspect_abi'](Path('{binary}'))))"]),
            ('permission-contract', ['/usr/bin/python3', '-c',
                "import os; from pathlib import Path; "
                f"assert os.access('{binary}',os.R_OK); "
                "p=Path('/diag/permission-'+str(os.getpid())); p.write_bytes(b'blocked'); p.chmod(0); "
                "assert not os.access(p,os.R_OK); p.chmod(0o600); p.unlink(); "
                "assert not os.access('/diag/missing-file',os.R_OK); print('real read permissions preserved')"]),
        ]
        for prefix, outer_env in [('normal', None), ('no-seccomp', dict(os.environ, PROOT_NO_SECCOMP='1'))]:
            for name, args in cases:
                run([*guest, *inspect, *args], prefix+'-'+name,
                    required=False, env=outer_env, timeout=30)
        # A separate owned extraction, same trusted source-built bytes, and the
        # original loader/tool inside a real chroot distinguish PRoot behavior.
        shutil.copy2(work/'target/debug/openpilot-runtime-inputs', rootfs/'tmp/diagnostic-seed')
        run(['sudo', '-n', 'chroot', rootfs, *inspect, '/usr/bin/ldd', '/tmp/diagnostic-seed'],
            'chroot-ldd-seed', required=False, timeout=30)
        run(['sudo', '-n', 'chroot', rootfs, *inspect, '/lib/ld-linux-aarch64.so.1',
             '--list', '/tmp/diagnostic-seed'], 'chroot-loader-list', required=False, timeout=30)
        if report['proot_patch_sha256']:
            assert all(row['exit_code'] == 0 for row in report['commands']), 'patched loader or permission contract failed'
    finally:
        (work/'report.json').write_text(json.dumps(report, indent=2)+'\n')


if __name__ == '__main__':
    main()
