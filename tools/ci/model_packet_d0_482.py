#!/usr/bin/env python3
"""#482 D0 Actions 전용. packet/fixture 데이터만 decode하며 모델은 실행하지 않는다."""
from __future__ import annotations

import hashlib
import importlib.util
import io
import json
import lzma
import os
from pathlib import Path, PurePosixPath
import re
import resource
import signal
import stat
import subprocess
import sys
import tarfile
import time
import traceback
import urllib.error
import urllib.parse
import urllib.request
import zipfile

MIB, GIB = 1024**2, 1024**3
COMMON = 'tools/ci/controlsd_normal_523.py'
COMMON_SHA = '8a5da6caaf97dc00a0672e271130e0ce0ead8c055bee9883f7e503b9904b85a7'
ARTIFACT, RUN, PR_SOURCE = 12592585842, 40173790842, 'e0e55f32161f3613bfd93bd23a18816abb159b04'
PR_HEAD = 'bacda68cf229eae5f5f757774e943afeb287c0c8'
CLI_MEMBER = 'driving-four-source/retirement/checkout-model.cli.json'
CLI_SHA = 'f4047be0caa5a4d8924820309329ad3fc6066ebc471d6ce3ddb768bd666de967'
ZIP_BYTES, ZIP_SHA = 176747548, '22c0e16340e756c4ba438319e26455942b4d1deb64d6e47ada03166dcf47ee66'
MEMBER = 'driving-four-source/retirement/checkout-model/1344-dual/modelV2-output-3.bin'
PACKET_SHA = '5c9ad58bd983e76375378fae1e27dce6267a9529c25d5707d6413ad03704dfad'
FIXTURE_SHA = 'afe1c795d7f609801eb4cd5a5a7748c3c996929403a3c0fa102f232f2a8d248d'
ARCHIVE_SHA = '3ddfb1213cccbbf0d4d0ef7854b79cef5637acdfd7b59d883b13063d6968b4fe'
RAW_MEMBER = 'observations/1344-dual/outputs/raw-pred-0.bin'
RAW_SHA = '9afb2defff4ff29e5bf70e54db32d0d531d5e2b44507c8de652ee2c0dfcb67fe'
SELECTOR = 'model_482_d0::pinned_pr1344_model_v2_raw_prediction_full_bit_census'
FEATURES = ['driving-daemon-native']


def need(value, message):
    if not value:
        raise RuntimeError(message)


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def load_common(root):
    path = root / COMMON
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC)
    try:
        before = os.fstat(fd)
        raw = os.pread(fd, 128*1024+1, 0)
        fields = lambda m: (m.st_dev, m.st_ino, m.st_size, m.st_mtime_ns, m.st_ctime_ns)
        need(stat.S_ISREG(before.st_mode) and len(raw) == before.st_size <= 128*1024
             and sha(raw) == COMMON_SHA and fields(before) == fields(os.fstat(fd)) == fields(path.lstat()),
             'common source pin/identity')
    finally:
        os.close(fd)
    spec = importlib.util.spec_from_file_location('_packet482_common', path)
    module = importlib.util.module_from_spec(spec)
    exec(compile(raw, str(path), 'exec'), module.__dict__)
    return module


class XzStream(io.RawIOBase):
    """전체 archive를 메모리에 합치지 않는 bounded data-only XZ stream."""
    def __init__(self, fd, heartbeat):
        self.fd, self.heartbeat = fd, heartbeat
        self.decoder = lzma.LZMADecompressor(memlimit=335544320)
        self.offset = self.expanded = 0
        self.ended = False

    def readable(self):
        return True

    def readinto(self, buffer):
        self.heartbeat()
        if self.ended:
            return 0
        while True:
            chunk = b''
            if self.decoder.needs_input:
                chunk = os.pread(self.fd, 64*1024, self.offset)
                need(chunk, 'truncated XZ input')
                self.offset += len(chunk)
            raw = self.decoder.decompress(chunk, max_length=min(len(buffer), 64*1024))
            self.expanded += len(raw)
            need(self.expanded <= 512*MIB, 'expanded fixture archive bound')
            if self.decoder.eof:
                need(not self.decoder.unused_data and self.offset == 59663776, 'XZ trailing/concatenated data')
                self.ended = True
            if raw or self.ended:
                buffer[:len(raw)] = raw
                return len(raw)


class D0:
    def __init__(self):
        expected = {'GITHUB_ACTIONS': 'true', 'GITHUB_REPOSITORY': 'bin9208/openpilot-rust',
                    'GITHUB_EVENT_NAME': 'push', 'GITHUB_REF': 'refs/heads/codex/feat-523-python-rust',
                    'GITHUB_JOB': 'model-packet-d0', 'RUNNER_ENVIRONMENT': 'github-hosted',
                    'RUNNER_OS': 'Linux', 'RUNNER_ARCH': 'X64'}
        need(len(sys.argv) == 1 and all(os.environ.get(k) == v for k, v in expected.items())
             and os.uname().machine == 'x86_64', 'exact hosted D0 job required')
        self.source = os.environ['GITHUB_SHA']
        need(re.fullmatch('[0-9a-f]{40}', self.source), 'source40 required')
        self.root, self.job = Path(os.environ['DIAG_SOURCE']), Path(os.environ['CI_JOB_TEMP'])
        owner = ':'.join(os.environ[k] for k in ('GITHUB_RUN_ID', 'GITHUB_RUN_ATTEMPT', 'GITHUB_JOB'))
        need(re.fullmatch(r'/tmp/openpilot-ci\.[A-Za-z0-9]{6}', str(self.job)), 'owned job literal')
        need(self.root == Path.cwd() == Path(os.environ['GITHUB_WORKSPACE'])
             and self.root.parent == Path(os.environ['DIAG_SOURCE_OWNER'])
             and self.root.parent.parent == Path(os.environ['DIAG_ACTIONS_WORKSPACE']), 'private source nesting')
        for path in (self.job, self.root, self.root.parent, self.job/'proof'):
            m = path.lstat()
            need(path.resolve(strict=True) == path and stat.S_ISDIR(m.st_mode)
                 and m.st_uid == os.getuid() and not m.st_mode & 0o022, 'owned canonical directory')
        need(self.job.stat().st_mode & 0o077 == 0, 'private job0700')
        for path in (self.job/'owner', self.root.parent/'.ci296-owner'):
            need(not path.is_symlink() and path.is_file() and path.read_text().strip() == owner, 'owner marker')
        m = self.root.parent.stat()
        need(f'{m.st_dev}:{m.st_ino}' == os.environ['DIAG_SOURCE_IDENTITY'], 'source inode binding')
        self.c = load_common(self.root)
        self.out = self.job/'proof/model-packet-d0'
        self.out.mkdir(mode=0o700)
        self.input = self.job/'packet-d0-input'
        self.input.mkdir(mode=0o700)
        self.build = self.job/'build'
        self.build.mkdir(mode=0o700, exist_ok=True)
        self.target = self.build/'target'
        need(Path(os.environ['CARGO_TARGET_DIR']) == self.target, 'private target binding')
        self.sdk = Path(os.environ['CI_PREPARED_RUST_ROOT'])
        need(self.sdk == self.job/'rustup-home/toolchains/1.94.0-x86_64-unknown-linux-gnu'
             and self.sdk.resolve(strict=True) == self.sdk, 'actual private SDK binding')
        need(Path(os.environ['CI_PREPARED_RUST_MANIFEST']) == self.job/'proof/prepared-rust-files.before.sha256', 'SDK manifest binding')
        self.holds = self.c.Holds()
        self.children = self.c.Children(self.out/'children.jsonl')
        self.deadline = time.monotonic() + 1800
        self.commands, self.phase = [], 'initialization'
        self.last_budget = 0.0
        self.finalizing = False
        self.env = {k: os.environ[k] for k in ('HOME', 'CARGO_HOME', 'RUSTUP_HOME', 'CARGO_TARGET_DIR')}
        need(self.env['CARGO_HOME'] == str(self.job/'cargo-home')
             and self.env['RUSTUP_HOME'] == str(self.job/'rustup-home'), 'private tool homes')
        self.env.update(PATH=f'{self.sdk}/bin:/usr/bin:/bin', LANG='C.UTF-8', LC_ALL='C.UTF-8',
                        CARGO_BUILD_JOBS='2', CARGO_INCREMENTAL='0', CARGO_PROFILE_DEV_DEBUG='0',
                        CARGO_PROFILE_TEST_DEBUG='0', CARGO_TERM_COLOR='never',
                        RUSTC=str(self.sdk/'bin/rustc'), RUSTDOC=str(self.sdk/'bin/rustdoc'))
        self.source_before = self.sdk_before = None

    def budget(self, force=False):
        need(time.monotonic() <= self.deadline, f'D0 total1800s deadline: {self.phase}')
        if self.finalizing:
            return
        if not force and time.monotonic() - self.last_budget < 2:
            return
        self.last_budget = time.monotonic()
        for root in (self.root, self.job):
            v = os.statvfs(root)
            need(v.f_bavail * v.f_frsize >= 512*MIB, f'finish512MiB reserve: {root}')
        # mutable build를 snapshot으로 부르지 않는다. 경합/미확정은 성공으로 처리하지 않는다.
        scopes = [(self.build, 8*GIB), (self.job/'cargo-home', 8*GIB),
                  (self.job/'proof', 512*MIB), (self.input, 256*MIB), (self.root.parent, 2*GIB)]
        totals = []
        for root, limit in scopes:
            allocated = 0
            for path in self.c.walk(root):
                m = path.lstat()
                allocated += m.st_blocks*512
                need(allocated <= limit, f'allocated growth cap: {root}')
                if stat.S_ISREG(m.st_mode):
                    need(m.st_size <= 512*MIB, f'file512MiB cap: {path}')
                else:
                    need(stat.S_ISDIR(m.st_mode) or stat.S_ISLNK(m.st_mode), 'special output file')
            totals.append(allocated)
        need(totals[0]+totals[1] <= 8*GIB, 'combined build/registry8GiB cap')

    def command(self, argv, label, seconds=60, env=None):
        self.phase = label
        self.budget(True)
        need(not self.children.scan(), 'live child before command')
        argv = list(map(str, argv))
        start = time.monotonic()
        process, error = None, None
        with (self.out/f'{label}.stdout').open('xb') as stdout, (self.out/f'{label}.stderr').open('xb') as stderr:
            try:
                def limits():
                    resource.setrlimit(resource.RLIMIT_FSIZE, (512*MIB, 512*MIB))
                    resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
                process = subprocess.Popen(argv, cwd=self.root, env=env or self.env, start_new_session=True,
                                           stdin=subprocess.DEVNULL, stdout=stdout, stderr=stderr, preexec_fn=limits)
                self.children.process = process
                while True:
                    self.children.reap()
                    need(time.monotonic() <= min(start+seconds, self.deadline), f'command deadline: {label}')
                    need(stdout.tell() <= 32*MIB and stderr.tell() <= 32*MIB, 'command log32MiB cap')
                    self.budget()
                    if process.returncode is not None:
                        need(not self.children.scan(), 'live adopted/escaped child after command')
                        need(process.returncode == 0, f'{label}: exit {process.returncode}; preserve stderr')
                        break
                    time.sleep(0.05)
            except BaseException as problem:
                error = str(problem)
                raise
            finally:
                self.children.stop()
                self.commands.append(dict(label=label, argv=argv, returncode=process.returncode if process else None,
                                          error=error, elapsed_seconds=time.monotonic()-start,
                                          all_owned_children_reaped=not self.children.scan()))
                self.children.process = None
        self.budget(True)
        return self.c.read(self.out/f'{label}.stdout', 32*MIB)

    def source_snapshot(self, label):
        need(self.command(['/usr/bin/git', 'rev-parse', 'HEAD'], f'head-{label}').decode().strip() == self.source, 'HEAD differs')
        self.command(['/usr/bin/git', 'diff', '--exit-code', 'HEAD', '--'], f'diff-{label}')
        index = self.command(['/usr/bin/git', 'ls-files', '--stage', '-z'], f'index-{label}')
        paths = []
        for entry in index.split(b'\0'):
            if not entry:
                continue
            meta, name = entry.split(b'\t', 1)
            mode, _, stage = meta.decode().split()
            relative = PurePosixPath(os.fsdecode(name))
            need(stage == '0' and mode != '160000' and not relative.is_absolute() and '..' not in relative.parts,
                 'unsafe/unmerged index')
            paths.append(self.root/str(relative))
        value = {'head': self.source, 'index_sha256': sha(index), 'files': self.c.snapshot(self.root, paths)}
        self.c.save(self.out/f'source-{label}.json', value)
        return value

    def sdk_snapshot(self, label):
        value = self.c.snapshot(self.sdk, self.c.walk(self.sdk), sdk=True)
        self.c.save(self.out/f'sdk-{label}.json', value)
        return value

    def packet(self):
        self.phase = 'pinned-pr-packet-download'
        token = os.environ.pop('GH_TOKEN', '')
        need(token and not any(k in self.env for k in ('GH_TOKEN', 'GITHUB_TOKEN')), 'download-only token required')
        api = f'https://api.github.com/repos/bin9208/openpilot-rust/actions/artifacts/{ARTIFACT}'
        request = urllib.request.Request(api, headers={'Authorization': 'Bearer '+token,
            'Accept': 'application/vnd.github+json', 'X-GitHub-Api-Version': '2022-11-28'})
        with urllib.request.urlopen(request, timeout=30) as response:
            raw = response.read(MIB+1)
        need(len(raw) <= MIB, 'artifact metadata1MiB bound')
        metadata = self.c.unique_json(raw)
        need(metadata['id'] == ARTIFACT and metadata['size_in_bytes'] == ZIP_BYTES and metadata['expired'] is False
             and metadata['digest'] == 'sha256:'+ZIP_SHA and metadata['workflow_run']['id'] == RUN
             and metadata['workflow_run']['head_sha'] == PR_HEAD, 'fixed PR artifact API identity')
        self.c.write_new(self.out/'pr-artifact-api.json', raw)
        job_request = urllib.request.Request(
            'https://api.github.com/repos/bin9208/openpilot-rust/actions/jobs/121111148638',
            headers=dict(request.header_items()))
        with urllib.request.urlopen(job_request, timeout=30) as response:
            job_raw = response.read(MIB+1)
        need(len(job_raw) <= MIB, 'job metadata1MiB bound')
        job = self.c.unique_json(job_raw)
        need(job['id'] == 121111148638 and job['run_id'] == RUN and job['head_sha'] == PR_HEAD
             and job['name'] == 'rust original model pipelines' and job['status'] == 'completed'
             and job['conclusion'] == 'failure', 'exact failed PR job identity')
        self.c.write_new(self.out/'pr-job-api.json', job_raw)
        # redirect URL에는 원래 API credential을 전달하지 않는다.
        class NoRedirect(urllib.request.HTTPRedirectHandler):
            def redirect_request(self, _req, _fp, _code, _msg, _headers, _url):
                return None
        opener = urllib.request.build_opener(NoRedirect())
        request = urllib.request.Request(api+'/zip', headers=dict(request.header_items()))
        try:
            opener.open(request, timeout=30)
            raise RuntimeError('artifact archive must return signed redirect')
        except urllib.error.HTTPError as redirect:
            need(redirect.code == 302, 'artifact archive302 required')
            location = redirect.headers['Location']
            redirect.close()
        need(location.startswith('https://') and '@' not in urllib.parse.urlsplit(location).netloc, 'HTTPS artifact redirect')
        archive = self.input/'pr.zip'
        h, count = hashlib.sha256(), 0
        download_deadline = time.monotonic()+180
        with urllib.request.urlopen(location, timeout=20) as response, archive.open('xb') as output:
            while chunk := response.read(256*1024):
                count += len(chunk)
                need(count <= ZIP_BYTES <= 180*MIB and time.monotonic() <= download_deadline, 'bounded artifact download')
                self.budget()
                h.update(chunk)
                output.write(chunk)
        need(count == ZIP_BYTES and h.hexdigest() == ZIP_SHA, 'complete PR ZIP size/SHA')
        self.holds.add(archive, ZIP_SHA)
        with zipfile.ZipFile(archive) as z:
            rows = z.infolist()
            need(len(rows) <= 1500 and len({r.filename for r in rows}) == len(rows)
                 and sum(r.file_size for r in rows) <= 512*MIB, 'PR ZIP roster/expanded bound')
            found = [r for r in rows if r.filename == MEMBER]
            need(len(found) == 1 and found[0].file_size == 19152 and not found[0].flag_bits & 1
                 and not stat.S_ISLNK(found[0].external_attr >> 16), 'exact packet member')
            with z.open(found[0]) as stream:
                packet = stream.read(19153)
            need(len(packet) == 19152 and sha(packet) == PACKET_SHA, 'selected packet CRC/size/SHA')
            cli_rows = [r for r in rows if r.filename == CLI_MEMBER]
            need(len(cli_rows) == 1 and cli_rows[0].file_size == 2509 and not cli_rows[0].flag_bits & 1
                 and not stat.S_ISLNK(cli_rows[0].external_attr >> 16), 'exact PR CLI provenance member')
            with z.open(cli_rows[0]) as stream:
                cli_raw = stream.read(2510)
            need(len(cli_raw) == 2509 and sha(cli_raw) == CLI_SHA, 'PR CLI provenance CRC/size/SHA')
            cli = self.c.unique_json(cli_raw)
            need(cli['source'] == PR_SOURCE and cli['status'] == 1
                 and cli['binary_sha256_before'] == cli['binary_sha256_after']
                 == 'ed1e4c87ac6cbb9ec3cf26df61acbce7c497d9cba5f3c388135d12350b095b09',
                 'actual merge source/failed same ELF binding')
            self.c.write_new(self.out/'pr-checkout-model.cli.json', cli_raw)
        self.c.write_new(self.input/'packet.bin', packet)
        self.holds.add(self.input/'packet.bin', PACKET_SHA)
        self.c.copy_new(self.input/'packet.bin', self.out/'packet.bin', 19152)
        self.c.save(self.out/'packet-provenance.json', {'artifact_id': ARTIFACT, 'run': RUN, 'merge_source': PR_SOURCE,
            'pr_head': PR_HEAD, 'pr_job': 121111148638, 'cli_member_sha256': CLI_SHA, 'zip_bytes': count, 'zip_sha256': ZIP_SHA,
            'member': MEMBER, 'member_bytes': len(packet), 'member_sha256': PACKET_SHA,
            'selected_member_crc_verified': True, 'whole_zip_crc_verified': False})

    def fixture(self):
        self.phase = 'frozen-raw-data-extraction'
        base = self.root/'rust/crates/dev-tools/tests/fixtures/driving-daemon-original'
        archive = base/'observations.tar.xz'
        held = self.holds.add(archive, ARCHIVE_SHA)
        need(archive.stat().st_size == 59663776, 'fixture archive size')
        self.holds.add(base/'fixture.json', FIXTURE_SHA)
        stream = XzStream(self.holds.files[held]['fd'], self.budget)
        selected, names = {}, set()
        with tarfile.open(fileobj=io.BufferedReader(stream, 64*1024), mode='r|') as tar:
            for member in tar:
                name = PurePosixPath(member.name)
                need(str(name) == member.name and not name.is_absolute() and '..' not in name.parts
                     and member.isfile() and member.size <= 512*MIB
                     and member.name not in names and len(names) < 2000, 'fixture regular safe unique roster')
                names.add(member.name)
                if member.name in ('fixture.json', RAW_MEMBER):
                    size = 310291 if member.name == 'fixture.json' else 10304
                    need(member.size == size, 'selected fixture member size')
                    source = tar.extractfile(member)
                    need(source is not None, 'fixture member data missing')
                    raw = source.read(size+1)
                    need(len(raw) == size, 'selected fixture member truncated')
                    selected[member.name] = raw
                self.budget()
            # tar 종료 뒤 XZ check와 trailing-byte 검증까지 끝낸다. padding은0만 허용한다.
            while tail := tar.fileobj.read(64*1024):
                need(not tail.strip(b'\0'), 'nonzero data after tar end')
        need(stream.ended and len(names) == 1564 and set(selected) == {'fixture.json', RAW_MEMBER}, 'full archive data traversal')
        need(sha(selected['fixture.json']) == FIXTURE_SHA and sha(selected[RAW_MEMBER]) == RAW_SHA, 'fixed manifest/raw SHA')
        manifest = self.c.unique_json(selected['fixture.json'])
        need(manifest['source_commit'] == 'e25a14386582858dae25c6daca5ced3bfb4bacab'
             and manifest['files'][RAW_MEMBER] == {'bytes': 10304, 'sha256': RAW_SHA}, 'independent fixture raw ledger')
        self.c.write_new(self.input/'frozen-raw.bin', selected[RAW_MEMBER])
        self.holds.add(self.input/'frozen-raw.bin', RAW_SHA)
        self.c.copy_new(self.input/'frozen-raw.bin', self.out/'frozen-raw.bin', 10304)
        self.c.save(self.out/'fixture-provenance.json', {'archive_sha256': ARCHIVE_SHA, 'archive_bytes': 59663776,
            'fixture_manifest_sha256': FIXTURE_SHA, 'fixture_source': manifest['source_commit'],
            'member': RAW_MEMBER, 'member_sha256': RAW_SHA, 'member_bytes': 10304,
            'tar_regular_members': len(names), 'expanded_bytes': stream.expanded,
            'historical_provider_proven': False, 'archive_code_executed': False})

    def select(self, raw):
        rows = [self.c.unique_json(line) for line in raw.splitlines()]
        need(rows and all(isinstance(r, dict) and r.get('reason') in ('compiler-artifact', 'compiler-message',
             'build-script-executed', 'build-finished') for r in rows), 'complete Cargo JSONL')
        need(rows[-1] == {'reason': 'build-finished', 'success': True}
             and sum(r['reason'] == 'build-finished' for r in rows) == 1, 'Cargo successful unique terminator')
        selected, seen = [], set()
        for row in rows:
            if row['reason'] != 'compiler-artifact':
                continue
            key = json.dumps(row, sort_keys=True)
            need(key not in seen, 'duplicate compiler artifact')
            seen.add(key)
            t = row['target']
            if t['name'] != 'driving_daemon_retirement' or t['kind'] != ['test']:
                continue
            manifest = self.root/'rust/crates/dev-tools/Cargo.toml'
            need(row['manifest_path'] == str(manifest) and row['package_id'] == f'path+file://{manifest.parent}#openpilot-dev-tools@0.1.0'
                 and t['src_path'] == str(manifest.parent/'tests/driving_daemon_retirement.rs')
                 and row['profile']['test'] is True and row['fresh'] is False and row['features'] == FEATURES,
                 'current exact test manifest/package/path/profile/features/fresh differs')
            need(isinstance(row['executable'], str) and row['executable'], 'non-null executable')
            path = Path(row['executable'])
            need(path.is_absolute() and path.resolve(strict=True) == path and path.is_relative_to(self.target), 'private current ELF path')
            m = path.lstat()
            need(stat.S_ISREG(m.st_mode) and m.st_uid == os.getuid() and m.st_mode & 0o111, 'owned current test ELF')
            self.holds.add(path, elf=True)
            selected.append((path, row))
        need(len(selected) == 1, 'missing/duplicate exact current test ELF')
        self.c.save(self.out/'selected-current-elf.json', selected[0][1])
        return selected[0][0]

    def run(self):
        for tool in ('git', 'python3', 'ldd', 'capnp', 'cc', 'c++', 'ld', 'ar'):
            self.holds.add(Path('/usr/bin')/tool)
        for name in ('cargo', 'rustc', 'rustdoc'):
            self.holds.add(self.sdk/'bin'/name, elf=True)
        self.holds.add(self.root/COMMON, COMMON_SHA)
        self.holds.add(Path(__file__))
        self.source_before = self.source_snapshot('before')
        self.sdk_before = self.sdk_snapshot('before')
        self.packet()
        self.fixture()
        raw = self.command([self.sdk/'bin/cargo', 'test', '--manifest-path', self.root/'rust/Cargo.toml',
            '--locked', '-p', 'openpilot-dev-tools', '--no-default-features', '--features', 'driving-daemon-native',
            '--test', 'driving_daemon_retirement', '--no-run', '--message-format=json'], 'current-build', 1200)
        binary = self.select(raw)
        providers = self.command(['/usr/bin/ldd', binary], 'test-dso', 30).decode()
        dso = set()
        for line in providers.splitlines():
            match = re.fullmatch(r'\s*(?:\S+\s+=>\s+)?(/\S+)\s+\(0x[0-9a-fA-F]+\)\s*', line)
            if match:
                path = Path(match[1]).resolve(strict=True)
                need(path.is_relative_to('/usr/lib') or path.is_relative_to('/lib') or path.is_relative_to(self.sdk), 'test DSO scope')
                dso.add(str(self.holds.add(Path(match[1]), elf=True)))
            else:
                need(re.fullmatch(r'\s*linux-vdso\.so\.\d+\s+\(0x[0-9a-fA-F]+\)\s*', line), 'unresolved DSO output')
        need(dso, 'test DSO closure missing')
        self.c.save(self.out/'test-declared-dso.json', {'paths': sorted(dso), 'scope': 'current test ldd declared closure',
            'actual_runtime_mapping_observed': False, 'whole_process_provider_closure': False})
        self.c.save(self.out/'held-before.json', self.holds.records())
        test_env = dict(self.env, GITHUB_ACTIONS='true', GITHUB_REPOSITORY='bin9208/openpilot-rust',
                        EXPECTED_SOURCE=self.source, MODEL482_D0_INPUT=str(self.input),
                        MODEL482_D0_REPORT=str(self.out/'comparison.json'))
        text = self.command([binary, SELECTOR, '--ignored', '--exact', '--nocapture', '--test-threads=1'], 'decode-d0', 60, test_env).decode()
        need(f'test {SELECTOR} ... ' in text and 'MODEL482_D0_COMPLETE words=2576 different_words=' in text
             and 'test result: ok. 1 passed; 0 failed; 0 ignored;' in text, 'one exact D0 test/marker/summary')
        report = self.c.unique_json(self.c.read(self.out/'comparison.json', 4*MIB))
        c = report['comparison']
        need(report['diagnostic_complete'] is True and report['current_decoder_source'] == self.source
             and c['words'] == 2576 and c['equal_words']+c['different_words'] == 2576
             and len(c['differences']) == c['different_words'] and c['all_differences_included'] is True,
             'complete full raw census report')
        for flag in ('original_executed', 'model_executed', 'native_daemon_executed', 'historical_provider_proven',
                     'first_producer_observed', 'full128_acceptance', 'historical_half_snan5_resolved', 'model_or_historical_acceptance'):
            need(report[flag] is False, 'D0 scope/acceptance flags')
        need(report['retirement_credit'] == 0, 'D0 credit must remain0')

    def finish(self, error):
        attempted_phase = self.phase
        self.finalizing = True
        self.deadline = time.monotonic()+120
        failures = []
        # 한 봉인 실패가 다른 봉인이나 회수 시도를 생략하지 않는다.
        for label, action in [('children', self.children.stop),
            ('source', lambda: need(self.source_before is not None and self.source_snapshot('after') == self.source_before, 'source before/after differs')),
            ('sdk', lambda: need(self.sdk_before is not None and self.sdk_snapshot('after') == self.sdk_before, 'SDK before/after differs')),
            ('held', self.holds.verify)]:
            try:
                action()
            except BaseException as problem:
                failures.append(f'{label}: {problem}')
        live = self.children.scan()
        try:
            proof_bytes = sum(path.lstat().st_blocks*512 for path in self.c.walk(self.job/'proof'))
            need(proof_bytes <= 512*MIB, 'final proof512MiB cap')
            for path in (self.out/'packet.bin', self.out/'frozen-raw.bin'):
                if path.exists():
                    original = self.input/path.name
                    need(self.c.regular(path)['sha256'] == self.c.regular(original)['sha256'], 'uploaded data copy differs')
        except BaseException as problem:
            failures.append(f'proof: {problem}')
        self.c.save(self.out/'held-after.json', self.holds.records())
        self.c.save(self.out/'commands.json', self.commands)
        self.c.save(self.out/'children.json', {'subreaper': True, 'live': list(live), 'all_owned_children_reaped': not live})
        self.c.save(self.out/'final-seals.json', {'pass': not failures and not live, 'failures': failures,
            'source': self.source, 'all_owned_children_reaped': not live})
        self.c.save(self.out/'outcome.json', {'phase': attempted_phase, 'error': error, 'finalizer_failures': failures,
            'diagnostic_complete': error is None and not failures and not live,
            'model_or_historical_acceptance': False, 'retirement_credit': 0})
        self.holds.close()
        os.close(self.children.journal)
        return error is None and not failures and not live


def interrupted(number, _frame):
    raise InterruptedError(f'D0 interrupted by signal {number}')


def main():
    signal.signal(signal.SIGTERM, interrupted)
    signal.signal(signal.SIGINT, interrupted)
    probe, error, passed = None, None, False
    try:
        probe = D0()
        probe.run()
    except BaseException:
        error = traceback.format_exc()
        print(error, file=sys.stderr)
    finally:
        if probe is not None:
            try:
                passed = probe.finish(error)
            except BaseException:
                print(traceback.format_exc(), file=sys.stderr)
    return 0 if passed else 1


if __name__ == '__main__':
    sys.exit(main())
