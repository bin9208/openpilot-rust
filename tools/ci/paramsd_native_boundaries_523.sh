#!/usr/bin/env bash
# #523 CI 전용. 설치/빌드 없이 fresh 원본 캡처와 기존 ignored 검사 2개만 실행한다.
set -euo pipefail
umask 077
fail() { printf 'paramsd boundaries: %s\n' "$*" >&2; exit 1; }
[[ $# == 0 ]] || fail '인자는 받지 않음; 문서의 명시적 환경변수를 사용'
[[ ${GITHUB_ACTIONS:-} == true && ${RUNNER_NAME:-} == bin-actions && ${RUNNER_OS:-} == Linux ]] || fail 'bin-actions Linux CI 전용'
[[ ${GITHUB_REPOSITORY:-} == bin9208/openpilot-rust ]] || fail '저장소 불일치'
for key in GITHUB_WORKSPACE RUNNER_TEMP EXPECTED_SOURCE CARGO_TARGET_DIR PARAMSD_BOUNDARY_PROOF PARAMSD_ORACLE_PYTHON PARAMSD_GENERATOR_BINARY PARAMSD_GENERATOR_SHA256 PARAMSD_CAR_TEST_BINARY PARAMSD_CAR_TEST_SHA256 PARAMSD_NUMPY_TEST_BINARY PARAMSD_NUMPY_TEST_SHA256; do
  [[ -n ${!key:-} ]] || fail "필수 환경변수 없음: $key"
done
[[ $EXPECTED_SOURCE =~ ^[0-9a-f]{40}$ ]] || fail 'source SHA 형식'
for key in PARAMSD_GENERATOR_SHA256 PARAMSD_CAR_TEST_SHA256 PARAMSD_NUMPY_TEST_SHA256; do
  [[ ${!key} =~ ^[0-9a-f]{64}$ ]] || fail "binary SHA 형식: $key"
done
for tool in git readlink timeout df sha256sum; do command -v "$tool" >/dev/null || fail "도구 없음: $tool"; done
timeout --version | { IFS= read -r line; [[ $line == *'GNU coreutils'* ]]; cat >/dev/null; } || fail 'GNU timeout 필요'
boundary_root=$(readlink -e -- "$GITHUB_WORKSPACE")
[[ $(git -C "$boundary_root" rev-parse HEAD) == "$EXPECTED_SOURCE" ]] || fail 'checkout SHA 불일치'
[[ $PARAMSD_ORACLE_PYTHON == /* && -x $PARAMSD_ORACLE_PYTHON ]] || fail '기존 venv Python 절대 경로 필요'
[[ $CARGO_TARGET_DIR == /* && -d $CARGO_TARGET_DIR ]] || fail '부모의 fresh Cargo target 절대 경로 필요'
[[ $PARAMSD_BOUNDARY_PROOF == /* && ! -e $PARAMSD_BOUNDARY_PROOF && ! -L $PARAMSD_BOUNDARY_PROOF ]] || fail 'proof는 새 절대 경로여야 함'
boundary_parent=$(readlink -e -- "$(dirname -- "$PARAMSD_BOUNDARY_PROOF")")
boundary_temp=$(readlink -e -- "$RUNNER_TEMP")
[[ $boundary_parent == "$boundary_temp" || $boundary_parent == "$boundary_temp/"* ]] || fail 'proof는 RUNNER_TEMP 하위만 허용'
boundary_free=$(df -B1 --output=avail -- "$boundary_parent" | tail -n 1)
boundary_free=${boundary_free//[[:space:]]/}
[[ $boundary_free =~ ^[0-9]+$ ]] && (( boundary_free >= 64 * 1024 * 1024 )) || fail '캡처/로그 예상 증가량 64 MiB 부족'
mkdir -- "$PARAMSD_BOUNDARY_PROOF"
export PARAMSD_BOUNDARY_PROOF
finish() {
  local result=$?
  trap - EXIT
  printf '%s\n' "$result" > "$PARAMSD_BOUNDARY_PROOF/exit-status"
  (cd -- "$PARAMSD_BOUNDARY_PROOF" && find . -type f ! -name files.sha256 -print0 | sort -z | xargs -0 -r sha256sum > files.sha256) || result=1
  exit "$result"
}
trap finish EXIT
printf '{"free_bytes":%s,"planned_growth_bytes":67108864}\n' "$boundary_free" > "$PARAMSD_BOUNDARY_PROOF/growth.json"
cp -- "${BASH_SOURCE[0]}" "$PARAMSD_BOUNDARY_PROOF/diagnostic-source.sh"
# -I는 PYTHONPATH/user site를 배제하고 기존 venv를 유지한다. 새 pycache 위치로 오래된 bytecode도 피한다.
# timeout은 별도 process group을 만들며 모든 native/test 자식은 해당 그룹을 상속한다.
timeout --signal=KILL 240s "$PARAMSD_ORACLE_PYTHON" -I -B - <<'BOUNDARY_PY' > "$PARAMSD_BOUNDARY_PROOF/capture.stdout" 2> "$PARAMSD_BOUNDARY_PROOF/capture.stderr"
import contextlib
import hashlib
import json
import math
import os
from pathlib import Path
import re
import resource
import stat
import struct
import subprocess
import sys
import traceback
import types

OUT = Path(os.environ['PARAMSD_BOUNDARY_PROOF']).resolve(strict=True)
ROOT = Path(os.environ['GITHUB_WORKSPACE']).resolve(strict=True)
TARGET = Path(os.environ['CARGO_TARGET_DIR']).resolve(strict=True)
EXPECTED = os.environ['EXPECTED_SOURCE']
resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
resource.setrlimit(resource.RLIMIT_FSIZE, (8 << 20, 8 << 20))
sys.pycache_prefix = str(OUT / 'unused-pycache')
held = {}
fds = []
source_before = None
providers_before = None
status = {'capture_complete': False, 'tests_passed': 0, 'maintained_atomic_gate_executed': False}

def need(value, reason):
    if not value:
        raise RuntimeError(reason)

def save(name, value):
    with (OUT / name).open('x') as stream:
        json.dump(value, stream, sort_keys=True, indent=2, allow_nan=False)
        stream.write('\n')

def identity(s):
    return [s.st_dev, s.st_ino, s.st_mode, s.st_size, s.st_mtime_ns, s.st_ctime_ns]

def seal(path):
    path = Path(path)
    before = path.stat()
    need(stat.S_ISREG(before.st_mode) and before.st_size <= 2 << 30, f'bounded regular input required: {path}')
    digest = hashlib.sha256()
    with path.open('rb') as stream:
        while block := stream.read(1 << 20):
            digest.update(block)
    need(identity(path.stat()) == identity(before), f'input changed while hashing: {path}')
    return {'identity': identity(before), 'sha256': digest.hexdigest(), 'realpath': str(path.resolve(strict=True))}

SOURCE_PATHS = [
    'tools/ci/paramsd_native_boundaries_523.sh',
    'rust/Cargo.toml', 'rust/Cargo.lock', 'rust/crates/dev-tools/Cargo.toml',
    'rust/crates/dev-tools/src/bin/generate_paramsd_car_params.rs',
    'rust/crates/dev-tools/src/paramsd_fixture_generate.rs',
    'rust/crates/dev-tools/src/paramsd_estimator_check.rs',
    'rust/crates/dev-tools/src/paramsd_estimator_check',
    'rust/crates/dev-tools/tests/paramsd_car_params_cli.rs',
    'rust/crates/dev-tools/tests/paramsd_estimator_check.rs', 'rust/crates/cereal',
    'rust/tools/paramsd_fixture.py', 'rust/tools/paramsd_source.py', 'rust/tools/check_paramsd.py',
    'openpilot/__init__.py', 'openpilot/cereal', 'opendbc_repo/opendbc/__init__.py',
    'opendbc_repo/opendbc/car/car.capnp', 'opendbc_repo/opendbc/car/include',
]

def git(*args):
    return subprocess.check_output(['git', '-C', str(ROOT), *args], timeout=10)

def sources():
    need(git('rev-parse', 'HEAD').decode().strip() == EXPECTED, 'checkout SHA differs')
    need(not git('diff', '--name-only', 'HEAD', '--', *SOURCE_PATHS), 'related tracked source is dirty')
    names = git('ls-files', '-z', '--', *SOURCE_PATHS).split(b'\0')
    return {os.fsdecode(p): seal(ROOT / os.fsdecode(p)) for p in names if p}

def original_module(name):
    # 원본 모듈 전체를 실제 source bytes에서 로드한다. main()/learner/provider load()는 호출하지 않는다.
    path = ROOT / 'rust/tools' / f'{name}.py'
    need(name not in sys.modules, f'preloaded original module: {name}')
    module = types.ModuleType(name)
    module.__file__ = str(path)
    sys.modules[name] = module
    exec(compile(path.read_bytes(), str(path), 'exec'), module.__dict__)
    return module

def run(label, name, arguments, extra_env=None):
    row = held[label]
    command = [f"/proc/self/fd/{row['fd']}", *arguments]
    env = {'PATH': '/nonexistent', 'HOME': str(OUT / 'home'), 'TMPDIR': str(OUT / 'tmp'),
           'LC_ALL': 'C.UTF-8', 'TZ': 'UTC'}
    env.update(extra_env or {})
    with (OUT / f'{name}.stdout').open('xb') as stdout, (OUT / f'{name}.stderr').open('xb') as stderr:
        child = subprocess.Popen(command, cwd=OUT, env=env, stdout=stdout, stderr=stderr, pass_fds=(row['fd'],))
        try:
            code = child.wait(timeout=60)
        except BaseException:
            with contextlib.suppress(ProcessLookupError):
                child.kill()
            child.wait()
            raise
        finally:
            save(f'{name}.process.json', {'command': command, 'held_executable': row['path'], 'source': EXPECTED,
                                         'pid': child.pid, 'returncode': child.returncode, 'reaped': child.returncode is not None})
    need(code == 0, f'{name} failed; retain stdout/stderr')

try:
    need(os.getpgrp() == os.getppid(), 'GNU timeout process group required')
    need(sys.prefix != sys.base_prefix, 'existing oracle venv required')
    need(sys.flags.optimize == 0, 'retained compare assertions must be enabled')
    need(Path(sys.executable).resolve() == Path(os.environ['PARAMSD_ORACLE_PYTHON']).resolve(), 'wrong oracle interpreter')
    for name in ('home', 'tmp', 'car-params'):
        (OUT / name).mkdir()
    source_before = sources()
    save('source.before.json', source_before)
    for path, pin in {
        'rust/tools/paramsd_fixture.py': '8c0356bb780039c475def68358567489e036ab422921b8389bc7fbed20635521',
        'rust/tools/check_paramsd.py': '5fda5495d04cdd665550f10a33bd61a3c3bc762e884a308f7bf152cbd10480a5',
    }.items():
        need(source_before[path]['sha256'] == pin, f'original source pin changed: {path}')
    for label, path_key, hash_key in [
        ('generator', 'PARAMSD_GENERATOR_BINARY', 'PARAMSD_GENERATOR_SHA256'),
        ('car-test', 'PARAMSD_CAR_TEST_BINARY', 'PARAMSD_CAR_TEST_SHA256'),
        ('numpy-test', 'PARAMSD_NUMPY_TEST_BINARY', 'PARAMSD_NUMPY_TEST_SHA256'),
    ]:
        path = Path(os.environ[path_key])
        need(path.is_absolute(), f'absolute binary required: {label}')
        path = path.resolve(strict=True)
        need(path.is_relative_to(TARGET) and os.access(path, os.X_OK), f'binary must belong to prepared fresh target: {label}')
        fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
        fds.append(fd)
        need(os.pread(fd, 4, 0) == b'\x7fELF', f'compiled native ELF required: {label}')
        row = seal(path)
        need(row['sha256'] == os.environ[hash_key] and identity(os.fstat(fd)) == row['identity'], f'fresh binary binding differs: {label}')
        held[label] = {'path': str(path), 'seal': row, 'fd': fd}
    need(len({(r['seal']['identity'][0], r['seal']['identity'][1]) for r in held.values()}) == 3, 'three distinct prepared executables required')
    save('executables.before.json', held)
    sys.path[:0] = [str(ROOT), str(ROOT / 'rust/tools'), str(ROOT / 'opendbc_repo')]
    import numpy as np
    import capnp
    import openpilot.cereal as cereal
    import opendbc
    need(np.__version__ == '2.5.3', 'independent NumPy 2.5.3 required')
    need(Path(cereal.__file__).resolve() == ROOT / 'openpilot/cereal/__init__.py', 'foreign cereal source')
    need(Path(opendbc.__file__).resolve() == ROOT / 'opendbc_repo/opendbc/__init__.py', 'foreign opendbc schema path')
    fixture = original_module('paramsd_fixture')
    original_module('paramsd_source')
    checker = original_module('check_paramsd')
    imported = {str(Path(m.__file__).resolve()) for m in list(sys.modules.values())
                if getattr(m, '__file__', None) and Path(m.__file__).is_file()}
    # NumPy/pycapnp의 실제 native dependency도 포함한다. 경로 없는 anonymous mappings는 파일 입력이 아니다.
    maps = Path('/proc/self/maps').read_text()
    (OUT / 'oracle-maps.before.txt').write_text(maps)
    for line in maps.splitlines():
        fields = line.split(None, 5)
        if len(fields) == 6 and fields[5].startswith('/'):
            need(not fields[5].endswith(' (deleted)'), 'mapped provider was deleted')
            imported.add(str(Path(fields[5]).resolve(strict=True)))
    imported.add(str(Path(sys.executable).resolve(strict=True)))
    providers_before = {p: seal(p) for p in sorted(imported)}
    save('providers.before.json', providers_before)
    save('oracle.json', {'python': sys.version, 'executable': sys.executable, 'prefix': sys.prefix,
                         'numpy_version': np.__version__, 'pycapnp_version': capnp.__version__, 'source': EXPECTED})
    car_proof = OUT / 'car-params'
    invocations = []
    for profile in range(3):
        for name, fingerprint in [('default', None), ('empty', ''), ('custom', 'SYNTHETIC_523_é_차량')]:
            original = bytes(fixture.car_params(profile, fingerprint))
            need(bool(original), 'empty original CarParams')
            original_path = car_proof / f'original-{profile}-{name}.bin'
            native_path = car_proof / f'native-{profile}-{name}.bin'
            with original_path.open('xb') as stream:
                stream.write(original)
            arguments = ['--root', str(ROOT), '--output', str(native_path), '--profile', str(profile)]
            if fingerprint is not None:
                arguments += ['--fingerprint', fingerprint]
            run('generator', f'car-{profile}-{name}', arguments)
            need(native_path.is_file() and not native_path.is_symlink(), 'native CLI output missing/aliased')
            invocations.append({'profile': profile, 'fingerprint_case': name, 'arguments': arguments,
                                'original': seal(original_path), 'native': seal(native_path)})
    need(len(invocations) == 9, 'exactly nine fresh CLI pairs required')
    save('car-invocations.json', invocations)

    def bits(value):
        return struct.unpack('<Q', struct.pack('<d', float(value)))[0]

    def value(raw):
        return struct.unpack('<d', struct.pack('<Q', raw))[0]

    # 32 explicit f64 patterns: signed zeros, subnormal/normal boundaries, ULPs,
    # asymmetry witnesses, maximum finite values, infinities and quiet NaNs.
    patterns = [
        0, 0x8000000000000000, 1, 0x8000000000000001,
        0x000fffffffffffff, 0x800fffffffffffff, 0x0010000000000000, 0x8010000000000000,
        0x0010000000000001, 0x8010000000000001, 0x3fefffffffffffff, 0xbfefffffffffffff,
        0x3ff0000000000000, 0xbff0000000000000, 0x3ff0000000000001, 0xbff0000000000001,
        bits(10.), bits(11.2), bits(-10.), bits(-11.2), bits(1e-300), bits(-1e-300),
        bits(1e300), bits(-1e300), 0x7feffffffffffffe, 0xffeffffffffffffe,
        0x7fefffffffffffff, 0xffefffffffffffff, 0x7ff0000000000000, 0xfff0000000000000,
        0x7ff8000000000001, 0xfff8000000000001,
    ]
    need(len(patterns) == len(set(patterns)) == 32, 'f64 seed roster changed')
    tolerances = [0., value(1), 1e-320, 1e-9, 2e-6, .1, 1., 2., sys.float_info.max]
    cases = []
    policy_checks = 0

    def observe(left, right, tolerance, category):
        global policy_checks
        # Expected answers come only from installed NumPy, never Rust or a copied isclose formula.
        with np.errstate(all='ignore'):
            close = bool(np.isclose(left, right, atol=tolerance, rtol=tolerance, equal_nan=True))
            if tolerance in (1e-9, 2e-6):
                path = 'paramsd.x[0]' if tolerance == 1e-9 else 'paramsd.event.liveParameters.steerRatio'
                try:
                    checker.compare(float(left), float(right), path, [])
                    original_close = True
                except AssertionError:
                    original_close = False
                need(original_close == close, 'retained compare/NumPy policy differs')
                policy_checks += 1
        cases.append({'expected_bits': bits(left), 'actual_bits': bits(right), 'tolerance': tolerance,
                      'close': close, 'category': category})

    for tolerance in tolerances:
        for left in patterns:
            for right in patterns:
                observe(value(left), value(right), tolerance, 'bit-matrix')
    references = [0., -0., 1., -1., 10., -10., 1e-300, -1e-300,
                  1e300, -1e300, sys.float_info.max, -sys.float_info.max, .5, -.5, 2., -2.]
    for tolerance in (1e-9, 2e-6):
        for reference in references:
            # Candidate construction targets neighboring threshold floats; NumPy still supplies every answer.
            radius = tolerance + tolerance * abs(reference)
            for direction in (-1., 1.):
                center = reference + direction * radius
                for candidate in (math.nextafter(center, -math.inf), center, math.nextafter(center, math.inf)):
                    observe(candidate, reference, tolerance, 'threshold-ulp')
                    observe(reference, candidate, tolerance, 'threshold-ulp-reversed')
    need(len(cases) == 9600 and policy_checks == 2432, 'exact corpus/policy observation count changed')
    need(any(c['close'] for c in cases) and any(not c['close'] for c in cases), 'degenerate observation corpus')
    corpus = OUT / 'numpy-controls.json'
    save('numpy-controls.json', {'numpy_version': np.__version__, 'cases': cases})
    save('capture-counts.json', {'car_pairs': 9, 'numpy_cases': len(cases), 'original_compare_observations': policy_checks,
                               'tolerances': tolerances, 'source': EXPECTED})
    captured = {str(p): seal(p) for p in [corpus, *sorted(car_proof.iterdir())]}
    need(len(captured) == 19, 'nine original/native pairs plus corpus required')
    save('captured.before.json', captured)
    status.update(capture_complete=True, car_pairs=9, numpy_cases=len(cases))
    for label, selector, env in [
        ('car-test', 'all_cli_profiles_and_fingerprints_match_fresh_python_fixture_bytes', {'PARAMSD_CAR_PARAMS_PROOF': str(car_proof)}),
        ('numpy-test', 'matches_fresh_numpy_comparison_boundary_observations', {'PARAMSD_NUMPY_CONTROLS': str(corpus)}),
    ]:
        run(label, label, [selector, '--ignored', '--exact', '--nocapture', '--test-threads=1', '--color=never'], env)
        output = (OUT / f'{label}.stdout').read_text()
        need(re.search(r'^test result: ok\. 1 passed; 0 failed; 0 ignored; 0 measured; \d+ filtered out; finished in .+$', output, re.M), 'ignored selector did not execute exactly one passing test')
        need(f'test {selector} ... ok' in output, 'wrong test selector executed')
        status['tests_passed'] += 1
    captured_after = {p: seal(p) for p in captured}
    save('captured.after.json', captured_after)
    need(captured_after == captured, 'captured proof changed during tests')
except BaseException as error:
    status['error'] = str(error)
    traceback.print_exc()
    raise
finally:
    try:
        after = {label: {'path': row['path'], 'seal': seal(row['path']), 'fd': row['fd']} for label, row in held.items()}
        save('executables.after.json', after)
        need(after == held and all(identity(os.fstat(r['fd'])) == r['seal']['identity'] for r in held.values()), 'prepared binary changed')
        if source_before is not None:
            source_after = sources()
            save('source.after.json', source_after)
            need(source_after == source_before, 'source roster/identity/hash changed')
        if providers_before is not None:
            providers_after = {p: seal(p) for p in providers_before}
            save('providers.after.json', providers_after)
            need(providers_after == providers_before, 'oracle source/extension/shared provider changed')
        status['final_guards_passed'] = True
    except BaseException as error:
        status.update(final_guards_passed=False, final_guard_error=str(error))
        raise
    finally:
        save('result.json', status)
        for fd in fds:
            os.close(fd)
need(status['capture_complete'] and status['tests_passed'] == 2 and status['final_guards_passed'], 'incomplete boundary acceptance')
save('complete.json', {'source': EXPECTED, 'car_pairs': 9, 'numpy_cases': 9600, 'passed_ignored_selectors': 2,
                       'maintained_atomic_gate_executed': False, 'device_or_runtime_acceptance': False})
BOUNDARY_PY
