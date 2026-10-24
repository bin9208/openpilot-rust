#!/usr/bin/env bash
# #482 CI 전용 원본/native compiler 비교. SDK 설치/빌드, object/model 실행 없음.
set -euo pipefail
umask 077
fail() { printf 'model compiler pair: %s\n' "$*" >&2; exit 1; }
[[ $# == 0 ]] || fail '인자는 받지 않음; readme.md의 명시적 환경변수를 사용'
[[ ${GITHUB_ACTIONS:-} == true && ${RUNNER_NAME:-} == bin-actions && ${RUNNER_OS:-} == Linux ]] || fail 'bin-actions Linux CI 전용'
[[ $(uname -m) == x86_64 && ${GITHUB_REPOSITORY:-} == bin9208/openpilot-rust ]] || fail '호스트/저장소 불일치'
for key in GITHUB_WORKSPACE RUNNER_TEMP EXPECTED_SOURCE ORACLE_PYTHON CURRENT_LLVM_WORKER CURRENT_LLVM_WORKER_SHA256 PAIR_OUTPUT PAIR_PREPARATION_GROWTH_MIB GH_TOKEN; do
  [[ -n ${!key:-} ]] || fail "필수 환경변수 없음: $key"
done
[[ $EXPECTED_SOURCE =~ ^[0-9a-f]{40}$ && $CURRENT_LLVM_WORKER_SHA256 =~ ^[0-9a-f]{64}$ ]] || fail 'source/worker pin 형식'
[[ $PAIR_PREPARATION_GROWTH_MIB =~ ^(0|[1-9][0-9]{0,3})$ ]] || fail '준비 증가량은 MiB 정수'
# ZIP 26MiB + 두 object 각64MiB + optimized IR/debug 로그 각64MiB + metadata 여유.
pair_diagnostic_growth_mib=384
pair_total_growth_mib=$((PAIR_PREPARATION_GROWTH_MIB + pair_diagnostic_growth_mib))
(( pair_total_growth_mib <= 1024 )) || fail 'SDK/원본 준비 포함 예상 증가량이 1GiB를 초과함'
for name in gh jq unzip sha256sum stat readlink timeout lscpu dpkg-query git df; do
  command -v "$name" >/dev/null || fail "준비된 도구 없음: $name (자동 설치하지 않음)"
done
timeout --version | sed -n '1p' | { IFS= read -r line; [[ $line == *'GNU coreutils'* ]]; } || fail 'GNU timeout 필요'
pair_root=$(readlink -e -- "$GITHUB_WORKSPACE")
[[ $(git -C "$pair_root" rev-parse HEAD) == "$EXPECTED_SOURCE" ]] || fail '현재 checkout SHA 불일치'
git -C "$pair_root" diff --quiet HEAD -- tinygrad_repo rust/Cargo.toml rust/Cargo.lock rust/crates/model-runtime || fail '관련 source에 미커밋 변경이 있음'
[[ $ORACLE_PYTHON == /* && -x $ORACLE_PYTHON && $CURRENT_LLVM_WORKER == /* && -x $CURRENT_LLVM_WORKER ]] || fail '준비된 절대 interpreter/worker 경로 필요'
[[ $PAIR_OUTPUT == /* && ! -e $PAIR_OUTPUT && ! -L $PAIR_OUTPUT ]] || fail 'PAIR_OUTPUT은 새 절대 경로여야 함'
pair_parent=$(readlink -e -- "$(dirname -- "$PAIR_OUTPUT")")
pair_temp=$(readlink -e -- "$RUNNER_TEMP")
[[ $pair_parent == "$pair_temp" || $pair_parent == "$pair_temp/"* ]] || fail '출력은 RUNNER_TEMP 하위에만 허용'
pair_free=$(df -B1 --output=avail -- "$pair_parent" | tail -n 1)
pair_free=${pair_free//[[:space:]]/}
[[ $pair_free =~ ^[0-9]+$ ]] || fail '사용 가능 공간 판독 실패'
(( pair_free >= pair_total_growth_mib * 1048576 )) || fail '작업별 예상 증가량을 수용할 공간 없음'
mkdir -- "$PAIR_OUTPUT"
export PAIR_OUTPUT
finish() {
  local result=$?
  trap - EXIT
  printf '%s\n' "$result" > "$PAIR_OUTPUT/draft.exit-status"
  # 실패/timeout의 부분 산출물도 보존. archive/object를 실행하지 않는다.
  (cd -- "$PAIR_OUTPUT" && find . -type f ! -name files.sha256 -print0 | sort -z | xargs -0 -r sha256sum > files.sha256) || result=1
  exit "$result"
}
trap finish EXIT
printf '{"free_bytes":%s,"preparation_growth_mib":%s,"diagnostic_growth_mib":%s,"total_growth_mib":%s,"limit_mib":1024}\n' \
  "$pair_free" "$PAIR_PREPARATION_GROWTH_MIB" "$pair_diagnostic_growth_mib" "$pair_total_growth_mib" > "$PAIR_OUTPUT/growth.json"
cp -- "${BASH_SOURCE[0]}" "$PAIR_OUTPUT/draft-source.sh"
printf '%s\n' "$EXPECTED_SOURCE" > "$PAIR_OUTPUT/source-commit.txt"
git -C "$pair_root" status --short > "$PAIR_OUTPUT/source-status.txt"
dpkg-query -W -f='${binary:Package}\t${Version}\n' libllvm18:amd64 > "$PAIR_OUTPUT/llvm-package.txt"
[[ $(dpkg-query -W -f='${Version}' libllvm18:amd64) == 1:18.1.3-1ubuntu1 ]] || fail 'apt libllvm18 원본 버전 불일치'
pair_llvm=/usr/lib/x86_64-linux-gnu/libLLVM-18.so
pair_llvm_pin=5c3a63957e28c15cdf1503892074f032d29b72e36a9824a77dfa87e48a0dafb0
[[ -f $pair_llvm ]] || fail '고정 apt LLVM provider 없음; wheel/SDK provider fallback 금지'
[[ -z ${LLVM_PATH:-} || $LLVM_PATH == "$pair_llvm" ]] || fail '상속 LLVM_PATH가 지정 경로와 다름'
export LLVM_PATH=$pair_llvm
printf '%s  %s\n' "$pair_llvm_pin" "$LLVM_PATH" | sha256sum --check --strict > "$PAIR_OUTPUT/llvm-pin-check.txt"
readlink -e -- "$LLVM_PATH" > "$PAIR_OUTPUT/llvm-realpath.txt"
lscpu --json > "$PAIR_OUTPUT/natural-cpu.json"
cp -- /proc/cpuinfo "$PAIR_OUTPUT/proc-cpuinfo.txt"
uname -a > "$PAIR_OUTPUT/uname.txt"

# GH_TOKEN은 부모가 actions:read 권한으로 제공. 인증 정보/환경 전체는 출력하지 않는다.
pair_artifact=12449897200
pair_zip_pin=fae8695cefb272f44c5bf5c33f608ad2e2d69d6d4504c1c8ee883cef51d51317
pair_member=input/historical/source.ll
pair_ir_pin=e223a2c36c6bde7cf676bb92fd2529858a723f9ecfa91c811b2b941756a9bea0
timeout --signal=KILL 30s gh api --hostname github.com -H 'Cache-Control: no-cache' \
  "repos/bin9208/openpilot-rust/actions/artifacts/$pair_artifact" > "$PAIR_OUTPUT/artifact-api.json"
jq -e --arg digest "sha256:$pair_zip_pin" '.id == 12449897200 and .size_in_bytes == 26734808 and .expired == false and .digest == $digest and .workflow_run.id == 39833558563' \
  "$PAIR_OUTPUT/artifact-api.json" >/dev/null
timeout --signal=KILL 30s gh api --hostname github.com -H 'Cache-Control: no-cache' \
  "repos/bin9208/openpilot-rust/actions/artifacts/$pair_artifact/zip" > "$PAIR_OUTPUT/artifact.zip"
[[ $(stat -c '%s' -- "$PAIR_OUTPUT/artifact.zip") == 26734808 ]] || fail 'ZIP 길이 불일치'
printf '%s  %s\n' "$pair_zip_pin" "$PAIR_OUTPUT/artifact.zip" | sha256sum --check --strict > "$PAIR_OUTPUT/zip-pin-check.txt"
unzip -tq "$PAIR_OUTPUT/artifact.zip" > "$PAIR_OUTPUT/zip-crc.txt"
unzip -Z1 "$PAIR_OUTPUT/artifact.zip" > "$PAIR_OUTPUT/zip-members.txt"
[[ $(grep -Fxc -- "$pair_member" "$PAIR_OUTPUT/zip-members.txt") == 1 ]] || fail 'exact IR member가 없거나 중복됨'
unzip -p "$PAIR_OUTPUT/artifact.zip" "$pair_member" > "$PAIR_OUTPUT/original.ll"
[[ $(stat -c '%s' -- "$PAIR_OUTPUT/original.ll") == 166049 ]] || fail '원본 IR 길이 불일치'
printf '%s  %s\n' "$pair_ir_pin" "$PAIR_OUTPUT/original.ll" | sha256sum --check --strict > "$PAIR_OUTPUT/ir-pin-check.txt"

# GNU timeout은 --foreground 없이 자기 process group을 만든다.
# Python과 native worker는 이 그룹에 남으며, 90s KILL이 그룹 전체를 종료한다.
# -I는 현재 venv를 유지하면서 PYTHONPATH/user site를 배제; source prefix D0만 명시 추가.
timeout --signal=KILL 90s "$ORACLE_PYTHON" -I -B - <<'PAIR_PY' > "$PAIR_OUTPUT/pair.stdout" 2> "$PAIR_OUTPUT/pair.stderr"
import contextlib
import ctypes
import hashlib
import json
import os
from pathlib import Path
import resource
import stat
import subprocess
import sys
import traceback

OUT = Path(os.environ['PAIR_OUTPUT']).resolve()
ROOT = Path(os.environ['GITHUB_WORKSPACE']).resolve()
D0 = ROOT / 'tinygrad_repo'
LLVM_PIN = '5c3a63957e28c15cdf1503892074f032d29b72e36a9824a77dfa87e48a0dafb0'
IR_PIN = 'e223a2c36c6bde7cf676bb92fd2529858a723f9ecfa91c811b2b941756a9bea0'
LIMIT = 64 << 20
# 추가 도구/SDK 생성 없이, 로그와 각 compile 산출물의 파일 크기도 제한한다.
resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
resource.setrlimit(resource.RLIMIT_FSIZE, (LIMIT, LIMIT))

def need(value, reason):
    if not value:
        raise RuntimeError(reason)

def save(name, value):
    with (OUT / name).open('x') as handle:
        json.dump(value, handle, indent=2, sort_keys=True)
        handle.write('\n')

def identity(s):
    return [s.st_dev, s.st_ino, s.st_size, s.st_mode, s.st_mtime_ns, s.st_ctime_ns]

def seal(path):
    path = Path(path)
    before = path.stat()
    need(stat.S_ISREG(before.st_mode), f'bounded regular input required: {path}')
    need(before.st_size <= 2 << 30, f'input exceeds bound: {path}')
    digest = hashlib.sha256()
    with path.open('rb') as handle:
        while chunk := handle.read(1 << 20):
            digest.update(chunk)
    need(identity(before) == identity(path.stat()), f'input changed during hash: {path}')
    return {'realpath': str(path.resolve(strict=True)), 'identity': identity(before), 'sha256': digest.hexdigest()}

def sources():
    # 부모 whole-source seal에 더해, import 전에 현재 tinygrad source 전체를 봉인한다.
    paths = sorted(p for p in (D0 / 'tinygrad').rglob('*') if p.is_file() and '__pycache__' not in p.parts)
    paths += [ROOT / p for p in [
        'rust/Cargo.toml', 'rust/Cargo.lock', 'rust/crates/model-runtime/Cargo.toml',
        'rust/crates/model-runtime/src/bin/offline-llvm-compile.rs',
        'rust/crates/model-runtime/src/compiler_backend_cpu/llvm.rs',
        'rust/crates/model-runtime/src/compiler_backend_cpu.rs',
        'rust/crates/model-runtime/src/owned_outputs.rs',
    ]]
    return {str(p.relative_to(ROOT)): seal(p) for p in paths}

held = {}
held_fds = []
source_before = None
imports_before = None
status = {'compiler_pair_complete': False, 'objects_executed': False, 'model_or_historical_acceptance': False}
try:
    need(os.getpgrp() == os.getppid(), 'Python must be in GNU timeout own process group')
    save('process-group.json', {'pid': os.getpid(), 'ppid': os.getppid(), 'pgid': os.getpgrp(), 'native_inherits_group': True, 'outer_timeout_s': 90, 'native_timeout_s': 65})
    source_before = sources()
    save('sources.before.json', source_before)
    pins = {
        'tinygrad_repo/tinygrad/runtime/support/compiler_cpu.py': 'db106591197315ecf28f0e15de99d2841c9892e6e58290213c62c598e7014139',
        'tinygrad_repo/tinygrad/runtime/support/c.py': '0899ca9748d7150e55afd05e0de9443cbba2ac3bbf5b80807723b17b79fbc684',
        'tinygrad_repo/tinygrad/runtime/autogen/llvm.py': 'b9bca21686da3f55612f128d8f8170b7544cee72a23352049c526d3694488a0d',
        'rust/crates/model-runtime/src/bin/offline-llvm-compile.rs': '7bd9f6a9867b43dee0c120d05e97364fa0b843141615590d382b63c762b6661c',
        'rust/crates/model-runtime/src/compiler_backend_cpu/llvm.rs': '204d164dc96b48365ef8656b46e0bbeb7814530ef184191e7356b95bd1894e27',
    }
    need(all(source_before[p]['sha256'] == h for p, h in pins.items()), 'reviewed source pin changed; stop for review')
    provider = Path(os.environ['LLVM_PATH']).resolve(strict=True)
    worker = Path(os.environ['CURRENT_LLVM_WORKER']).resolve(strict=True)
    for label, path, pin in [
        ('provider', provider, LLVM_PIN), ('worker', worker, os.environ['CURRENT_LLVM_WORKER_SHA256']),
        ('interpreter', Path(sys.executable).resolve(strict=True), None), ('input', OUT / 'original.ll', IR_PIN),
    ]:
        fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
        held_fds.append(fd)
        row = seal(path)
        need(identity(os.fstat(fd)) == row['identity'], f'held fd differs: {label}')
        need(pin is None or row['sha256'] == pin, f'held pin differs: {label}')
        held[label] = {'path': str(path), 'seal': row, 'fd': fd}
    save('providers.before.json', held)
    save('python-environment.json', {'executable': sys.executable, 'prefix': sys.prefix, 'base_prefix': sys.base_prefix, 'version': sys.version, 'source_prefix_D0': str(D0)})
    need(Path(sys.executable).resolve() == Path(os.environ['ORACLE_PYTHON']).resolve(), 'oracle interpreter differs')
    need(sys.prefix != sys.base_prefix, 'existing oracle venv required; no implicit package setup')
    need(os.environ.get('LLVMOPT', '1') == '1', 'original default O2 context required; do not rewrite inherited setting')
    need(not any(k == 'tinygrad' or k.startswith('tinygrad.') for k in sys.modules), 'unexpected preloaded tinygrad; source prefix admission required')
    os.environ.update(DEBUG='7', ALLOW_DEVICE_USAGE='0', PROFILE='0', VIZ='0')
    sys.path.insert(0, str(D0))
    with (OUT / 'original-debug7.log').open('x') as log, contextlib.redirect_stdout(log):
        from tinygrad.runtime.autogen import llvm
        from tinygrad.runtime.support.compiler_cpu import CPULLVMCompiler
        from tinygrad.device import Device
        for module in ('tinygrad', 'tinygrad.runtime.support.compiler_cpu', 'tinygrad.runtime.support.c', 'tinygrad.runtime.autogen.llvm'):
            need(Path(sys.modules[module].__file__).resolve().is_relative_to((D0 / 'tinygrad').resolve()), 'original module outside source prefix D0')
        need(Path(llvm.dll._name) == Path('/usr/lib/x86_64-linux-gnu/libLLVM-18.so'), 'autogen DLL name differs; no SDK fallback')
        need(Path(llvm.dll._name).resolve(strict=True) == provider, 'autogen provider realpath differs')
        need(seal(llvm.dll._name)['sha256'] == LLVM_PIN, 'loaded provider pin differs')
        compiler = CPULLVMCompiler(['x86_64', 'native'])
        def tm_string(getter):
            ptr = getter(compiler.target_machine)
            need(bool(ptr), 'null TM getter result')
            try:
                return ctypes.string_at(ptr).decode()
            finally:
                llvm.LLVMDisposeMessage(ptr)
        passes = compiler.passes.decode()
        need(passes == 'default<O2>', 'source compiler pass configuration changed')
        config = {
            'library': os.environ['LLVM_PATH'], 'library_sha256': LLVM_PIN,
            'triple': tm_string(llvm.LLVMGetTargetMachineTriple),
            'cpu': tm_string(llvm.LLVMGetTargetMachineCPU),
            'features': tm_string(llvm.LLVMGetTargetMachineFeatureString), 'passes': passes,
            # 위 pinned source의 실제 O2 분기에서 네 setter가 모두 True임을 확인했다.
            'loop_unrolling': True, 'loop_vectorization': True, 'slp_vectorization': True, 'verify_each': True,
        }
        save('natural-config.json', config)
        version = [ctypes.c_uint32() for _ in range(3)]
        llvm.LLVMGetVersion(*(ctypes.byref(x) for x in version))
        version = [x.value for x in version]
        need(version == [18, 1, 3], 'LLVM version differs')
        imported = {str(Path(m.__file__).resolve()) for m in list(sys.modules.values()) if getattr(m, '__file__', None) and Path(m.__file__).is_file()}
        imports_before = {p: seal(p) for p in sorted(imported)}
        save('imports.before.json', imports_before)
        maps = Path('/proc/self/maps').read_text()
        (OUT / 'original-maps.before.txt').write_text(maps)
        llvm_maps = [line.split(maxsplit=5) for line in maps.splitlines() if 'libLLVM' in line]
        expected_device = held['provider']['seal']['identity'][0]
        need(llvm_maps and all(len(r) == 6 and int(r[4]) == held['provider']['seal']['identity'][1] and
             r[3] == f'{os.major(expected_device):02x}:{os.minor(expected_device):02x}' and
             Path(r[5]).resolve(strict=True) == provider for r in llvm_maps), 'actual mapped LLVM differs')
        source = (OUT / 'original.ll').read_bytes()
        need(hashlib.sha256(source).hexdigest() == IR_PIN, 'IR changed')
        # compile_to_obj만 직접 한 번 호출한다. compile/compile_cached/jit_loader는 호출하지 않는다.
        original = compiler.compile_to_obj(source.decode())
        need(not Device._opened_devices, 'unexpected device initialization')
    need(0 < len(original) <= LIMIT and original.startswith(b'\x7fELF'), 'original object bound/type')
    with (OUT / 'original.o').open('xb') as handle:
        handle.write(original)
    save('original.receipt.json', {'config': config, 'llvm_version': version, 'source_sha256': IR_PIN,
        'object_sha256': hashlib.sha256(original).hexdigest(), 'provider_dll_name': str(llvm.dll._name),
        'direct_compile_to_obj_calls': 1, 'cache_used': False, 'object_executed': False})
    command = [f"/proc/self/fd/{held['worker']['fd']}", '--config', str(OUT / 'natural-config.json'),
               '--object', str(OUT / 'native.o'), '--receipt', str(OUT / 'native.receipt.json'),
               '--optimized-ir', str(OUT / 'native.optimized.ll')]
    with (OUT / 'native.stdout').open('xb') as stdout, (OUT / 'native.stderr').open('xb') as stderr:
        # start_new_session 없이 90s timeout 소유 그룹에 남긴다.
        child = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=stdout, stderr=stderr,
                                 pass_fds=(held['worker']['fd'],))
        try:
            child.communicate(input=source, timeout=65)
        except BaseException:
            with contextlib.suppress(ProcessLookupError):
                child.kill()
            child.wait()
            raise
        finally:
            save('native-process.json', {'pid': child.pid, 'returncode': child.returncode, 'reaped': child.returncode is not None, 'timeout_s': 65})
    need(child.returncode == 0, 'native worker failed; retain native.stderr')
    native = (OUT / 'native.o').read_bytes()
    receipt = json.loads((OUT / 'native.receipt.json').read_bytes())
    need(receipt['config'] == config and receipt['llvm_version'] == version and receipt['source_sha256'] == IR_PIN, 'native config/provider/source receipt differs')
    need(receipt['object_sha256'] == hashlib.sha256(native).hexdigest(), 'native object receipt hash differs')
    need(receipt['optimized_ir_sha256'] == seal(OUT / 'native.optimized.ll')['sha256'], 'native IR receipt hash differs')
    first = next((i for i, (a, b) in enumerate(zip(original, native)) if a != b), None)
    if first is None and len(original) != len(native):
        first = min(len(original), len(native))
    status.update(compiler_pair_complete=True, object_equal=original == native,
                  original_bytes=len(original), native_bytes=len(native), first_different_byte=first,
                  original_object_sha256=hashlib.sha256(original).hexdigest(), native_object_sha256=hashlib.sha256(native).hexdigest())
    need(original == native, 'object byte mismatch; retain both objects, IR logs, config and receipts')
except BaseException as error:
    status['error'] = str(error)
    traceback.print_exc()
    raise
finally:
    try:
        after = {label: {'path': row['path'], 'seal': seal(row['path']), 'fd': row['fd']} for label, row in held.items()}
        save('providers.after.json', after)
        need(after == held and all(identity(os.fstat(row['fd'])) == row['seal']['identity'] for row in held.values()), 'held source/provider identity changed')
        if source_before is not None:
            source_after = sources()
            save('sources.after.json', source_after)
            need(source_after == source_before, 'source roster/identity/hash changed')
            need(subprocess.check_output(['git', '-C', str(ROOT), 'rev-parse', 'HEAD'], text=True, timeout=5).strip() == os.environ['EXPECTED_SOURCE'], 'checkout HEAD changed')
        if imports_before is not None:
            imports_after = {p: seal(p) for p in imports_before}
            save('imports.after.json', imports_after)
            need(imports_after == imports_before, 'imported oracle source/extension changed')
        (OUT / 'original-maps.after.txt').write_text(Path('/proc/self/maps').read_text())
        status['final_guards_passed'] = True
    except BaseException as error:
        status.update(final_guards_passed=False, final_guard_error=str(error))
        raise
    finally:
        save('comparison.json', status)
        for fd in held_fds:
            os.close(fd)
PAIR_PY

# Python exit0만으로 수용하지 않고 필요한 완료/guard 상태를 다시 확인한다.
jq -e '.compiler_pair_complete == true and .object_equal == true and .final_guards_passed == true and .objects_executed == false and .model_or_historical_acceptance == false' \
  "$PAIR_OUTPUT/comparison.json" >/dev/null
printf 'compiler pair compiled only; model/historical acceptance remains false\n' > "$PAIR_OUTPUT/complete.txt"
