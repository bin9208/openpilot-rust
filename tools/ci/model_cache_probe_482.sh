#!/usr/bin/env bash
# #482 CI 전용. 실제 원본 cache 한 건의 miss -> hit만 관찰한다. 생성 code는 실행하지 않는다.
set -euo pipefail
umask 077
fail() { printf 'model cache probe: %s\n' "$*" >&2; exit 1; }
[[ $# == 0 ]] || fail '인자 없음; cache-only workflow의 명시적 환경변수 사용'
[[ ${GITHUB_ACTIONS:-} == true && ${RUNNER_NAME:-} == bin-actions && ${RUNNER_OS:-} == Linux ]] || fail 'bin-actions Linux CI 전용'
[[ ${GITHUB_REPOSITORY:-} == bin9208/openpilot-rust && $(uname -m) == x86_64 ]] || fail '저장소/호스트 불일치'
for key in DIAG_SOURCE RUNNER_TEMP EXPECTED_SOURCE ORACLE_PYTHON CACHE_PROBE_OUTPUT CACHE_PREPARATION_GROWTH_MIB GH_TOKEN; do
  [[ -n ${!key:-} ]] || fail "필수 환경변수 없음: $key"
done
[[ $EXPECTED_SOURCE =~ ^[0-9a-f]{40}$ ]] || fail '정확 source SHA 필요'
[[ $CACHE_PREPARATION_GROWTH_MIB =~ ^(0|[1-9][0-9]{0,3})$ ]] || fail '준비 증가량은 MiB 정수'
cache_diagnostic_growth_mib=256
cache_total_growth_mib=$((CACHE_PREPARATION_GROWTH_MIB + cache_diagnostic_growth_mib))
(( cache_total_growth_mib <= 1024 )) || fail '준비 포함 예상 증가량이 1GiB를 초과함'
for name in git readlink timeout sha256sum stat dpkg-query lscpu df jq gh; do
  command -v "$name" >/dev/null || fail "준비된 도구 없음: $name; 설치/fallback 안 함"
done
timeout --version | sed -n '1p' | { IFS= read -r line; [[ $line == *'GNU coreutils'* ]]; } || fail 'GNU timeout 필요'
[[ $DIAG_SOURCE == /* && $ORACLE_PYTHON == /* && -x $ORACLE_PYTHON ]] || fail '절대 source/준비된 venv 경로 필요'
cache_root=$(readlink -e -- "$DIAG_SOURCE")
[[ $(git -C "$cache_root" rev-parse HEAD) == "$EXPECTED_SOURCE" ]] || fail 'source HEAD 불일치'
git -C "$cache_root" diff --quiet HEAD -- tinygrad_repo || fail '원본 source 변경 있음'
[[ -z $(git -C "$cache_root" ls-files --others --exclude-standard -- tinygrad_repo) ]] || fail '원본 source에 untracked 파일 있음'
PAIR_NATURAL_CONFIG="$cache_root/tools/ci/model-cache-482-natural-config.json"
[[ -f $PAIR_NATURAL_CONFIG && ! -L $PAIR_NATURAL_CONFIG ]] || fail '고정 source config 없음'
[[ $(stat -c '%s' -- "$PAIR_NATURAL_CONFIG") == 1247 ]] || fail 'pair config 크기 불일치'
printf '%s  %s\n' 2d769aa04fecbf2175532d5ebca995eaaece64adb1200d81bcddebabd1ff7a0d "$PAIR_NATURAL_CONFIG" | sha256sum --check --strict
export PAIR_NATURAL_CONFIG
[[ $CACHE_PROBE_OUTPUT == /* && ! -e $CACHE_PROBE_OUTPUT && ! -L $CACHE_PROBE_OUTPUT ]] || fail '새 절대 proof 경로 필요'
cache_parent=$(readlink -e -- "$(dirname -- "$CACHE_PROBE_OUTPUT")")
cache_temp=$(readlink -e -- "$RUNNER_TEMP")
[[ $cache_parent == "$cache_temp" || $cache_parent == "$cache_temp/"* ]] || fail 'proof는 RUNNER_TEMP 하위여야 함'
cache_free=$(df -B1 --output=avail -- "$cache_parent" | tail -n 1)
cache_free=${cache_free//[[:space:]]/}
[[ $cache_free =~ ^[0-9]+$ ]] || fail 'free bytes 판독 실패'
(( cache_free >= cache_total_growth_mib * 1048576 )) || fail '작업별 증가량 수용 공간 없음'
mkdir -m 700 -- "$CACHE_PROBE_OUTPUT"
export CACHE_PROBE_OUTPUT
finish() {
  local result=$?
  trap - EXIT
  printf '%s\n' "$result" > "$CACHE_PROBE_OUTPUT/probe.exit-status"
  (cd -- "$CACHE_PROBE_OUTPUT" && find . -type f ! -name files.sha256 -print0 | sort -z | xargs -0 -r sha256sum > files.sha256) || result=1
  exit "$result"
}
trap finish EXIT
cp -- "${BASH_SOURCE[0]}" "$CACHE_PROBE_OUTPUT/probe-source.sh"
printf '%s\n' "$EXPECTED_SOURCE" > "$CACHE_PROBE_OUTPUT/source-commit.txt"
git -C "$cache_root" status --short > "$CACHE_PROBE_OUTPUT/source-status.txt"
printf '{"free_bytes":%s,"preparation_growth_mib":%s,"diagnostic_growth_mib":256,"total_growth_mib":%s,"limit_mib":1024}\n' \
  "$cache_free" "$CACHE_PREPARATION_GROWTH_MIB" "$cache_total_growth_mib" > "$CACHE_PROBE_OUTPUT/growth.json"
dpkg-query -W -f='${binary:Package}\t${Version}\n' libllvm18:amd64 > "$CACHE_PROBE_OUTPUT/llvm-package.txt"
[[ $(dpkg-query -W -f='${Version}' libllvm18:amd64) == 1:18.1.3-1ubuntu1 ]] || fail 'apt LLVM 버전 불일치'
# 성공한 97f7ad36 pair의 실제 경로. 이전 초안의 .so.1로 대체하지 않는다.
cache_llvm=/usr/lib/x86_64-linux-gnu/libLLVM-18.so
[[ -f $cache_llvm && ( -z ${LLVM_PATH:-} || $LLVM_PATH == "$cache_llvm" ) ]] || fail 'pair의 정확 LLVM_PATH 필요; provider fallback 금지'
export LLVM_PATH=$cache_llvm
printf '%s  %s\n' 5c3a63957e28c15cdf1503892074f032d29b72e36a9824a77dfa87e48a0dafb0 "$LLVM_PATH" | sha256sum --check --strict > "$CACHE_PROBE_OUTPUT/llvm-pin-check.txt"
lscpu --json > "$CACHE_PROBE_OUTPUT/natural-host.json"
cp -- /proc/cpuinfo "$CACHE_PROBE_OUTPUT/proc-cpuinfo.txt"

# actions:read token은 환경으로만 전달한다. metadata -> bounded ZIP -> size/hash 순서.
cache_zip_pin=ba28ae07b5becc3afe4346db3e86437f9a93aaaf02031c19f83e070ad9c7d93b
MODEL_BASELINE_ZIP="$CACHE_PROBE_OUTPUT/baseline.zip"
export MODEL_BASELINE_ZIP
(
  ulimit -f 64
  exec timeout --signal=KILL 30s gh api --hostname github.com -H 'Cache-Control: no-cache' \
    repos/bin9208/openpilot-rust/actions/artifacts/12404776700
) > "$CACHE_PROBE_OUTPUT/baseline-api.json"
jq -e --arg digest "sha256:$cache_zip_pin" \
  '.id == 12404776700 and .size_in_bytes == 4391525 and .digest == $digest and .expired == false and .workflow_run.id == 39729562958 and .workflow_run.head_sha == "21ed562971a92e4fea74ea777c189fad36e7bc2b"' \
  "$CACHE_PROBE_OUTPUT/baseline-api.json" >/dev/null
(
  # Bash 기본 단위 1024 bytes: 최대 8MiB. --foreground 없이 다운로드도 별도 그룹.
  ulimit -f 8192
  exec timeout --signal=KILL 30s gh api --hostname github.com -H 'Cache-Control: no-cache' \
    repos/bin9208/openpilot-rust/actions/artifacts/12404776700/zip
) > "$MODEL_BASELINE_ZIP.part"
[[ $(stat -c '%s' -- "$MODEL_BASELINE_ZIP.part") == 4391525 ]] || fail 'baseline ZIP 길이 불일치'
printf '%s  %s\n' "$cache_zip_pin" "$MODEL_BASELINE_ZIP.part" | sha256sum --check --strict > "$CACHE_PROBE_OUTPUT/baseline-pin-check.txt"
mv -- "$MODEL_BASELINE_ZIP.part" "$MODEL_BASELINE_ZIP"

# --foreground 없이 GNU timeout 소유 process group. kernel 실행/worker spawn 없음.
timeout --signal=KILL 90s "$ORACLE_PYTHON" -I -B - <<'CACHE_PY' > "$CACHE_PROBE_OUTPUT/probe.stdout" 2> "$CACHE_PROBE_OUTPUT/probe.stderr"
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
import zipfile

OUT = Path(os.environ['CACHE_PROBE_OUTPUT']).resolve(strict=True)
ROOT = Path(os.environ['DIAG_SOURCE']).resolve(strict=True)
D0 = ROOT / 'tinygrad_repo'
LLVM_PIN = '5c3a63957e28c15cdf1503892074f032d29b72e36a9824a77dfa87e48a0dafb0'
ZIP_PIN = 'ba28ae07b5becc3afe4346db3e86437f9a93aaaf02031c19f83e070ad9c7d93b'
CONFIG_PIN = '2d769aa04fecbf2175532d5ebca995eaaece64adb1200d81bcddebabd1ff7a0d'
IR_PIN = 'e223a2c36c6bde7cf676bb92fd2529858a723f9ecfa91c811b2b941756a9bea0'
FROZEN_PIN = '324697e4d28a400da3c521fea2f9140eea04d50f19e6771a96549f4b8a5a6767'
LIMIT = 16 << 20
resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
resource.setrlimit(resource.RLIMIT_FSIZE, (LIMIT, LIMIT))

def need(value, reason):
    if not value:
        raise RuntimeError(reason)

def sha(data):
    return hashlib.sha256(data).hexdigest()

def save(name, value):
    with (OUT / name).open('x') as handle:
        json.dump(value, handle, indent=2, sort_keys=True)
        handle.write('\n')

def write(name, value):
    need(len(value) <= LIMIT, f'output bound: {name}')
    with (OUT / name).open('xb') as handle:
        handle.write(value)

def identity(row):
    return [row.st_dev, row.st_ino, row.st_size, row.st_mode, row.st_mtime_ns, row.st_ctime_ns]

def seal(path, limit=1 << 30):
    path = Path(path)
    before = path.stat()
    need(stat.S_ISREG(before.st_mode) and before.st_size <= limit, f'regular input/bound: {path}')
    digest = hashlib.sha256()
    with path.open('rb') as handle:
        while chunk := handle.read(1 << 20):
            digest.update(chunk)
    need(identity(before) == identity(path.stat()), f'changed while hashing: {path}')
    return {'realpath': str(path.resolve(strict=True)), 'identity': identity(before), 'sha256': digest.hexdigest()}

def sources():
    paths = sorted(p for p in (D0 / 'tinygrad').rglob('*') if p.is_file() and '__pycache__' not in p.parts)
    need(len(paths) <= 4096 and sum(p.stat().st_size for p in paths) <= 64 << 20, 'source roster bound')
    return {str(p.relative_to(ROOT)): seal(p, LIMIT) for p in paths}

def imports():
    paths = {str(Path(m.__file__).resolve(strict=True)) for m in list(sys.modules.values())
             if getattr(m, '__file__', None) and Path(m.__file__).is_file()}
    need(len(paths) <= 4096, 'import roster bound')
    return {p: seal(p) for p in sorted(paths)}

held = {}
source_before = imports_before = None
restore = None
helpers = None
phase = 'admission'
calls = {'get': 0, 'put': 0, 'compile_to_obj': 0, 'jit_loader': 0}
events = []
status = {'cache_protocol_complete': False, 'objects_executed': False, 'frozen_image_executed': False,
          'model_or_historical_acceptance': False, 'historical_creation_context_known': False,
          'compiler_pair_repeated': False}

def hold(label, path, pin=None, limit=1 << 30):
    path = Path(path).resolve(strict=True)
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    held[label] = {'path': str(path), 'fd': fd, 'seal': seal(path, limit)}
    need(identity(os.fstat(fd)) == held[label]['seal']['identity'], f'held identity: {label}')
    need(pin is None or held[label]['seal']['sha256'] == pin, f'held pin: {label}')
    return fd

def mapped_provider(label):
    maps = Path('/proc/self/maps').read_text()
    write(f'process-maps.{label}.txt', maps.encode())
    rows = [line.split(maxsplit=5) for line in maps.splitlines() if 'libLLVM' in line]
    device, inode = held['provider']['seal']['identity'][:2]
    need(rows and all(len(row) == 6 and int(row[4]) == inode and
         row[3] == f'{os.major(device):02x}:{os.minor(device):02x}' and
         str(Path(row[5]).resolve(strict=True)) == held['provider']['path'] for row in rows), 'actual LLVM mapping differs')

try:
    need(OUT.stat().st_uid == os.getuid() and stat.S_IMODE(OUT.stat().st_mode) == 0o700, 'proof ownership/mode')
    need(os.getpgrp() == os.getppid(), 'GNU timeout own process group required')
    save('process-group.json', {'pid': os.getpid(), 'ppid': os.getppid(), 'pgid': os.getpgrp(), 'timeout_s': 90})
    source_before = sources()
    save('sources.before.json', source_before)
    pins = {
        'helpers.py': '24c347d2137c92e1dc35d78cd9df45b38d31cc4d34729a35215f8822b3f2f6fb',
        'device.py': '7c28ae3854a3f009fc6eddfb52f03b3d382a23b581d94a3c4741a12bdc79f008',
        'runtime/support/compiler_cpu.py': 'db106591197315ecf28f0e15de99d2841c9892e6e58290213c62c598e7014139',
        'runtime/support/elf.py': 'ed0340b507c8fb23f1fa1ae1b3ca1297006d70c289a016233d9f1d5291801aa8',
        'runtime/support/c.py': '0899ca9748d7150e55afd05e0de9443cbba2ac3bbf5b80807723b17b79fbc684',
        'runtime/autogen/llvm.py': 'b9bca21686da3f55612f128d8f8170b7544cee72a23352049c526d3694488a0d',
    }
    need(all(source_before[f'tinygrad_repo/tinygrad/{p}']['sha256'] == pin for p, pin in pins.items()), 'reviewed entrypoint source changed')
    provider = Path(os.environ['LLVM_PATH']).resolve(strict=True)
    hold('provider', provider, LLVM_PIN)
    hold('interpreter', sys.executable)
    need(sys.prefix != sys.base_prefix and Path(sys.executable).resolve() == Path(os.environ['ORACLE_PYTHON']).resolve(), 'prepared oracle venv required')
    hold('pyvenv', Path(sys.prefix) / 'pyvenv.cfg', limit=65536)
    baseline_fd = hold('baseline', os.environ['MODEL_BASELINE_ZIP'], ZIP_PIN, 4391525)
    config_fd = hold('pair_config', os.environ['PAIR_NATURAL_CONFIG'], CONFIG_PIN, 1247)
    save('providers-inputs.before.json', held)
    with os.fdopen(os.dup(config_fd), 'rb') as handle:
        pair_config_bytes = handle.read(1248)
    need(len(pair_config_bytes) == 1247 and sha(pair_config_bytes) == CONFIG_PIN, 'pair config bytes changed')
    write('pair-reference-config.json', pair_config_bytes)
    pair_config = json.loads(pair_config_bytes)
    need(pair_config['cpu'] == 'alderlake' and pair_config['library_sha256'] == LLVM_PIN, 'wrong pair reference')

    members = {
        'kernel92-original.ll': (166049, IR_PIN),
        'original-kernel92-code.bin': (14442, FROZEN_PIN),
        'program-binding.json': (967, 'e9353a5e24b982c1cbe45f9e02bb479a867273115971da9104d03f7d06e78728'),
        'context.json': (396, 'a6a1f6b07d38a54e85537df7655fced57139e77dfdb195c0c9e16b44dca9f02a'),
    }
    extracted = {}
    with os.fdopen(os.dup(baseline_fd), 'rb') as handle, zipfile.ZipFile(handle) as archive:
        infos = archive.infolist()
        need(len(infos) == 133 and len({i.filename for i in infos}) == len(infos), 'baseline roster/duplicate member')
        need(sum(i.file_size for i in infos) <= 16 << 20, 'baseline expanded size bound')
        for leaf, (size, pin) in members.items():
            info = archive.getinfo('first-kernel/' + leaf)
            need(info.file_size == size and not info.is_dir() and not info.flag_bits & 1 and
                 info.compress_type in (zipfile.ZIP_STORED, zipfile.ZIP_DEFLATED), 'member size/type/encryption')
            # ZipFile.read 검증은 선택한 네 member CRC에만 적용; 전체 추출/PKL 역직렬화 없음.
            data = archive.read(info)
            need(len(data) == size and sha(data) == pin, f'baseline member pin: {leaf}')
            extracted[leaf] = data
            write('baseline-' + leaf, data)
    binding = json.loads(extracted['program-binding.json'])
    need(binding['ir_sha256'] == IR_PIN and binding['code_sha256'] == FROZEN_PIN and binding['code_bytes'] == 14442, 'baseline binding differs')
    source = extracted['kernel92-original.ll'].decode('utf-8')
    frozen = extracted['original-kernel92-code.bin']

    need(os.environ.get('LLVMOPT', '1') == '1' and os.environ.get('ASSERT_COMPILE', '0') == '0', 'inherited compile control differs')
    need(not any(n == 'tinygrad' or n.startswith('tinygrad.') for n in sys.modules), 'tinygrad preloaded before source admission')
    private_cache = OUT / 'private-cache'
    private_cache.mkdir(mode=0o700)
    database = private_cache / 'cache.db'
    need(not database.exists(), 'cache must start absent')
    # 새 pycache prefix에서만 bytecode를 찾고 -B로 쓰지 않는다. 사용자 cache는 읽지 않는다.
    sys.pycache_prefix = str(OUT / 'unused-pycache')
    os.environ.update(CACHEDB=str(database), CCACHE='1', CACHELEVEL='2', DEBUG='7',
                      ALLOW_DEVICE_USAGE='0', PROFILE='0', VIZ='0')
    sys.path.insert(0, str(D0))
    save('python-environment.json', {'executable': sys.executable, 'version': sys.version, 'prefix': sys.prefix,
         'base_prefix': sys.base_prefix, 'source_prefix_D0': str(D0), 'pycache_prefix': sys.pycache_prefix,
         'controls': {k: os.environ[k] for k in ('CACHEDB', 'CCACHE', 'CACHELEVEL', 'DEBUG', 'ALLOW_DEVICE_USAGE', 'PROFILE', 'VIZ', 'LLVM_PATH')}})
    with (OUT / 'original-debug7.log').open('x') as debug, contextlib.redirect_stdout(debug):
        from tinygrad import helpers
        import tinygrad.device as device
        from tinygrad.runtime.support import compiler_cpu, elf
        from tinygrad.runtime.autogen import llvm
        for name, module in list(sys.modules.items()):
            if name == 'tinygrad' or name.startswith('tinygrad.'):
                need(Path(module.__file__).resolve().is_relative_to((D0 / 'tinygrad').resolve()), f'import outside D0: {name}')
        need(helpers.CACHEDB == str(database) and helpers._db_connection is None and not helpers._db_tables and not database.exists(), 'cache used during import')
        need(str(llvm.dll._name) == os.environ['LLVM_PATH'] and Path(llvm.dll._name).resolve() == provider, 'actual DLL provider differs')
        need(seal(llvm.dll._name)['sha256'] == LLVM_PIN, 'loaded provider SHA differs')
        compiler = compiler_cpu.CPULLVMCompiler(['x86_64', 'native'])
        def tm_string(getter):
            pointer = getter(compiler.target_machine)
            need(bool(pointer), 'null target-machine getter')
            try:
                return ctypes.string_at(pointer).decode()
            finally:
                llvm.LLVMDisposeMessage(pointer)
        config = {'library': os.environ['LLVM_PATH'], 'library_sha256': LLVM_PIN,
                  'triple': tm_string(llvm.LLVMGetTargetMachineTriple), 'cpu': tm_string(llvm.LLVMGetTargetMachineCPU),
                  'features': tm_string(llvm.LLVMGetTargetMachineFeatureString), 'passes': compiler.passes.decode(),
                  'loop_unrolling': True, 'loop_vectorization': True, 'slp_vectorization': True, 'verify_each': True}
        save('natural-config.json', config)
        # target은 natural getter 결과다. reference는 비교/중단 조건으로만 사용한다.
        need(config == pair_config, 'natural target differs from successful pair; do not force CPU/features')
        version = [ctypes.c_uint32() for _ in range(3)]
        llvm.LLVMGetVersion(*(ctypes.byref(v) for v in version))
        need([v.value for v in version] == [18, 1, 3], 'LLVM version differs')
        need(compiler.cachekey and compiler.jit and not device.Device._opened_devices, 'real cached JIT compiler required without device')
        need(compiler.compile_cached.__func__ is device.Compiler.compile_cached and
             compiler.compile.__func__ is compiler_cpu.LLVMCompiler.compile, 'unexpected compiler entrypoint')
        real_get, real_put = device.diskcache_get, device.diskcache_put
        real_obj, real_link = compiler.compile_to_obj, compiler_cpu.jit_loader
        need(real_get is helpers.diskcache_get and real_put is helpers.diskcache_put and real_link is elf.jit_loader, 'unexpected original delegate')
        save('actual-cache-key.json', {'table': compiler.cachekey, 'database_version': helpers.VERSION,
             'key_type': 'str', 'key': source, 'key_utf8_sha256': IR_PIN, 'key_utf8_bytes': 166049,
             'delegates': {n: f'{f.__module__}.{f.__qualname__}' for n, f in
                           [('get', real_get), ('put', real_put), ('compile_to_obj', real_obj), ('jit_loader', real_link)]}})
        imports_before = imports()
        save('imports.before.json', imports_before)
        mapped_provider('before')
        observed = {}
        def event(kind, **values):
            row = {'sequence': len(events), 'phase': phase, 'kind': kind, **values}
            events.append(row)
            with (OUT / 'events.jsonl').open('a') as handle:
                handle.write(json.dumps(row, sort_keys=True) + '\n')
                handle.flush()
        def check_key(table, key):
            need(table == compiler.cachekey and type(key) is str and key == source, 'unexpected actual cache key')
        def get(table, key):
            calls['get'] += 1
            check_key(table, key)
            value = real_get(table, key)
            need(value is None or type(value) is bytes, 'unexpected cache value type')
            event('get', hit=value is not None, value_sha256=None if value is None else sha(value), table=table, key_sha256=sha(key.encode()))
            return value
        def put(table, key, value):
            calls['put'] += 1
            check_key(table, key)
            need(value is observed.get('linked'), 'put value must be actual linker return')
            returned = real_put(table, key, value)
            event('put', value_sha256=sha(value), returned_sha256=sha(returned))
            need(returned is value, 'real put returned unexpected value')
            return returned
        def to_obj(src):
            calls['compile_to_obj'] += 1
            need(src == source and calls['compile_to_obj'] == 1, 'compile input/count differs')
            obj = real_obj(src)
            need(type(obj) is bytes and 0 < len(obj) <= LIMIT and obj.startswith(b'\x7fELF'), 'fresh object bound/type')
            observed['object'] = obj
            write('fresh.o', obj)
            event('compile_to_obj', source_sha256=sha(src.encode()), object_sha256=sha(obj), object_bytes=len(obj))
            return obj
        def link(obj):
            calls['jit_loader'] += 1
            need(obj is observed.get('object') and calls['jit_loader'] == 1, 'link input must be actual compile_to_obj return')
            image = real_link(obj)
            need(type(image) is bytes and 0 < len(image) <= LIMIT, 'runtime image bound/type')
            observed['linked'] = image
            event('jit_loader', object_sha256=sha(obj), image_sha256=sha(image), image_bytes=len(image), base=0, link_libs=None)
            return image
        restore = (device, compiler_cpu, compiler, real_get, real_put, real_obj, real_link)
        device.diskcache_get, device.diskcache_put = get, put
        compiler.compile_to_obj, compiler_cpu.jit_loader = to_obj, link
        phase = 'first-compile-cached'
        first = compiler.compile_cached(source)
        write('miss.runtime-image.bin', first)
        need(first is observed.get('linked'), 'first return must be actual linker return')
        phase = 'second-compile-cached'
        second = compiler.compile_cached(source)
        write('hit.runtime-image.bin', second)
        need(calls == {'get': 2, 'put': 1, 'compile_to_obj': 1, 'jit_loader': 1}, 'unexpected cache protocol call counts')
        need([e['kind'] for e in events] == ['get', 'compile_to_obj', 'jit_loader', 'put', 'get'] and
             events[0]['hit'] is False and events[-1]['hit'] is True and second == first, 'actual miss/hit/return protocol failed')
        need(not device.Device._opened_devices, 'unexpected device initialization')
        # 실제 SQLite row를 읽되 pickle.loads를 별도로 호출하거나 cache 값을 만들지 않는다.
        table = compiler.cachekey + '_' + str(helpers.VERSION)
        quoted = '"' + table.replace('"', '""') + '"'
        connection = helpers._db_connection
        need(connection is not None, 'real cache never opened SQLite')
        rows = connection.execute(f'SELECT key, val FROM {quoted}').fetchall()
        tables = connection.execute("SELECT name FROM sqlite_master WHERE type='table'").fetchall()
        need(len(rows) == 1 and rows[0][0] == source and tables == [(table,)], 'unexpected private cache rows/tables')
        save('cache-row.json', {'table': table, 'row_count': len(rows), 'key_sha256': sha(rows[0][0].encode()),
             'serialized_value_sha256': sha(rows[0][1]), 'serialized_value_bytes': len(rows[0][1])})
        connection.close()
        helpers._db_connection = None
    first_difference = next((i for i, (a, b) in enumerate(zip(first, frozen)) if a != b), None)
    if first_difference is None and len(first) != len(frozen):
        first_difference = min(len(first), len(frozen))
    status.update(cache_protocol_complete=True, fresh_image_sha256=sha(first), hit_image_sha256=sha(second),
                  fresh_object_sha256=sha(observed['object']), fresh_image_bytes=len(first), frozen_image_bytes=len(frozen),
                  frozen_image_sha256=sha(frozen), fresh_image_equals_frozen=first == frozen, first_different_byte=first_difference)
except BaseException as error:
    status['error'] = str(error)
    traceback.print_exc()
    raise
finally:
    try:
        if restore is not None:
            device, compiler_cpu, compiler, real_get, real_put, real_obj, real_link = restore
            device.diskcache_get, device.diskcache_put = real_get, real_put
            compiler.compile_to_obj, compiler_cpu.jit_loader = real_obj, real_link
        if helpers is not None and helpers._db_connection is not None:
            helpers._db_connection.close()
            helpers._db_connection = None
        after = {label: {'path': row['path'], 'fd': row['fd'], 'seal': seal(row['path'])} for label, row in held.items()}
        save('providers-inputs.after.json', after)
        need(after == held and all(identity(os.fstat(row['fd'])) == row['seal']['identity'] for row in held.values()), 'held provider/input changed')
        if source_before is not None:
            source_after = sources()
            save('sources.after.json', source_after)
            need(source_before == source_after, 'source roster/content changed')
            need(subprocess.check_output(['git', '-C', str(ROOT), 'rev-parse', 'HEAD'], text=True, timeout=5).strip() == os.environ['EXPECTED_SOURCE'], 'source HEAD changed')
        if imports_before is not None:
            imports_after = imports()
            save('imports.after.json', imports_after)
            need(imports_after == imports_before, 'import roster/content changed')
            mapped_provider('after')
        cache_files = sorted((OUT / 'private-cache').glob('*'))
        need(len(cache_files) <= 3, 'private cache file count bound')
        save('cache-files.after.json', {p.name: seal(p, LIMIT) for p in cache_files})
        status['final_guards_passed'] = True
    except BaseException as error:
        status.update(final_guards_passed=False, final_guard_error=str(error))
        raise
    finally:
        status.update(calls=calls, last_phase=phase)
        save('comparison.json', status)
        for row in held.values():
            os.close(row['fd'])
CACHE_PY

jq -e '.cache_protocol_complete == true and .final_guards_passed == true and .objects_executed == false and .frozen_image_executed == false and .model_or_historical_acceptance == false' \
  "$CACHE_PROBE_OUTPUT/comparison.json" >/dev/null
printf 'fresh original cache protocol observed only; historical acceptance remains false\n' > "$CACHE_PROBE_OUTPUT/complete.txt"
