#!/usr/bin/env python3
"""Compare pinned AGNOS compiler commands on GitHub ARM against recorded x86/QEMU."""
import hashlib
import json
import lzma
import os
from pathlib import Path
import platform
import resource
import shutil
import subprocess
import tempfile
import time

SOURCE = '9b8c6337b56b80b79788ebc389b6622b484fe0c7'
BASELINE_SECONDS = 213.1690385139991


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def main():
    assert os.environ['GITHUB_ACTIONS'] == 'true'
    assert os.environ['RUNNER_ENVIRONMENT'] == 'github-hosted'
    assert os.environ['RUNNER_ARCH'] == 'ARM64'
    assert platform.machine() == 'aarch64'
    cpus = sorted(os.sched_getaffinity(0))
    assert len(cpus) > 1, 'No one-job fallback: user requires automatic parallelism'
    owned = Path(tempfile.mkdtemp(prefix='hosted-arm-agnos-643-', dir=os.environ['RUNNER_TEMP']))
    cache = Path(os.environ['AGNOS_INPUT_CACHE'])
    cache.mkdir(parents=True, exist_ok=True)
    for name in ('cargo', 'rustup'):
        (cache/name).mkdir(exist_ok=True)
    with open(os.environ['GITHUB_ENV'], 'a') as stream:
        stream.write(f'AGNOS_REPORT_DIRECTORY={owned}\n')
    # Image extraction, Rust distributions, locked dependencies and fresh outputs.
    growth = 16 * 1024**3
    assert shutil.disk_usage(owned).free >= growth, 'Insufficient estimated operation growth'
    report = dict(source_commit=SOURCE, runner=os.environ['RUNNER_NAME'],
                  workflow_commit=os.environ['GITHUB_SHA'], allowed_cpus=cpus,
                  cargo_build_jobs=len(cpus), execution_mode='native ARM / PRoot; no QEMU',
                  physical_arm_cpu=True, agnos_kernel_boot=False, commands=[],
                  full_original_workflow=False, full_runtime_build=False, cache_hits=[],
                  baseline_run=40904508393, baseline_compile_seconds=BASELINE_SECONDS,
                  limitation='This timed pair covers runtime-build/plan, not the entire original workflow')
    work = owned / 'native-agnos'
    work.mkdir()
    source = owned / 'source'
    started = time.monotonic()

    def run(argv, name, env=None, required=True):
        before = resource.getrusage(resource.RUSAGE_CHILDREN)
        begin = time.monotonic()
        output = work / f'{name}.stdout'
        error = work / f'{name}.stderr'
        print(f'START {name}', flush=True)
        with output.open('wb') as out, error.open('wb') as err:
            with subprocess.Popen(list(map(str, argv)), stdout=out, stderr=err, env=env) as result:
                while True:
                    try:
                        result.wait(timeout=60)
                        break
                    except subprocess.TimeoutExpired:
                        print('COMMAND_PROGRESS ' + json.dumps(dict(name=name,
                            elapsed_seconds=time.monotonic()-begin,
                            allowed_cpus=sorted(os.sched_getaffinity(0)))), flush=True)
        after = resource.getrusage(resource.RUSAGE_CHILDREN)
        row = dict(name=name, argv=list(map(str, argv)), exit_code=result.returncode,
                   elapsed_seconds=time.monotonic()-begin,
                   child_cpu_seconds=after.ru_utime+after.ru_stime-before.ru_utime-before.ru_stime)
        report['commands'].append(row)
        print('COMMAND_RESULT ' + json.dumps({k: v for k, v in row.items() if k != 'argv'}), flush=True)
        if result.returncode and required:
            print(error.read_text(errors='replace')[-14000:], flush=True)
            print(output.read_text(errors='replace')[-4000:], flush=True)
            raise RuntimeError(f'{name} failed with exit {result.returncode}')
        return output

    def download(url, path, expected):
        if path.is_file() and digest(path) == expected:
            report['cache_hits'].append(path.name)
            print('CACHE_HIT ' + path.name, flush=True)
            return
        run(['curl', '--proto', '=https', '--tlsv1.2', '--fail', '--location', '--retry', '5',
             '--retry-all-errors', '--connect-timeout', '20', '--max-time', '1800',
             '--output', path, url], path.name+'-download')
        assert digest(path) == expected, f'Wrong SHA256 for {path.name}'

    try:
        env = dict(os.environ, GIT_LFS_SKIP_SMUDGE='1')
        run(['git', 'init', source], 'source-init', env)
        run(['git', '-C', source, 'remote', 'add', 'origin',
             'https://github.com/bin9208/openpilot-rust.git'], 'source-origin', env)
        run(['git', '-C', source, 'fetch', '--depth=1', 'origin', SOURCE], 'source-fetch', env)
        run(['git', '-C', source, 'checkout', '--detach', SOURCE], 'source-checkout', env)
        original = {}
        probe = source / 'tools/ci/probe_native_agnos.py'
        exec(compile(probe.read_bytes(), str(probe), 'exec'), original)
        asset = json.loads((source/'tools/ci/fixtures/native-agnos-assets-599.json').read_text())['assets']['agnos-system-19-8-carrot-bt1']
        image = cache/'system.img.xz'
        download(asset['url'], image, asset['sha256'])
        assert image.stat().st_size == asset['bytes']
        sparse, raw, rootfs = owned/'system.sparse', owned/'system.raw', owned/'rootfs'
        print('START verified AGNOS extraction', flush=True)
        with lzma.open(image, 'rb') as src, sparse.open('wb') as dst:
            total = 0
            while block := src.read(1024**2):
                total += len(block)
                assert total <= original['RAW_BYTES'] + 64*1024**2
                dst.write(block)
        assert digest(sparse) == original['SPARSE_SHA']
        run(['simg2img', sparse, raw], 'sparse-to-raw')
        assert raw.stat().st_size == original['RAW_BYTES'] and digest(raw) == original['RAW_SHA']
        sparse.unlink()
        rootfs.mkdir()
        run(['debugfs', '-R', f'rdump / {rootfs}', raw], 'extract-agnos')
        assert (rootfs/'VERSION').read_text().strip() == '19.8-carrot-bt1'
        raw.unlink()
        report['image_raw_sha256'] = original['RAW_SHA']
        rustup = cache/'rustup-init'
        download('https://static.rust-lang.org/rustup/archive/1.29.1/aarch64-unknown-linux-gnu/rustup-init',
                 rustup, original['RUSTUP_SHA'])
        rustup.chmod(0o700)
        shutil.copy2(rustup, work/'rustup-init')
        rustup = work/'rustup-init'
        guest_work, source_mount = '/openpilot-native-build-probe-599', '/openpilot-native-source-599'
        for path in (guest_work, source_mount):
            (rootfs/path.lstrip('/')).mkdir()
        for name in ('cargo', 'rustup'):
            (work/name).mkdir()
        (work/'probe.gitconfig').write_text(f'[safe]\n\tdirectory = {source_mount}\n[core]\n\thooksPath = /dev/null\n')
        proot = shutil.which('proot')
        assert proot
        report['proot_path'] = proot
        report['proot_sha256'] = digest(Path(proot))
        guest = [proot, '-r', rootfs, '-b', '/proc', '-b', '/dev', '-b', '/etc/resolv.conf',
                 '-b', '/etc/hosts', '-b', f'{work}:{guest_work}', '-b', f'{source}:{source_mount}',
                 '-b', f'{cache}/cargo:{guest_work}/cargo', '-b', f'{cache}/rustup:{guest_work}/rustup',
                 '-w', guest_work, '/usr/bin/env', '-i', 'HOME=/root',
                 f'CARGO_HOME={guest_work}/cargo', f'RUSTUP_HOME={guest_work}/rustup',
                 'RUSTUP_TOOLCHAIN=1.94.0',
                 'CARGO_INCREMENTAL=0', 'CARGO_PROFILE_DEV_DEBUG=0', 'CARGO_PROFILE_TEST_DEBUG=0',
                 f'CARGO_BUILD_JOBS={len(cpus)}', f'RUST_TEST_THREADS={len(cpus)}',
                 'GIT_CONFIG_NOSYSTEM=1', f'GIT_CONFIG_GLOBAL={guest_work}/probe.gitconfig',
                 'GIT_OPTIONAL_LOCKS=0',
                 f'PATH={guest_work}/cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin']

        def execute(argv, name, required=True):
            return run([*guest, *(str(a).replace(str(work), guest_work) for a in argv)], name,
                       required=required)

        for argv, name in ((['/bin/uname', '-m'], 'guest-architecture'),
                           (['getconf', 'GNU_LIBC_VERSION'], 'guest-glibc'),
                           (['gcc', '--version'], 'guest-gcc'),
                           (['g++', '--version'], 'guest-gxx')):
            result = execute(argv, name).read_text()
            print(name, result[:1000], flush=True)
            report[name] = result
        assert report['guest-architecture'].strip() == 'aarch64'
        if not (cache/'rustup/toolchains/1.94.0-aarch64-unknown-linux-gnu/bin/rustc').is_file():
            execute([rustup, '-y', '--default-toolchain', '1.94.0', '--profile', 'minimal', '--no-modify-path'], 'rust-setup')
        else:
            report['cache_hits'].append('rust-1.94.0')
        version = execute(['rustc', '-vV'], 'rust-version').read_text()
        assert 'release: 1.94.0\n' in version and 'host: aarch64-unknown-linux-gnu\n' in version
        # Preserve locked versions; remove network preparation from measured builds.
        fetch = ['cargo', 'fetch', '--locked', '--manifest-path', f'{source_mount}/rust/Cargo.toml',
                 '--target', 'aarch64-unknown-linux-gnu']
        execute([*fetch, '--offline'], 'locked-dependency-cache-check', required=False)
        if report['commands'][-1]['exit_code']:
            execute(fetch, 'locked-dependency-fetch')
        else:
            report['cache_hits'].append('locked-dependencies-offline')
        execute([*fetch, '--offline'], 'locked-dependency-cache-verified')
        cpus = sorted(os.sched_getaffinity(0))
        assert len(cpus) > 1, 'No one-job fallback'
        guest = [f'CARGO_BUILD_JOBS={len(cpus)}' if a.startswith('CARGO_BUILD_JOBS=') else
                 f'RUST_TEST_THREADS={len(cpus)}' if a.startswith('RUST_TEST_THREADS=') else a
                 for a in map(str, guest)]
        report.update(allowed_cpus=cpus, cargo_build_jobs=len(cpus))
        print('BUILD_CAPACITY ' + json.dumps(dict(allowed_cpus=cpus, cargo_build_jobs=len(cpus))), flush=True)
        build = ['cargo', 'build', '--locked', '--manifest-path', f'{source_mount}/rust/Cargo.toml',
                 '-p', 'openpilot-runtime-build', '--bin', 'openpilot-runtime-build',
                 '--bin', 'openpilot-runtime-plan', '--target-dir', work/'project-target', '--message-format=json']
        execute(build, 'native-project-build')
        report['native_compile_seconds'] = report['commands'][-1]['elapsed_seconds']
        report['qemu_to_native_compile_time_ratio'] = BASELINE_SECONDS / report['native_compile_seconds']
        for name in ('openpilot-runtime-build', 'openpilot-runtime-plan'):
            binary = work/'project-target/debug'/name
            with binary.open('rb') as stream:
                header = stream.read(64)
            assert header[:6] == b'\x7fELF\x02\x01' and int.from_bytes(header[18:20], 'little') == 183
        execute(build, 'native-project-reused')
        cargo = execute(['rustup', 'which', '--toolchain', '1.94.0', 'cargo'], 'native-cargo-path').read_text().strip()
        catalog = json.loads(execute([work/'project-target/debug/openpilot-runtime-plan',
            '--root', source_mount, '--cargo', cargo, '--check'], 'native-project-catalog').read_text())
        assert catalog['source']['commit'] == SOURCE and catalog['groups'] == 13 and catalog['binaries'] == 99
        assert not catalog['compiled'] and not catalog['activated']
        report['catalog'] = dict(groups=13, binaries=99, compiled=False)
        execute(['/usr/bin/python3', f'{source_mount}/tools/ci/verify_native_compiler_features.py',
                 '--cargo', cargo, '--root', source_mount, '--source-commit', SOURCE,
                 '--output', work/'native-compiler-features'], 'native-project-compiler-features')
        assert not execute(['git', '-C', source_mount, 'status', '--porcelain', '--untracked-files=all'],
                           'source-after').read_text().strip()
        report['compiler_comparison_success'] = True
        report['remaining_original_steps'] = 'Pinned-input probes, privileged bootstrap, codec/provider regressions and full generation remain unverified'
    except BaseException as error:
        report['compiler_comparison_success'] = False
        report['failure'] = str(error)
        raise
    finally:
        report['total_elapsed_seconds'] = time.monotonic()-started
        report['owned_work_directory'] = str(owned)
        (owned/'report.json').write_text(json.dumps(report, indent=2)+'\n')
        print('AGNOS_HOSTED_ARM_REPORT ' + json.dumps(report), flush=True)
        with open(os.environ['GITHUB_STEP_SUMMARY'], 'a') as stream:
            stream.write('\n## 실제 AGNOS 컴파일 비교\n\n')
            stream.write('원본 전체 workflow의 성공을 뜻하지 않습니다. cgroup 검증 등은 미완료입니다.\n\n')
            stream.write('| 명령 | 시간(초) | 종료 코드 |\n|---|---:|---:|\n')
            for row in report['commands']:
                stream.write(f"| {row['name']} | {row['elapsed_seconds']:.3f} | {row['exit_code']} |\n")
            if report.get('compiler_comparison_success'):
                stream.write(f"\n기존 bin-actions3/QEMU: {BASELINE_SECONDS:.3f}초. "
                             f"GitHub ARM: {report['native_compile_seconds']:.3f}초. "
                             f"시간 비율: {report['qemu_to_native_compile_time_ratio']:.3f}.\n")
        # Keep evidence and the job-owned cache for diagnosis, never remove another job's data.


if __name__ == '__main__':
    main()
