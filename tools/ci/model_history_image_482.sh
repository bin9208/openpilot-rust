#!/usr/bin/env bash
# #482 CI용 inert 원문 fetch. compiler/native/reconstruct/PKL/script를 실행하지 않는다.
# 부모가 준비: ORACLE_PYTHON(절대3.12.15), OPENPILOT_ORACLE_PYTHON_LIBRARY_PATH,
# GH_TOKEN(actions:read), HISTORY_FETCH_PROOF(기존 private proof 절대 directory).
# 예: HISTORY_FETCH_PROOF="$CI_JOB_TEMP/proof" bash /absolute/model-history-object-fetch.sh
# 출력: history92.o, history92.ll, history92.frozen.bin, baseline ZIP 및 metadata/receipt.
# 로컬 검증은 bash -n / embedded ast.parse뿐이다. 실제 fetch/수용은 CI UNRUN.
# gh api --method HEAD --include는 redirect를 HEAD로 따라간다(거대 GET 안 함).
# GH_DEBUG=api의 response Location은 bounded pipe로만 받아 메모리에서 파싱한다.
# stdout/stderr 원문, Authorization/token, signed Location은 출력·파일 저장하지 않는다.
# source artifact 전체 hash는 API metadata다. 전체4.39GB hash/CRC 독립 검증 아님.
set +x
set -euo pipefail
umask 077
[[ $# == 0 ]] || { printf '%s\n' 'history92 fetch: 인자 대신 명시적 환경변수 사용' >&2; exit 1; }
[[ ${GITHUB_ACTIONS:-} == true && ${RUNNER_NAME:-} == bin-actions && ${RUNNER_OS:-} == Linux && ${GITHUB_REPOSITORY:-} == bin9208/openpilot-rust ]] || {
  printf '%s\n' 'history92 fetch: bin-actions Linux CI 전용' >&2; exit 1;
}
for key in ORACLE_PYTHON OPENPILOT_ORACLE_PYTHON_LIBRARY_PATH HISTORY_FETCH_PROOF GH_TOKEN; do
  [[ -n ${!key:-} ]] || { printf 'history92 fetch: 필수 환경변수 없음: %s\n' "$key" >&2; exit 1; }
done
[[ $ORACLE_PYTHON == /* && -x $ORACLE_PYTHON && $OPENPILOT_ORACLE_PYTHON_LIBRARY_PATH == /* && $HISTORY_FETCH_PROOF == /* ]] || exit 1
command -v gh >/dev/null
command -v timeout >/dev/null
# GNU timeout의 기본 별도 process group. 모든 gh 자식도 같은 그룹에 남는다.
LD_LIBRARY_PATH="$OPENPILOT_ORACLE_PYTHON_LIBRARY_PATH" \
  timeout --signal=KILL 150s "$ORACLE_PYTHON" -I -B -S - <<'HISTORY_PY'
import hashlib
import io
import json
import os
from pathlib import Path
import re
import selectors
import stat
import struct
import subprocess
import sys
import time
import urllib.parse
import urllib.request
import zipfile
import zlib

LIMIT = 20_000_000
MAX_MEMBER = 1 << 20
transferred = 0
phase = '초기 경로/공간 검사'
range_receipts = []
outfd = None

SOURCE = {
    'id': 12365522894, 'name': 'driving-native-independent-296',
    'size': 4388618662, 'run': 39635596188,
    'head': '8e77eed65b9c68ada39f0415f7a67c8164c34f26',
    'sha256': 'ba088fa5a2ba3d85d416ca5a2fb8f70b79ee4355be3241d05ce6a1175d72dff9',
}
BASELINE = {
    'id': 12404776700, 'name': 'model-road-isolated-kernel92-296-1',
    'size': 4391525, 'run': 39729562958,
    'head': '21ed562971a92e4fea74ea777c189fad36e7bc2b',
    'sha256': 'ba28ae07b5becc3afe4346db3e86437f9a93aaaf02031c19f83e070ad9c7d93b',
}
SOURCE_COMMIT = 'e25a14386582858dae25c6daca5ced3bfb4bacab'
PKL_SHA = '487ef8698775a593726c79ef0651940480f755f5ded57f672d0ee3a4bd8a624f'
PREFIX = 'driving-native-296/complete-fixture/catalog/e776e00928c8166d2cb5d47874d43f2aa860161f5c0110dc556210fa42805534/'
CENTRAL = (4384660480, 3958084, 'efd7a02d7bb297cb2b2d51605da17ddcfaa36a592dc9c191753fc6674e8c3f3b')
# name / LFH offset / compressed bytes / raw bytes / CRC32 / raw SHA256.
MEMBERS = {
    'history92.o': (PREFIX + 'driving-1344x760/kernel-92.o', 16045629, 6857, 15928,
        '0f01a7fe', 'bf56a30c37f0128497b626329df7239dd7a1c4b5dd9ec728d501f2c11deebea1'),
    'history92.ll': (PREFIX + 'driving-1344x760/kernel-92.txt', 16052669, 29492, 166049,
        '5c56f4bd', 'e223a2c36c6bde7cf676bb92fd2529858a723f9ecfa91c811b2b941756a9bea0'),
    'history92-fixture.json': ('driving-native-296/complete-fixture/fixture.json', 128964152, 51746, 310291,
        '243b24dc', 'afe1c795d7f609801eb4cd5a5a7748c3c996929403a3c0fa102f232f2a8d248d'),
}
FROZEN = (14442, '324697e4d28a400da3c521fea2f9140eea04d50f19e6771a96549f4b8a5a6767')
BASE_MEMBERS = {
    'history92.frozen.bin': ('first-kernel/original-kernel92-code.bin', *FROZEN),
    'history92-binding.json': ('first-kernel/program-binding.json', 967,
        'e9353a5e24b982c1cbe45f9e02bb479a867273115971da9104d03f7d06e78728'),
    'history92-context.json': ('first-kernel/context.json', 396,
        'a6a1f6b07d38a54e85537df7655fced57139e77dfdb195c0c9e16b44dca9f02a'),
}

class FetchFailure(Exception):
    pass

def need(value, message):
    if not value:
        raise FetchFailure(message)

def sha(data):
    return hashlib.sha256(data).hexdigest()

def charge(count):
    global transferred
    transferred += count
    need(transferred <= LIMIT, '전송 상한 초과')

def gh_api(endpoint, head=False):
    # 헤더/디버그를 파일이나 콘솔에 넘기지 않는다. 양쪽 pipe는 각각64 KiB 제한.
    env = dict(os.environ)
    env.pop('GH_DEBUG', None)
    if head:
        env['GH_DEBUG'] = 'api'
    args = ['gh', 'api', '--hostname', 'github.com', '-H', 'Cache-Control: no-cache']
    if head:
        args += ['--method', 'HEAD', '--include']
    child = subprocess.Popen(args + [endpoint], env=env, stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    output = {child.stdout: bytearray(), child.stderr: bytearray()}
    deadline = time.monotonic() + 30
    try:
        with selectors.DefaultSelector() as ready:
            for stream in output:
                os.set_blocking(stream.fileno(), False)
                ready.register(stream, selectors.EVENT_READ)
            while ready.get_map():
                need(time.monotonic() < deadline, 'gh 제한시간 초과')
                for key, _ in ready.select(0.1):
                    data = os.read(key.fd, 65536)
                    charge(len(data))
                    if not data:
                        ready.unregister(key.fileobj)
                    else:
                        output[key.fileobj].extend(data)
                        need(len(output[key.fileobj]) <= 65536, 'gh 응답64 KiB 상한 초과')
        need(child.wait(timeout=max(0.1, deadline - time.monotonic())) == 0, 'gh API 실패')
        return bytes(output[child.stdout]), bytes(output[child.stderr])
    finally:
        if child.poll() is None:
            child.kill()
        child.wait()
        child.stdout.close()
        child.stderr.close()

def metadata(pin):
    raw, _ = gh_api(f"repos/bin9208/openpilot-rust/actions/artifacts/{pin['id']}")
    data = json.loads(raw)
    run = data['workflow_run']
    need(data['id'] == pin['id'] and data['name'] == pin['name']
        and data['size_in_bytes'] == pin['size'] and data['expired'] is False
        and data['digest'] == 'sha256:' + pin['sha256']
        and run['id'] == pin['run'] and run['head_sha'] == pin['head']
        and run['repository_id'] == 1395501963 and run['head_repository_id'] == 1395501963,
        'artifact metadata pin 불일치')
    # API download URL은 저장하지 않는다. 필요한 고정 필드만 receipt에 남긴다.
    return dict(pin, expired=False, repository_id=run['repository_id'])

def location(pin):
    stdout, stderr = gh_api(f"repos/bin9208/openpilot-rust/actions/artifacts/{pin['id']}/zip", head=True)
    found = re.findall(rb'(?im)^[<>]?\s*location:\s*(https://[^\s]+)', stdout + stderr)
    need(len(found) == 1, 'HEAD redirect Location 한 개 필요')
    url = found[0].decode('ascii')
    parsed = urllib.parse.urlsplit(url)
    need(parsed.scheme == 'https' and parsed.username is None and parsed.password is None
        and parsed.port is None and not parsed.fragment
        and re.fullmatch(r'productionresultssa[0-9]+\.blob\.core\.windows\.net', parsed.hostname or ''),
        '허용하지 않은 artifact download host')
    return url

class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None

opener = urllib.request.build_opener(NoRedirect)

def ranged(pin, url, start, length, role):
    need(0 <= start < pin['size'] and 0 < length <= 4 * 1024 * 1024
        and start + length <= pin['size'] and transferred + length <= LIMIT, 'Range/작업 상한 초과')
    request = urllib.request.Request(url, headers={
        'Range': f'bytes={start}-{start + length - 1}', 'Accept-Encoding': 'identity'})
    with opener.open(request, timeout=30) as response:
        need(response.status == 206 and response.headers.get('Content-Range') ==
            f"bytes {start}-{start + length - 1}/{pin['size']}"
            and response.headers.get('Content-Length') == str(length)
            and response.headers.get('Content-Encoding') in (None, 'identity'), '정확한206/Range/size 필요')
        data = response.read(length)
        charge(len(data))
        need(len(data) == length, '잘린 Range 응답')
    range_receipts.append({'artifact': pin['id'], 'role': role, 'start': start,
        'bytes': length, 'status': 206, 'sha256': sha(data)})
    return data

def source_directory(url):
    tail_start = SOURCE['size'] - 65557
    tail = ranged(SOURCE, url, tail_start, 65557, 'EOCD/ZIP64')
    need(sha(tail) == '1bffc9ea3d53ea1b3f6e718bedcaae5addb2888872e4e46fe66b4c94ffe26ee5', 'tail pin 불일치')
    at = tail.rfind(b'PK\x05\x06')
    need(at >= 20, 'EOCD 부재')
    end = struct.unpack_from('<4s4H2LH', tail, at)
    need(at + 22 + end[-1] == len(tail) and end[1:3] == (0, 0), 'EOCD/multidisk 불일치')
    locator = struct.unpack_from('<4sLQL', tail, at - 20)
    need(locator[0] == b'PK\x06\x07' and locator[1] == 0 and locator[3] == 1, 'ZIP64 locator 불일치')
    pos = locator[2] - tail_start
    need(0 <= pos and pos + 56 <= len(tail), 'ZIP64 EOCD tail 범위')
    z64 = struct.unpack_from('<4sQ2H2L4Q', tail, pos)
    need(z64[0] == b'PK\x06\x06' and z64[1] == 44 and z64[4:6] == (0, 0)
        and z64[6] == z64[7] == 28880 and z64[8:10] == (CENTRAL[1], CENTRAL[0]), 'ZIP64 directory pin 불일치')
    data = ranged(SOURCE, url, CENTRAL[0], CENTRAL[1], 'central directory')
    need(sha(data) == CENTRAL[2], 'central directory 전체 pin 불일치')
    wanted = {row[0]: row for row in MEMBERS.values()}
    found = {}
    names = set()
    pos = 0
    while pos < len(data):
        row = struct.unpack_from('<4s6H3L5H2L', data, pos)
        need(row[0] == b'PK\x01\x02', 'central entry signature')
        stop = pos + 46 + row[10] + row[11] + row[12]
        need(stop <= len(data), 'central entry 범위')
        name = data[pos + 46:pos + 46 + row[10]].decode('utf-8' if row[3] & 0x800 else 'cp437')
        need(name not in names, '중복 central member')
        names.add(name)
        if name in wanted:
            expected = wanted[name]
            need((row[16], row[8], row[9], f'{row[7]:08x}') == expected[1:5]
                and row[3] == 8 and row[4] == 8 and row[13] == 0, '선택 central entry binding 불일치')
            found[name] = expected
        pos = stop
    need(pos == len(data) and len(names) == 28880 and set(found) == set(wanted), 'directory 전체 roster/선택 member 불일치')

def source_member(url, row):
    name, offset, compressed, size, crc, pin = row
    need(max(compressed, size) <= MAX_MEMBER, '선택 member1 MiB 상한')
    header = ranged(SOURCE, url, offset, 30, 'LFH:' + name)
    fields = struct.unpack('<4s5H3L2H', header)
    need(fields[0] == b'PK\x03\x04' and fields[2] == 8 and fields[3] == 8
        and fields[6] in (0, int(crc, 16)) and fields[7] in (0, compressed)
        and fields[8] in (0, size), 'local header/central 형식 불일치')
    name_size, extra_size = fields[-2:]
    need(name_size == len(name.encode('ascii')) and extra_size <= 65535, 'LFH name/extra 범위')
    payload = ranged(SOURCE, url, offset + 30, name_size + extra_size + compressed, 'payload:' + name)
    need(payload[:name_size] == name.encode('ascii'), 'LFH member 이름 불일치')
    decoder = zlib.decompressobj(-15)
    data = decoder.decompress(payload[name_size + extra_size:], size + 1)
    need(decoder.eof and not decoder.unused_data and not decoder.unconsumed_tail
        and len(data) == size and f'{zlib.crc32(data) & 0xffffffff:08x}' == crc and sha(data) == pin,
        '선택 deflate 종료/size/CRC/SHA 불일치')
    return data

def baseline_inputs(url):
    raw = b''.join(ranged(BASELINE, url, start, min(4 * 1024 * 1024, BASELINE['size'] - start), 'baseline ZIP')
        for start in range(0, BASELINE['size'], 4 * 1024 * 1024))
    need(len(raw) == BASELINE['size'] and sha(raw) == BASELINE['sha256'], 'baseline 전체 ZIP SHA 불일치')
    result = {'history92-baseline.zip': raw}
    with zipfile.ZipFile(io.BytesIO(raw)) as archive:
        infos = archive.infolist()
        need(len(infos) == 133 and len({info.filename for info in infos}) == 133
            and sum(info.file_size for info in infos) <= 16 * 1024 * 1024, 'baseline roster/확장 상한')
        for output, (name, size, pin) in BASE_MEMBERS.items():
            info = archive.getinfo(name)
            need(not info.is_dir() and not info.flag_bits & 1 and info.file_size == size
                and max(info.file_size, info.compress_size) <= MAX_MEMBER
                and info.compress_type in (zipfile.ZIP_STORED, zipfile.ZIP_DEFLATED), 'baseline 선택 member 형식')
            with archive.open(info) as stream:
                data = stream.read(size + 1)  # 선택 member의 원문 CRC도 ZipExtFile이 확인한다.
            need(len(data) == size and sha(data) == pin, 'baseline 선택 member size/SHA')
            result[output] = data
    return result

def save(name, data):
    fd = os.open(name, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW | os.O_CLOEXEC,
        0o600, dir_fd=outfd)
    with os.fdopen(fd, 'wb') as stream:
        stream.write(data)
        stream.flush()
        os.fsync(stream.fileno())

try:
    need(sys.version_info[:3] == (3, 12, 15), '준비된 Python3.12.15 필요')
    proof = Path(os.environ['HISTORY_FETCH_PROOF'])
    need(proof.is_absolute() and proof.resolve(strict=True) == proof, '절대 canonical proof 경로 필요')
    outfd = os.open(proof, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC)
    owned = os.fstat(outfd)
    need(stat.S_ISDIR(owned.st_mode) and owned.st_uid == os.geteuid() and owned.st_mode & 0o077 == 0,
        'private owned proof directory 필요')
    space = os.fstatvfs(outfd)
    need(space.f_bavail * space.f_frsize >= 16 * 1024 * 1024, '출력 filesystem의 작업별16 MiB 증가량 수용 필요')
    outputs = set(MEMBERS) | set(BASE_MEMBERS) | {'history92-baseline.zip', 'history92-fetch.json'}
    for name in outputs:
        need(not os.path.lexists(proof / name), '기존 출력 덮어쓰기 금지')
    phase = '두 artifact metadata pin 확인'
    source_meta, baseline_meta = metadata(SOURCE), metadata(BASELINE)
    phase = '원본 source artifact exact Range/central/member 확인'
    source_url = location(SOURCE)
    source_directory(source_url)
    result = {name: source_member(source_url, row) for name, row in MEMBERS.items()}
    fixture = json.loads(result['history92-fixture.json'])
    need(fixture['version'] == 1 and fixture['source_commit'] == SOURCE_COMMIT
        and fixture['catalog_generation'] == PREFIX.split('/')[-2]
        and fixture['original_manifest_sha256'] == 'ed174bb4610ee55108b3f2502d7ed284cb66abb3cd153777ca546bf8fe0d3fca',
        'source fixture commit/generation/original pin 불일치')
    for output in ('history92.o', 'history92.ll'):
        row = MEMBERS[output]
        relative = row[0].removeprefix('driving-native-296/complete-fixture/')
        need(fixture['files'][relative] == {'bytes': row[3], 'sha256': row[5]}, 'fixture와 선택 object/IR 불일치')
    need(fixture['files']['models/driving_tinygrad.pkl'] == {'bytes': 72045323, 'sha256': PKL_SHA}, 'fixture 원본 PKL binding 불일치')
    phase = 'baseline 전체 ZIP pin 및 frozen BINARY 선택 추출'
    result.update(baseline_inputs(location(BASELINE)))
    binding = json.loads(result['history92-binding.json'])
    need(binding['pkl_sha256'] == PKL_SHA and binding['ir_sha256'] == MEMBERS['history92.ll'][5]
        and binding['code_sha256'] == FROZEN[1] and binding['code_bytes'] == FROZEN[0], 'baseline PKL/SOURCE/BINARY binding 불일치')
    context = json.loads(result['history92-context.json'])
    need(context['source_commit'] == 'defb9011df3fca924dca41a703b48ce5ff0c51e0'
        and context['workflow_commit'] == BASELINE['head'] and context['diagnostic_only'] is True
        and context['whole_original_pipeline_proved'] is False and context['retirement_acceptance'] is False,
        'baseline context pin/수용 상태 불일치')
    phase = '검증된 원문 저장'
    need((owned.st_dev, owned.st_ino) == (proof.stat().st_dev, proof.stat().st_ino), 'proof directory identity 변경')
    for name, data in result.items():
        save(name, data)
    receipt = {'schema': 'openpilot.model-history92-fetch.v1', 'status': 'VERIFIED_INERT_INPUTS_ONLY',
        'source_artifact': source_meta, 'baseline_artifact': baseline_meta,
        'source_commit': SOURCE_COMMIT, 'central_directory_sha256': CENTRAL[2],
        'source_archive_whole_hash_independently_verified': False,
        'baseline_archive_whole_hash_independently_verified': True,
        'source_selected_member_crc_sha_verified': True,
        'all_archive_members_crc_verified': False, 'transferred_bytes_including_gh_capture': transferred,
        'transfer_limit_bytes': LIMIT, 'range_requests': range_receipts,
        'outputs': {name: {'bytes': len(data), 'sha256': sha(data)} for name, data in result.items()},
        'limitation': 'source archive 전체 digest는 API metadata다. 당시 native exporter object이며 PKL 최초 생성 object라는 증명이나 reconstructed-image equality는 아니다.',
        'original_scripts_executed': False, 'pickle_loaded': False, 'compiler_invoked': False,
        'object_or_model_executed': False, 'reconstruction_performed': False,
        'historical_first_compiler_object_proved': False, 'same_image_as_frozen_verified': False,
        'model_or_historical_acceptance': False, 'frozen5_waived': False}
    save('history92-fetch.json', (json.dumps(receipt, ensure_ascii=False, indent=2) + '\n').encode())
    print('history92 fetch: 검증된 object/IR/frozen BINARY 원문과 receipt 저장; 모델 수용은 false')
except Exception as error:
    # urllib/gh 예외에는 signed URL/헤더가 포함될 수 있으므로 repr/traceback을 출력하지 않는다.
    reason = str(error) if isinstance(error, FetchFailure) else type(error).__name__
    print('history92 fetch 실패: ' + phase + ': ' + reason + ' (URL/인증 응답 비공개)', file=sys.stderr)
    sys.exit(1)
finally:
    if outfd is not None:
        os.close(outfd)
HISTORY_PY
