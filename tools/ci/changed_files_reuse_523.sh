#!/usr/bin/env bash
# #523 CI 자료 admission 전용. 원본/후보/archived gate 실행 및 expected 재생성 없음.
set -euo pipefail
umask 077
fail() { printf 'changed-files reuse: %s\n' "$*" >&2; exit 1; }
[[ $# == 0 ]] || fail '인자는 받지 않음; 명시적 환경변수 필요'
[[ ${GITHUB_ACTIONS:-} == true && ${RUNNER_NAME:-} == bin-actions && ${RUNNER_OS:-} == Linux && ${RUNNER_ARCH:-} == X64 ]] || fail 'registered bin-actions Linux/X64 CI 전용'
[[ ${GITHUB_REPOSITORY:-} == bin9208/openpilot-rust ]] || fail '저장소 불일치'
for key in AST_REUSE_SOURCE EXPECTED_SOURCE AST_REUSE_PROOF AST_REUSE_PYTHON AST_REUSE_PYTHON_LIBRARY_PATH RUNNER_TEMP GH_TOKEN; do
  [[ -n ${!key:-} ]] || fail "필수 환경변수 없음: $key"
done
[[ $EXPECTED_SOURCE =~ ^[0-9a-f]{40}$ ]] || fail 'EXPECTED_SOURCE 형식'
[[ -z ${LD_PRELOAD:-} && -z ${GCONV_PATH:-} && -z ${GCONV_MODULES_CACHE:-} ]] || fail 'loader/GNU conversion override 금지'
for key in AST_REUSE_SOURCE AST_REUSE_PYTHON AST_REUSE_PYTHON_LIBRARY_PATH; do
  [[ ${!key} == /* && $(readlink -e -- "${!key}") == "${!key}" ]] || fail "canonical 절대 경로 필요: $key"
done
[[ -d $AST_REUSE_SOURCE && -x $AST_REUSE_PYTHON && -d $AST_REUSE_PYTHON_LIBRARY_PATH ]] || fail '준비된 source/interpreter/library 필요'
[[ -z ${LD_LIBRARY_PATH:-} || $LD_LIBRARY_PATH == "$AST_REUSE_PYTHON_LIBRARY_PATH" ]] || fail '다른 library search path 금지'
[[ $AST_REUSE_PROOF == /* && ! -e $AST_REUSE_PROOF && ! -L $AST_REUSE_PROOF ]] || fail 'proof는 존재하지 않는 새 절대 경로 필요'
reuse_parent=$(dirname -- "$AST_REUSE_PROOF")
[[ $(readlink -e -- "$reuse_parent") == "$reuse_parent" ]] || fail 'proof parent는 canonical 경로 필요'
reuse_temp=$(readlink -e -- "$RUNNER_TEMP")
[[ $reuse_parent == "$reuse_temp" || $reuse_parent == "$reuse_temp/"* ]] || fail 'proof는 RUNNER_TEMP 하위만 허용'
reuse_free=$(df -PB1 -- "$reuse_parent" | awk 'NR == 2 {print $4}')
[[ $reuse_free =~ ^[0-9]+$ ]] && ((reuse_free >= 128 * 1024 * 1024)) || fail '96 MiB ZIP + 32 MiB metadata 증가량 부족'
[[ -x /usr/bin/git && -x /usr/bin/gh && -x /usr/bin/timeout ]] || fail '기존 git/gh/GNU timeout 필요; 설치하지 않음'
mkdir -m 700 -- "$AST_REUSE_PROOF"
set -C
finish() {
  local result=$?
  trap - EXIT
  printf '%s\n' "$result" > "$AST_REUSE_PROOF/exit-status" || result=1
  exit "$result"
}
trap finish EXIT
# export 속성만 정리하여 token을 env(1)의 argv에 넣지 않는다.
# metadata Python에만 지정한 libpython 경로를 전달한다. Git/GH 자식에는 전달하지 않는다.
(
  while IFS= read -r reuse_key; do
    case "$reuse_key" in
      GH_TOKEN|AST_REUSE_SOURCE|EXPECTED_SOURCE|AST_REUSE_PROOF|AST_REUSE_PYTHON|AST_REUSE_PYTHON_LIBRARY_PATH) ;;
      *) export -n "$reuse_key" ;;
    esac
  done < <(compgen -e)
  export PATH=/usr/bin:/bin HOME="$AST_REUSE_PROOF" LANG=C.UTF-8
  export LD_LIBRARY_PATH="$AST_REUSE_PYTHON_LIBRARY_PATH"
  export GH_TOKEN AST_REUSE_SOURCE EXPECTED_SOURCE AST_REUSE_PROOF AST_REUSE_PYTHON AST_REUSE_PYTHON_LIBRARY_PATH
  exec /usr/bin/timeout --signal=KILL 1200s \
    "$AST_REUSE_PYTHON" -I -B -S -X "pycache_prefix=$AST_REUSE_PROOF/unused-pycache" -
) > "$AST_REUSE_PROOF/admission.stdout" 2> "$AST_REUSE_PROOF/admission.stderr" <<'REUSE_PY'
import base64
import copy
import difflib
import gzip
import hashlib
import io
import json
import os
from pathlib import Path
import re
import resource
import selectors
import stat
import subprocess
import sys
import tarfile
import time
import tomllib
import traceback
import zipfile

RECIPE = 'ab9fef829e8131c6d960ebf46e63c4ab538dbf23'
RECIPE_PATH = '.github/workflows/changed-files-caller-296-once.yml'
RECIPE_SHA = '6cd5f05d274890fd8cdbf9e38adb2bfd87828ce6cfbd4ac8e48858ea139615d2'
MANIFEST_SHA = '52e8d27ba4edb8c23fbd73ea17c13a1f8e307214c86ba6b17d97713849571955'
BASE = 'cfd7583ab1aab6cece048f9c1d64a144956ec906'
REPO_ID = 1395501963
DEV = 'rust/crates/dev-tools/'
SEMANTIC = DEV + 'Cargo.toml::excluding-test-registration'
# 부모 검토용 고정 postimage. 외부 env/새 HEAD로 자동 갱신하지 않는다.
# prepare-changed-files-context의 정확한 path/required-features 등록을 포함한 Cargo semantic 핀.
# 새 builder/wrapper는 옛58파일 baseline 외 current binding proof 대상이며 unknown diff 허용 없음.
OVERLAY = {
    'rust/Cargo.lock': 'a82648159767a21268e61ed4f2aba64940b7458733333ece0fab653017929ea0',
    DEV + 'src/lib.rs': 'b817e5ccecebacdf904632a5d8705b266c6e00998516e5b13092c9c8c9be9412',
    SEMANTIC: 'cc35942a27ad0ba982fca4147ab4ef32726511bbfd2a80ddecc58caa475f333f',
}
JOBS = {
    'initial-subset': 119943476311, 'focused-registry': 119982392508,
    'japanese': 119982359498, 'idna': 119986617957, 'payloads': 119991257461,
    'hz': 119993854190, 'korean': 120008946381, 'gb': 120011723407,
    'big5': 120014613509, 'jp2': 120017501768, 'provider-completion': 120015788302,
    'cp949-original': 120031572902, 'cp949-native': 120049346740,
    'lifecycle': 119991917126,
}
ROOT = Path(os.environ['AST_REUSE_SOURCE'])
OUT = Path(os.environ['AST_REUSE_PROOF'])
EXPECTED = os.environ['EXPECTED_SOURCE']
resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
resource.setrlimit(resource.RLIMIT_FSIZE, (32 << 20, 32 << 20))
held = []
written = 0
metadata_written = 0
zip_bytes = 0
result = {
    'schema': 'changed-files-historical-data-admission-523-v1', 'passed': False,
    'source_commit': EXPECTED, 'recipe_commit': RECIPE, 'recipe_sha256': RECIPE_SHA,
    'manifest_sha256': MANIFEST_SHA, 'proofs': [],
    'qualification_inputs_reused': False, 'qualification_tree_rosters_reused': False,
    'fresh_caller_qualified': False, 'source_absence_qualified': False,
    'current_sdk_or_native_binary_qualified': False, 'retirement_credit': 0,
    'historical_extension_identity_retroactively_established': False,
    'crc_scope': 'selected ZIP members; selected lifecycle tar members, no corpus repeat',
}


def need(value, reason):
    if not value:
        raise RuntimeError(reason)


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def unique_json(raw):
    def pairs(items):
        value = {}
        for key, item in items:
            need(key not in value, f'duplicate JSON key: {key}')
            value[key] = item
        return value

    def constant(value):
        raise RuntimeError(f'non-JSON constant: {value}')

    return json.loads(raw, object_pairs_hook=pairs, parse_constant=constant)


def identity(info):
    return [info.st_dev, info.st_ino, info.st_uid, info.st_gid, info.st_mode,
            info.st_size, info.st_mtime_ns, info.st_ctime_ns]


proof_fd = os.open(OUT, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
proof_id = os.fstat(proof_fd)
need(OUT.resolve(strict=True) == OUT and proof_id.st_uid == os.getuid()
     and stat.S_IMODE(proof_id.st_mode) == 0o700,
     'proof owner/mode must be current UID/0700')


def save_raw(name, raw, archive=False):
    global written, metadata_written
    need(re.fullmatch(r'[a-z0-9_.-]+', name) is not None, 'fixed output name required')
    written += len(raw)
    need(written <= 128 << 20, 'total proof output exceeds 128 MiB')
    need(archive or len(raw) <= 8 << 20, 'selected metadata exceeds 8 MiB')
    if not archive:
        metadata_written += len(raw)
        need(metadata_written <= 32 << 20, 'total metadata exceeds 32 MiB')
    fd = os.open(name, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW,
                 0o600, dir_fd=proof_fd)
    with os.fdopen(fd, 'wb') as stream:
        stream.write(raw)


def save(name, value):
    save_raw(name, (json.dumps(value, sort_keys=True, indent=2, allow_nan=False) + '\n').encode())


class Held:
    def __init__(self, path, limit, owned=False):
        self.path, self.limit = Path(path), limit
        need(self.path.is_absolute() and self.path.resolve(strict=True) == self.path,
             f'noncanonical input: {path}')
        self.fd = os.open(self.path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
        held.append(self)
        self.before = os.fstat(self.fd)
        need(stat.S_ISREG(self.before.st_mode) and 0 <= self.before.st_size <= limit,
             f'bounded regular input required: {path}')
        need(not owned or self.before.st_uid == os.getuid(), f'foreign source: {path}')
        self.raw = self.read()
        self.pin = sha(self.raw)
        self.verify()

    def read(self):
        blocks, offset = [], 0
        while True:
            raw = os.pread(self.fd, min(1 << 20, self.limit + 1 - offset), offset)
            if not raw:
                break
            blocks.append(raw)
            offset += len(raw)
            need(offset <= self.limit, f'input grew beyond bound: {self.path}')
        return b''.join(blocks)

    def verify(self):
        need(self.path.resolve(strict=True) == self.path, f'input path changed: {self.path}')
        need(identity(os.fstat(self.fd)) == identity(self.before)
             == identity(self.path.lstat()), f'input identity changed: {self.path}')
        actual_sha = sha(self.read())
        need(actual_sha == self.pin, f'input bytes changed: {self.path}')
        actual = os.fstat(self.fd)
        need(identity(actual) == identity(self.before)
             == identity(self.path.lstat()), f'input changed during recheck: {self.path}')
        return {'actual_path': str(self.path), 'identity': identity(actual), 'sha256': actual_sha}

    def record(self):
        return self.verify()


CHILD_ENV = {
    'PATH': '/usr/bin:/bin', 'HOME': str(OUT), 'LANG': 'C.UTF-8',
    'GIT_CONFIG_NOSYSTEM': '1', 'GIT_CONFIG_GLOBAL': '/dev/null',
    'GIT_NO_REPLACE_OBJECTS': '1', 'GIT_TERMINAL_PROMPT': '0',
    'GH_PROMPT_DISABLED': '1', 'GH_NO_UPDATE_NOTIFIER': '1',
}


def command(argv, limit, network=False):
    # stdout/stderr의 pipe를 함께 drain한다. 다운로드 중 limit 초과/timeout 즉시 실패.
    env = dict(CHILD_ENV)
    if network:
        env['GH_TOKEN'] = os.environ['GH_TOKEN']
    proc = subprocess.Popen(argv, env=env, stdin=subprocess.DEVNULL,
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    chunks = {'out': bytearray(), 'err': bytearray()}
    deadline = time.monotonic() + (120 if network else 30)
    try:
        with selectors.DefaultSelector() as poll:
            for stream, name in ((proc.stdout, 'out'), (proc.stderr, 'err')):
                os.set_blocking(stream.fileno(), False)
                poll.register(stream, selectors.EVENT_READ, name)
            while poll.get_map():
                need(time.monotonic() < deadline, f'command timeout: {argv[0]}')
                for key, _ in poll.select(0.2):
                    raw = os.read(key.fileobj.fileno(), 65536)
                    if not raw:
                        poll.unregister(key.fileobj)
                        continue
                    chunks[key.data].extend(raw)
                    cap = limit if key.data == 'out' else 65536
                    need(len(chunks[key.data]) <= cap, f'bounded command output exceeded: {argv[0]}')
        remaining = deadline - time.monotonic()
        need(remaining > 0, f'command wait timeout: {argv[0]}')
        need(proc.wait(timeout=remaining) == 0,
             f'command failed: {argv[0]}: {bytes(chunks["err"]).decode(errors="replace")}')
        return bytes(chunks['out'])
    finally:
        if proc.poll() is None:
            proc.kill()
        proc.wait()
        proc.stdout.close()
        proc.stderr.close()


def git(*args, limit=8 << 20):
    return command(['/usr/bin/git', '-c', 'core.fsmonitor=false', '-C', str(ROOT), *args], limit)


def api(path, limit=1 << 20):
    return command(['/usr/bin/gh', 'api', '--hostname', 'github.com',
                    '-H', 'Cache-Control: no-cache',
                    'repos/bin9208/openpilot-rust/' + path], limit, network=True)


def historical_blob(ref, path):
    # shallow/private checkout에도 옛 Git object를 설치하지 않는다. 고정 ref의 데이터만 읽는다.
    response = unique_json(api('contents/' + path + '?ref=' + ref, limit=4 << 20))
    need(response['type'] == 'file' and response['path'] == path
         and response['encoding'] == 'base64', 'fixed historical blob metadata')
    raw = base64.b64decode(''.join(response['content'].splitlines()), validate=True)
    need(len(raw) == response['size'] <= 1 << 20, 'historical blob bound')
    return raw


def semantic(raw):
    value = tomllib.loads(raw.decode())
    value.pop('test', None)
    return value


def json_hash(value):
    return sha(json.dumps(value, sort_keys=True, separators=(',', ':')).encode())


def source_roster():
    names = {'rust/Cargo.toml', 'rust/Cargo.lock', DEV + 'src/lib.rs',
             DEV + 'src/changed_files_check.rs', DEV + 'src/bin/check_changed_files.rs',
             DEV + 'src/qa_process.rs'}
    folder = ROOT / DEV / 'src/changed_files_check'
    need(folder.resolve(strict=True) == folder and folder.is_dir(), 'source directory changed')
    for directory, dirs, files in os.walk(folder, followlinks=False):
        need(all(not (Path(directory) / n).is_symlink() for n in dirs), 'source directory symlink')
        names.update(str((Path(directory) / n).relative_to(ROOT)) for n in files)
    return names


def overlay_semantics(old_cargo, current_cargo, old_lock, current_lock):
    # 정확히 여섯 별도 feature/일곱 bin/네 optional dependency 추가만 허용한다.
    trimmed = copy.deepcopy(current_cargo)
    for name in ('route-sanitizer-native', 'kbhit-native', 'github-utils-native',
                 'developer-ssh-native', 'route-vault-bundle-native', 'current-catalog-export-native'):
        need(name not in old_cargo['features'], 'overlay feature already existed')
        del trimmed['features'][name]
    for name in ('openpilot-http-transport', 'openpilot-registration', 'ureq', 'cookie_store'):
        need(name not in old_cargo['dependencies'] and trimmed['dependencies'][name]['optional'] is True,
             'overlay dependency must be new and optional')
        del trimmed['dependencies'][name]
    new_bins = {'build-route-vault-bundle', 'export-current-driving-catalog', 'developer-ssh',
                'setup-ssh-keys', 'developer-auth', 'developer-auth-json', 'prepare-changed-files-context'}
    need({b['name'] for b in trimmed['bin']} - {b['name'] for b in old_cargo['bin']} == new_bins,
         'unexpected binary registration')
    trimmed['bin'] = [b for b in trimmed['bin'] if b['name'] not in new_bins]
    need(trimmed == old_cargo, 'unreviewed Cargo semantic difference')
    need(current_cargo['features']['changed-files-native']
         == old_cargo['features']['changed-files-native']
         == ['dep:ruff_python_parser', 'dep:ruff_python_ast', 'dep:unicode_names2'],
         'changed-files feature changed')
    trimmed_lock = copy.deepcopy(current_lock)
    packages = [p for p in trimmed_lock['package'] if p['name'] == 'openpilot-dev-tools']
    need(len(packages) == 1, 'ambiguous dev-tools lock package')
    for name in ('cookie_store', 'openpilot-http-transport', 'openpilot-registration', 'ureq'):
        packages[0]['dependencies'].remove(name)
    need(trimmed_lock == old_lock, 'unreviewed lock resolver difference')


def safe_name(name, directory=False):
    if directory and name.endswith('/'):
        name = name[:-1]
    need(isinstance(name, str) and name and '\0' not in name and not name.startswith('/')
         and all(p not in ('', '.', '..') for p in name.split('/')), 'unsafe archive name')


def zip_member(archive, name, limit=8 << 20):
    info = archive.getinfo(name)
    need(not info.is_dir() and 0 <= info.file_size <= limit
         and stat.S_IFMT(info.external_attr >> 16) in (0, stat.S_IFREG),
         f'bounded regular ZIP member required: {name}')
    raw = archive.read(info)  # 선택 member의 CRC도 zipfile이 검사한다.
    need(len(raw) == info.file_size, 'ZIP member length mismatch')
    return raw


def proof_members(archive, proof):
    wanted = {proof['receipt_member'], 'source.txt'}
    for name in wanted:
        safe_name(name)
    if proof['archive_layout'] == 'flat-zip':
        return {name: zip_member(archive, name) for name in wanted}
    need(proof['archive_layout'] == 'gnu-tar-in-zip-v1', 'unsupported proof layout')
    need(set(archive.namelist()) == {'evidence.tar.gz', 'evidence.tar.gz.sha256',
                                   'archive-members.json', 'packaging-status.json'}, 'lifecycle ZIP roster')
    payload = zip_member(archive, 'evidence.tar.gz')
    need(sha(payload) == proof['payload_archive_sha256'], 'lifecycle payload pin')
    need(zip_member(archive, 'evidence.tar.gz.sha256')
         == (sha(payload) + '  evidence.tar.gz\n').encode(), 'lifecycle payload SHA receipt')
    index = unique_json(zip_member(archive, 'archive-members.json'))
    status = unique_json(zip_member(archive, 'packaging-status.json'))
    need(isinstance(index, dict) and 0 < len(index) <= 10000, 'lifecycle index bound')
    need(status['archive_created'] is True and status['source_directory_present'] is True
         and status['raw_name_roundtrip_verified'] is True and status['source_mutation'] is False
         and status['native_acceptance_claimed'] is False and status['archive_sha256'] == sha(payload)
         and status['archive_bytes'] == len(payload) and status['member_count'] == len(index),
         'lifecycle packaging receipt')
    with gzip.GzipFile(fileobj=io.BytesIO(payload)) as stream:
        expanded = stream.read((256 << 20) + 1)
    need(len(expanded) <= 256 << 20, 'lifecycle expansion bound')
    selected, seen = {}, set()
    with tarfile.open(fileobj=io.BytesIO(expanded), mode='r:',
                      encoding='utf-8', errors='surrogateescape') as tar:
        for member in tar:
            name = member.name.rstrip('/') if member.isdir() else member.name
            safe_name(name)
            key = os.fsencode(name).hex()
            need(key in index and key not in seen, 'unindexed/duplicate tar member')
            seen.add(key)
            need(member.isfile() or member.isdir() or member.issym(), 'special/hardlink tar member')
            if name in wanted:
                need(member.isfile() and 0 <= member.size <= 8 << 20, 'selected tar member must be regular')
                with tar.extractfile(member) as stream:  # 메모리 read만; 파일시스템 extract 아님.
                    raw = stream.read((8 << 20) + 1)
                need(len(raw) == member.size == index[key]['bytes']
                     and index[key]['kind'] == 'file' and sha(raw) == index[key]['sha256'],
                     'selected tar member index pin')
                selected[name] = raw
    need(seen == set(index) and set(selected) == wanted, 'lifecycle selected member/roster missing')
    return selected


def main():
    global zip_bytes
    need(sys.version_info[:3] == (3, 12, 15), 'explicit CPython 3.12.15 required')
    need(ROOT.is_absolute() and ROOT.resolve(strict=True) == ROOT, 'canonical source root required')
    need(git('rev-parse', 'HEAD').decode().strip() == EXPECTED, 'source HEAD mismatch')
    recipe_raw = historical_blob(RECIPE, RECIPE_PATH)
    need(len(recipe_raw) == 82705 and sha(recipe_raw) == RECIPE_SHA, 'fixed recipe bytes mismatch')
    recipe = unique_json(recipe_raw)  # 이 고정 workflow는 JSON 형식이다. shell run은 실행하지 않는다.
    steps = next(iter(recipe['jobs'].values()))['steps']
    chunks = {}
    for step in steps:
        match = re.fullmatch(r'Stage sealed chunk accepted-gates\.json ([0-2])', step.get('name', ''))
        if match:
            ordinal = int(match[1])
            need(ordinal not in chunks, 'duplicate manifest chunk')
            encoded = re.fullmatch(r"set -euo pipefail\nprintf '%s' '([A-Za-z0-9+/=]+)' > [^\n]+\n", step['run'])
            need(encoded is not None, 'unexpected fixed chunk recipe')
            chunks[ordinal] = encoded[1]
    need(set(chunks) == {0, 1, 2}, 'missing manifest chunks')
    compressed = base64.b64decode(''.join(chunks[i] for i in range(3)), validate=True)
    with gzip.GzipFile(fileobj=io.BytesIO(compressed)) as stream:
        raw = stream.read((1 << 20) + 1)
    need(len(raw) == 58790 and sha(raw) == MANIFEST_SHA, 'accepted manifest pin mismatch')
    save_raw('accepted-gates.json', raw)
    manifest = unique_json(raw)
    need(manifest['schema'] == 'changed-files-retirement-gates-v1'
         and manifest['reviewed_source_base'] == BASE, 'manifest schema/base mismatch')
    # 과거 qualification_inputs/전체 tree 수용은 이식하지 않는다. 아래59항목만 비교한다.
    pins = manifest['reviewed_native_inputs_sha256']
    need(len(pins) == 59 and {n for n in pins if '::' in n} == {SEMANTIC}, 'native input schema')
    names = source_roster()
    need(names == set(pins) - {SEMANTIC} and len(names) == 58, 'native input roster changed')
    source = {name: Held(ROOT / name, 4 << 20, owned=True) for name in sorted(names)}
    cargo = Held(ROOT / DEV / 'Cargo.toml', 1 << 20, owned=True)
    script = Held(ROOT / 'tools/ci/changed_files_reuse_523.sh', 1 << 20, owned=True)
    current = {name: value.pin for name, value in source.items()}
    current[SEMANTIC] = json_hash(semantic(cargo.raw))
    expected_pins = {**pins, **OVERLAY}
    need(current == expected_pins, 'unreviewed current source or overlay pin mismatch')
    need(sum(current[n] == pins[n] for n in names) == 56, 'expected 56 unchanged physical inputs')
    old_cargo = historical_blob(BASE, DEV + 'Cargo.toml')
    old_lock = historical_blob(BASE, 'rust/Cargo.lock')
    old_lib = historical_blob(BASE, DEV + 'src/lib.rs')
    need(json_hash(semantic(old_cargo)) == pins[SEMANTIC]
         and sha(old_lock) == pins['rust/Cargo.lock'] and sha(old_lib) == pins[DEV + 'src/lib.rs'],
         'overlay base differs from historical pins')
    overlay_semantics(semantic(old_cargo), semantic(cargo.raw), tomllib.loads(old_lock.decode()),
                      tomllib.loads(source['rust/Cargo.lock'].raw.decode()))
    save_raw('lib-overlay.diff', ''.join(difflib.unified_diff(
        old_lib.decode().splitlines(keepends=True), source[DEV + 'src/lib.rs'].raw.decode().splitlines(keepends=True),
        fromfile=BASE + '/lib.rs', tofile=EXPECTED + '/lib.rs')).encode())
    save('source.before.json', {**{n: v.record() for n, v in source.items()},
                                DEV + 'Cargo.toml': cargo.record(), 'admission_script': script.record()})
    save('current-overlay.json', {'historical_inputs_sha256': pins, 'current_inputs_sha256': current,
                                'overlay_sha256': OVERLAY, 'unchanged_files': 56,
                                'test_registration_excluded_only_from_cargo_semantic_hash': True,
                                'full_cargo_toml_sha256': cargo.pin})
    providers = manifest['cp949_runtime_providers_sha256']
    need(len(providers) == 8, 'exact eight GNU runtime providers required')
    provider_files = {name: Held(name, 8 << 20) for name in sorted(providers)}
    save('gnu-providers.before.json', {n: v.record() for n, v in provider_files.items()})
    need({n: v.pin for n, v in provider_files.items()} == providers, 'current GNU provider path/hash differs')
    interpreter = Held(os.environ['AST_REUSE_PYTHON'], 128 << 20)
    library = Held(Path(os.environ['AST_REUSE_PYTHON_LIBRARY_PATH']) / 'libpython3.12.so.1.0', 128 << 20)
    need(Path(sys.executable) == interpreter.path, 'interpreter path mismatch')
    maps = [line.split(maxsplit=5) for line in Path('/proc/self/maps').read_text().splitlines()
            if 'libpython' in line]
    info = library.before
    device = f'{os.major(info.st_dev):02x}:{os.minor(info.st_dev):02x}'
    need(maps and all(len(row) == 6 and row[3] == device and int(row[4]) == info.st_ino
                      and row[5] == str(library.path) for row in maps), 'actual mapped libpython mismatch')
    save('metadata-python.before.json', {'version': sys.version, 'interpreter': interpreter.record(),
                                         'library': library.record()})
    need(manifest['coverage_closure_accepted'] is True and set(manifest['gates']) == {'codec', 'lifecycle'},
         'historical group acceptance missing')
    proofs = []
    for group in manifest['gates'].values():
        need(group['accepted'] is True and group['remaining_requirements'] == [], 'incomplete historical group')
        proofs.extend(group['proofs'])
    need(len(proofs) == 14 and {p['scope'] for p in proofs} == set(JOBS), '14 unique proof scopes required')
    need(sum(p['archive_bytes'] for p in proofs) == 80563694 <= 96 << 20, 'fixed total ZIP size budget')
    for proof in proofs:
        scope = proof['scope']
        need(proof['accepted'] is True and 0 < proof['archive_bytes'] <= 24 << 20, 'proof acceptance/size')
        meta = {}
        for kind, path in [('run', f'actions/runs/{proof["run_id"]}'),
                           ('job', f'actions/jobs/{JOBS[scope]}'),
                           ('artifact', f'actions/artifacts/{proof["artifact_id"]}')]:
            response = api(path)
            save_raw(scope + '.' + kind + '.json', response)
            meta[kind] = unique_json(response)
        run, job, artifact = meta['run'], meta['job'], meta['artifact']
        need(run['id'] == proof['run_id'] and run['status'] == 'completed'
             and run['conclusion'] == 'success' and run['head_sha'] == proof['workflow_sha']
             and run['repository']['id'] == REPO_ID, 'actual run binding/success mismatch')
        need(job['id'] == JOBS[scope] and job['run_id'] == run['id']
             and job['head_sha'] == run['head_sha'] and job['status'] == 'completed'
             and job['conclusion'] == 'success', 'actual job binding/success mismatch')
        need(artifact['id'] == proof['artifact_id'] and artifact['expired'] is False
             and artifact['workflow_run']['id'] == run['id']
             and artifact['workflow_run']['head_sha'] == run['head_sha']
             and artifact['workflow_run']['repository_id'] == REPO_ID
             and artifact['workflow_run']['head_repository_id'] == REPO_ID
             and artifact['digest'] == 'sha256:' + proof['archive_sha256']
             and artifact['size_in_bytes'] == proof['archive_bytes'], 'artifact metadata binding mismatch')
        archive_raw = api(f'actions/artifacts/{proof["artifact_id"]}/zip', limit=proof['archive_bytes'])
        zip_bytes += len(archive_raw)
        need(zip_bytes <= 96 << 20 and len(archive_raw) == proof['archive_bytes']
             and sha(archive_raw) == proof['archive_sha256'], 'whole ZIP size/SHA mismatch')
        save_raw(scope + '.zip', archive_raw, archive=True)
        with zipfile.ZipFile(io.BytesIO(archive_raw)) as archive:
            infos = archive.infolist()
            need(len(infos) == len(set(archive.namelist())) == proof['archive_members'], 'ZIP roster/count')
            need(sum(i.file_size for i in infos) <= 256 << 20, 'ZIP expanded size bound')
            for item in infos:
                safe_name(item.filename, item.is_dir())
                need(not item.flag_bits & 1, 'encrypted ZIP unsupported')
            members = proof_members(archive, proof)
            receipt_raw = members[proof['receipt_member']]
            need(sha(receipt_raw) == proof['receipt_sha256'], 'selected receipt SHA mismatch')
            receipt = unique_json(receipt_raw)
            need(receipt['passed'] is True and receipt['source'] == proof['source_sha'], 'receipt passed/source pin')
            # 원문 flag·한계는 raw receipt 그대로 보존한다. 새로운 provider 완결성 주장 없음.
            if scope == 'provider-completion':
                need(receipt['native_candidate_execution'] is False
                     and receipt['native_tables_used_as_expected'] is False, 'original-only receipt boundary')
            if scope == 'cp949-native':
                routing_before = zip_member(archive, 'gnu-routing.before.json')
                routing_after = zip_member(archive, 'gnu-routing.after.json')
                native_before = zip_member(archive, 'actual/native-providers.before.json')
                native_after = zip_member(archive, 'actual/native-providers.after.json')
                need(routing_before == routing_after and native_before == native_after, 'historical provider drift')
                bound = unique_json(routing_before)
                bound.update({p: seal['sha256'] for p, seal in unique_json(native_before).items()
                              if '.so' in Path(p).name})
                need(bound == providers and receipt['backend'] == 'current-gnu-iconv-cp949', 'actual GNU binding')
                for name, data in [('routing-before', routing_before), ('routing-after', routing_after),
                                   ('native-before', native_before), ('native-after', native_after)]:
                    save_raw('cp949-native.' + name + '.json', data)
            lines = members['source.txt'].decode().splitlines()
            need(all('=' in line for line in lines), 'malformed source provenance')
            provenance = dict(line.split('=', 1) for line in lines)
            need(len(provenance) == len(lines) and provenance['source_commit'] == proof['source_sha']
                 and provenance['workflow_head'] == proof['workflow_sha'], 'selected source/workflow binding')
            save_raw(scope + '.receipt.json', receipt_raw)
            save_raw(scope + '.source.txt', members['source.txt'])
        result['proofs'].append({**proof, 'actual_job_id': JOBS[scope],
                                 'observed_run_conclusion': run['conclusion'],
                                 'observed_job_conclusion': job['conclusion'],
                                 'selected_source_sha256': sha(members['source.txt'])})
    need(len(result['proofs']) == 14 and zip_bytes == 80563694, 'incomplete historical admission')
    need(source_roster() == names and git('rev-parse', 'HEAD').decode().strip() == EXPECTED,
         'source roster/HEAD changed during admission')
    for item in held:
        item.verify()
    save('source.after.json', {**{n: v.record() for n, v in source.items()},
                               DEV + 'Cargo.toml': cargo.record(), 'admission_script': script.record()})
    save('gnu-providers.after.json', {n: v.record() for n, v in provider_files.items()})
    save('metadata-python.after.json', {'version': sys.version, 'interpreter': interpreter.record(),
                                        'library': library.record()})
    need(OUT.resolve(strict=True) == OUT and identity(OUT.lstat())[:5] == identity(proof_id)[:5]
         and identity(os.fstat(proof_fd))[:5] == identity(proof_id)[:5], 'proof directory replaced')
    result.update(passed=True, admitted_proofs=14, zip_download_bytes=zip_bytes,
                  current_native_input_overlay_verified=True,
                  gnu_provider_path_hash_before_after_verified=True)


try:
    main()
except BaseException as error:
    result['error'] = f'{type(error).__name__}: {error}'
    # 실패에도 열린 입력의 변화 여부를 남기며, stale PASS는 새 proof/create_new로 불가능하다.
    failures = []
    for item in held:
        try:
            item.verify()
        except BaseException as changed:
            failures.append(str(changed))
    result['input_recheck_failures'] = failures
    traceback.print_exc()
finally:
    save('reuse-admission.json', result)
    for item in held:
        os.close(item.fd)
    os.close(proof_fd)
if not result['passed']:
    sys.exit(1)
print(json.dumps({'passed': True, 'admitted_proofs': 14, 'zip_bytes': zip_bytes,
                  'fresh_caller_qualified': False, 'retirement_credit': 0}))
REUSE_PY
