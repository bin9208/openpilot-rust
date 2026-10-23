"""One-off Actions observation of authenticated source109; never acceptance."""

import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import platform
import re
import resource
import selectors
import shutil
import signal
import stat
import subprocess
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
import zipfile


SOURCE = '10919538d31b04d28d9a23b383d925441434d579'
ARTIFACT = 12485436178
ZIP_BYTES = 10023988
ZIP_SHA = '965038890303673a38af53376f8b315fdc7f96df20cc5bf1e85d9676d1f21905'
BINARY_SHA = '8f019be68c4d19311344d9db016b1ed36846905145345b33eda865f3a8a9e7f0'
INPUT_SHA = '20af4658f3f42d016780e7f2ba4bc7ea8afc1ac1802cddbb464c203eee75c11b'
MEMBERS = {
    'binary': ('oracles/safety-src/pandad-protocol-source', 8 * 1024**2, BINARY_SHA),
    'original.input.jsonl': ('oracles/safety/source.input.jsonl', 4 * 1024**2, INPUT_SHA),
    'manifest.json': ('oracles/safety-src/manifest.json', 32 * 1024, None),
}
STDOUT_LIMIT, STDERR_LIMIT = 16 * 1024**2, 4 * 1024**2
# ZIP16 + selected13 + relocated8 + captures40 + samples/receipts8 + bookkeeping.
EVIDENCE_GROWTH = 160 * 1024**2
DEADLINE = 90
MARKERS = {'diagnostic_only': True, 'source_native_acceptance': False}


def sha(data):
    return hashlib.sha256(data).hexdigest()


def save(path, value):
    with path.open('x') as stream:
        json.dump(value, stream, indent=2)
        stream.write('\n')


def require(condition, reason):
    if not condition:
        raise RuntimeError(reason)


def owned_root():
    require(os.environ.get('GITHUB_ACTIONS') == 'true'
            and os.environ.get('RUNNER_NAME') == 'bin-actions'
            and os.environ.get('GITHUB_REPOSITORY') == 'bin9208/openpilot-rust'
            and platform.machine() == 'x86_64' and os.geteuid() != 0,
            'Expected the registered unprivileged x64 Actions runner')
    root = Path(os.environ['CI_JOB_TEMP'])
    require(root.parent == Path('/tmp') and root.name.startswith('openpilot-ci.')
            and not root.is_symlink() and root.is_dir()
            and root.stat().st_uid == os.getuid(), 'Unexpected job-temp ownership')
    owner = root / 'owner'
    expected = ':'.join(os.environ[key] for key in ('GITHUB_RUN_ID', 'GITHUB_RUN_ATTEMPT', 'GITHUB_JOB'))
    require(not owner.is_symlink() and owner.is_file() and owner.stat().st_size < 256
            and owner.read_text() == expected, 'Job-temp marker does not match this job')
    require(root.stat().st_dev == Path(os.environ['GITHUB_WORKSPACE']).stat().st_dev,
            'Params must remain on the workspace filesystem')
    return root


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, request, fp, code, message, headers, target):
        return None


def api(suffix):
    token = os.environ.get('GH_TOKEN')
    require(bool(token), 'Missing actions:read token')
    request = urllib.request.Request(
        f'https://api.github.com/repos/bin9208/openpilot-rust/actions/artifacts/{ARTIFACT}{suffix}',
        headers={'Authorization': 'Bearer ' + token, 'Accept': 'application/vnd.github+json',
                 'X-GitHub-Api-Version': '2022-11-28', 'User-Agent': 'panda-safety-518'})
    return urllib.request.build_opener(NoRedirect).open(request, timeout=20)


def download(output):
    # Do not retain/print exceptions containing a signed storage URL.
    try:
        with api('') as response:
            metadata = json.loads(response.read(65537))
        require(metadata['id'] == ARTIFACT and metadata['size_in_bytes'] == ZIP_BYTES
                and metadata['digest'] == 'sha256:' + ZIP_SHA and not metadata['expired']
                and metadata['name'] == 'rust-panda-runtime-evidence'
                and metadata['workflow_run']['head_sha'] == SOURCE
                and metadata['workflow_run']['id'] == 39893953434,
                'Artifact API provenance mismatch')
        try:
            with api('/zip'):
                raise RuntimeError('Expected an explicit artifact storage redirect')
        except urllib.error.HTTPError as redirect:
            require(redirect.code == 302, 'Artifact API did not return a storage redirect')
            location = redirect.headers.get('Location', '')
        target = urllib.parse.urlsplit(location)
        require(target.scheme == 'https' and target.hostname and not target.username
                and not target.password and target.port in (None, 443), 'Invalid storage redirect')
        request = urllib.request.Request(location, headers={'User-Agent': 'panda-safety-518'})
        began, total, digest = time.monotonic(), 0, hashlib.sha256()
        with urllib.request.build_opener(NoRedirect).open(request, timeout=20) as response:
            with (output / 'artifact.zip').open('xb') as stream:
                while chunk := response.read(64 * 1024):
                    total += len(chunk)
                    require(total <= ZIP_BYTES and time.monotonic() - began <= 90,
                            'Artifact copy exceeded its byte/time allowance')
                    digest.update(chunk)
                    stream.write(chunk)
        require(total == ZIP_BYTES and digest.hexdigest() == ZIP_SHA, 'Artifact ZIP digest/size mismatch')
    except (urllib.error.URLError, ValueError, KeyError, OSError):
        raise RuntimeError('Artifact transport/provenance failed; URL and credentials withheld') from None
    return {'id': ARTIFACT, 'source_commit': SOURCE, 'bytes': total, 'sha256': digest.hexdigest()}


def rows_from(data):
    rows = [json.loads(line) for line in data.splitlines()]
    require(len(rows) == 897 and sum(len(row['operations']) for row in rows) == 4499,
            'Original safety workload count changed')
    require(len({row['label'] for row in rows}) == 897
            and all(row['op'] == 'safety' for row in rows), 'Original case identity changed')
    return rows


def canonical(rows):
    return sha(json.dumps([{key: value for key, value in row.items() if key != 'params_root'}
                           for row in rows], sort_keys=True, separators=(',', ':')).encode())


def prepare(output, report):
    free = shutil.disk_usage(output).free
    report.update(free_before_bytes=free, estimated_copy_and_evidence_growth_bytes=EVIDENCE_GROWTH)
    require(free >= EVIDENCE_GROWTH, 'Bounded artifact/evidence growth does not fit')
    report['artifact'] = download(output)
    with zipfile.ZipFile(output / 'artifact.zip') as archive:
        infos = archive.infolist()
        require(len(infos) <= 512 and sum(info.file_size for info in infos) <= 128 * 1024**2,
                'Archive member count/expanded-size bound exceeded')
        names = set()
        for info in infos:
            path = PurePosixPath(info.filename)
            require(info.filename not in names and not path.is_absolute() and '..' not in path.parts
                    and not info.flag_bits & 1
                    and stat.S_IFMT(info.external_attr >> 16) in (0, stat.S_IFREG, stat.S_IFDIR),
                    'Unsafe or duplicate ZIP member')
            names.add(info.filename)
        for destination, (name, limit, expected) in MEMBERS.items():
            require(archive.getinfo(name).file_size <= limit, 'Selected ZIP member exceeded its bound')
            data = archive.read(name)
            require(expected is None or sha(data) == expected, 'Selected member SHA-256 mismatch')
            with (output / destination).open('xb') as stream:
                stream.write(data)
    manifest = json.loads((output / 'manifest.json').read_text())
    require(manifest['binary_sha256'] == BINARY_SHA and manifest['sanitize'] is True,
            'Original sanitized build manifest mismatch')
    report['input_canonical_sha256'] = canonical(rows_from((output / 'original.input.jsonl').read_bytes()))
    binary = output / 'binary'
    header = binary.read_bytes()[:20]
    require(header[:6] == b'\x7fELF\x02\x01' and header[18:20] == b'\x3e\x00',
            'Expected the authenticated x64 ELF executable')
    binary.chmod(0o700)
    # ldd runs only in Actions, after authentication. It checks relocations and
    # symbol versions; resolved dependency hashes record this execution's ABI.
    environment = {key: value for key, value in os.environ.items() if key not in ('GH_TOKEN', 'GITHUB_TOKEN')}
    resolved = subprocess.run(['/usr/bin/ldd', '-r', str(binary)], capture_output=True,
                              timeout=20, env=environment)
    (output / 'dependencies.stdout').write_bytes(resolved.stdout)
    (output / 'dependencies.stderr').write_bytes(resolved.stderr)
    text = (resolved.stdout + resolved.stderr).decode(errors='replace')
    require(resolved.returncode == 0 and 'not found' not in text and 'undefined symbol' not in text,
            'Authenticated binary cannot be reused: dependency resolution failed; safety-only rebuild needed')
    dependencies = sorted(set(re.findall(r'(?:=>\s*)?(/[^\s()]+)', text)))
    require(dependencies, 'No resolved ELF dependency identities')
    report['dependencies'] = [{'path': name, 'sha256': sha(Path(name).read_bytes())} for name in dependencies]
    report['sanitizer_environment'] = {key: os.environ.get(key) for key in
                                     ('ASAN_OPTIONS', 'LSAN_OPTIONS', 'UBSAN_OPTIONS', 'LD_LIBRARY_PATH', 'LD_PRELOAD')}
    report['prepared'] = True


def filesystem(root):
    entries = []
    for line in Path('/proc/self/mountinfo').read_text().splitlines():
        fields = line.split()
        target = Path(re.sub(r'\\([0-7]{3})', lambda match: chr(int(match[1], 8)), fields[4]))
        if root == target or target in root.parents:
            entries.append((len(target.parts), fields[fields.index('-') + 1], str(target), fields[5]))
    require(entries, 'No Params mount identity')
    _, kind, target, options = max(entries)
    require(kind not in ('tmpfs', 'ramfs'), 'Durable Params observation cannot use memory filesystem')
    return {'type': kind, 'mount': target, 'options': options, 'device': root.stat().st_dev}


def sample_child(group, binary):
    # Follow only this new session's descendants; never scan unrelated processes.
    pending, samples = [group], []
    identity = binary.stat()

    def read_at(directory, name):
        descriptor = os.open(name, os.O_RDONLY | os.O_CLOEXEC | os.O_NOFOLLOW, dir_fd=directory)
        try:
            return os.read(descriptor, 4096).decode(errors='replace')
        finally:
            os.close(descriptor)

    for _ in range(8):
        if not pending:
            break
        pid = pending.pop(0)
        directory = None
        try:
            # Anchor reads to this proc inode, so PID reuse cannot redirect a
            # later pathname read into a different process's metadata.
            directory = os.open(f'/proc/{pid}', os.O_RDONLY | os.O_DIRECTORY | os.O_CLOEXEC | os.O_NOFOLLOW)
            fields = read_at(directory, 'stat').rsplit(') ', 1)[1].split()
            if int(fields[2]) != group:
                continue
            pending.extend(int(value) for value in read_at(directory, f'task/{pid}/children').split())
            executable = os.stat('exe', dir_fd=directory)
            if (executable.st_dev, executable.st_ino) != (identity.st_dev, identity.st_ino):
                continue
            item = {'pid': pid, 'start_ticks': fields[19], 'state': fields[0],
                    'user_ticks': int(fields[11]), 'system_ticks': int(fields[12])}
            for name in ('wchan', 'io', 'syscall'):
                try:
                    value = read_at(directory, name)
                    item[name] = value.split()[0] if name == 'syscall' else value
                except (OSError, IndexError) as error:
                    item[name + '_unavailable'] = type(error).__name__
            samples.append(item)
        except (OSError, ValueError, IndexError):
            continue
        finally:
            if directory is not None:
                os.close(directory)
    return samples


def run_observation(output, label, command, payload, report):
    selector = selectors.DefaultSelector()
    files = {name: (output / f'{label}.{name}').open('xb') for name in ('stdout', 'stderr')}
    totals, truncated = {'stdout': 0, 'stderr': 0}, False
    usage_before = resource.getrusage(resource.RUSAGE_CHILDREN)
    environment = {key: value for key, value in os.environ.items() if key not in ('GH_TOKEN', 'GITHUB_TOKEN')}
    began, next_sample, sent = time.monotonic(), 0, 0
    process, cleanup_error = None, None
    samples = report['proc_samples'] = []
    try:
        process = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                   stderr=subprocess.PIPE, env=environment, start_new_session=True)
        report['wrapper_pid'] = process.pid
        for channel in ('stdin', 'stdout', 'stderr'):
            stream = getattr(process, channel)
            os.set_blocking(stream.fileno(), False)
            selector.register(stream, selectors.EVENT_WRITE if channel == 'stdin' else selectors.EVENT_READ, channel)
        while selector.get_map():
            elapsed = time.monotonic() - began
            require(elapsed < DEADLINE + 5, 'Outer observation watchdog expired')
            if elapsed >= next_sample and len(samples) < 192:
                samples.append({'seconds': elapsed, 'source': sample_child(process.pid, output / 'binary')})
                next_sample = elapsed + 0.5
            for key, _ in selector.select(timeout=min(0.5, DEADLINE + 5 - elapsed)):
                stream, channel = key.fileobj, key.data
                if channel == 'stdin':
                    try:
                        sent += os.write(stream.fileno(), payload[sent:sent + 65536])
                    except BlockingIOError:
                        continue
                    except BrokenPipeError:
                        selector.unregister(stream)
                        stream.close()
                        continue
                    if sent == len(payload):
                        selector.unregister(stream)
                        stream.close()
                else:
                    try:
                        chunk = os.read(stream.fileno(), 65536)
                    except BlockingIOError:
                        continue
                    if not chunk:
                        selector.unregister(stream)
                        stream.close()
                        continue
                    limit = STDOUT_LIMIT if channel == 'stdout' else STDERR_LIMIT
                    remaining = max(0, limit - totals[channel])
                    files[channel].write(chunk[:remaining])
                    totals[channel] += len(chunk)
                    truncated |= len(chunk) > remaining
        report['returncode'] = process.wait(timeout=5)
        report['deadline_reached'] = time.monotonic() - began >= DEADLINE
        report['outcome'] = 'timeout' if process.returncode in (124, 137) and report['deadline_reached'] else (
            'completed' if process.returncode == 0 else 'source_or_tracer_failure')
    except Exception as error:
        report['capture_error'] = type(error).__name__ + ': ' + str(error)
        raise
    finally:
        # No Popen context manager and no unbounded wait, including exceptions.
        if process is not None:
            if process.returncode is None:
                try:
                    os.killpg(process.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
            try:
                process.wait(timeout=5)
                report['wrapper_reaped'] = True
            except subprocess.TimeoutExpired:
                report['cleanup_incomplete_owned_pid'] = process.pid
                cleanup_error = 'Owned observation process did not reap within cleanup budget'
            finally:
                for stream in (process.stdin, process.stdout, process.stderr):
                    if not stream.closed:
                        stream.close()
            # A reaped timeout wrapper is insufficient evidence that a killed,
            # uninterruptible descendant has gone. Observe; do not signal a
            # potentially reused group after reaping its original leader.
            try:
                os.killpg(process.pid, 0)
                report['cleanup_incomplete_owned_group'] = process.pid
                cleanup_error = 'Observation group remains after bounded cleanup'
            except ProcessLookupError:
                report['owned_group_gone'] = True
        selector.close()
        for stream in files.values():
            stream.close()
        usage = resource.getrusage(resource.RUSAGE_CHILDREN)
        report.update(elapsed_seconds=time.monotonic() - began, input_bytes_sent=sent,
                      output_bytes=totals, output_truncated=truncated,
                      child_tree_user_seconds=usage.ru_utime - usage_before.ru_utime,
                      child_tree_system_seconds=usage.ru_stime - usage_before.ru_stime,
                      child_tree_voluntary_switches=usage.ru_nvcsw - usage_before.ru_nvcsw,
                      child_tree_block_output=usage.ru_oublock - usage_before.ru_oublock)
        if cleanup_error:
            raise RuntimeError(cleanup_error)
    require(not truncated, 'Observation output bound exceeded; evidence is incomplete')


def observe(output, label, report):
    preparation = json.loads((output / 'prepare.json').read_text())
    require(preparation.get('prepared') is True, 'Authenticated preparation is incomplete')
    require(sha((output / 'binary').read_bytes()) == BINARY_SHA, 'Reference binary changed')
    original = (output / 'original.input.jsonl').read_bytes()
    require(sha(original) == INPUT_SHA, 'Original workload changed')
    rows = rows_from(original)
    params = output / (label + '-params')
    params.mkdir(mode=0o700)
    report['filesystem'] = filesystem(params)
    relocated = [row | {'params_root': str(params)} for row in rows]
    require(canonical(relocated) == canonical(rows), 'Non-root input fields changed')
    payload = ''.join(json.dumps(row) + '\n' for row in relocated).encode()
    (output / f'{label}.input.jsonl').write_bytes(payload)
    report.update(original_input_sha256=INPUT_SHA, relocated_input_sha256=sha(payload),
                  root_independent_input_sha256=canonical(rows), rows=len(rows), operations=4499,
                  params_root=str(params), binary_sha256=BINARY_SHA, deadline_seconds=DEADLINE,
                  instrumented=label == 'profile', untraced=label == 'normal',
                  timing_scope='wrapper/source wall time under bounded evidence capture')
    report['input_normalization'] = {'only_changed_field': 'params_root',
                                     'original_roots': sorted({row['params_root'] for row in rows}),
                                     'all_other_fields_equal': True}
    command = ['/usr/bin/timeout', '--signal=TERM', '--kill-after=2s', '90s']
    if label == 'profile':
        tracer = shutil.which('strace')
        require(bool(tracer), 'Trace requested but strace is unavailable')
        command += [tracer, '-f', '-c', '-w', '-S', 'time', '-e',
                    'trace=fsync,fdatasync,flock,rename,renameat,renameat2,unlink,unlinkat', '--']
    command += [str(output / 'binary')]
    report['argv'] = command
    try:
        run_observation(output, label, command, payload, report)
    finally:
        stdout = output / f'{label}.stdout'
        stderr = output / f'{label}.stderr'
        if stdout.exists():
            data = stdout.read_bytes()
            lines = data.split(b'\n')
            report['stdout_unterminated_tail_bytes'] = len(lines.pop())
            completed = []
            for index, line in enumerate(lines):
                try:
                    result = json.loads(line)
                    require(index < len(rows) and len(result['results']) == len(rows[index]['operations']),
                            'Output row/operation shape mismatch')
                    completed.append(rows[index]['label'])
                except (RuntimeError, ValueError, KeyError, TypeError):
                    report['output_parse_error_row'] = index
                    break
            report.update(completed_rows=len(completed), completed_labels=completed,
                          completed_operations=sum(len(rows[i]['operations']) for i in range(len(completed))),
                          next_input_label=rows[len(completed)]['label'] if len(completed) < len(rows) else None)
            report['source_batch_output_complete'] = (len(completed) == len(rows)
                and 'output_parse_error_row' not in report and not report['stdout_unterminated_tail_bytes'])
        if stderr.exists():
            text = stderr.read_text(errors='replace')
            report['lsan_ptrace_limitation'] = 'LeakSanitizer' in text and 'ptrace' in text
        # Preserve the mid-row clue that survived in the original failed ZIP,
        # even though raw Params directories are not uploaded by this workflow.
        snapshot = report['params_snapshot'] = {}
        for key in ('CarParams', 'FirmwareQueryDone', 'ControlsReady',
                    'ObdMultiplexingEnabled', 'ObdMultiplexingChanged'):
            path = params / 'panda-safety-fixture' / key
            try:
                require(path.resolve().is_relative_to(params), 'Indirect Params snapshot target')
                info = path.stat()
                require(stat.S_ISREG(info.st_mode) and info.st_size <= 4096,
                        'Params snapshot exceeded its regular-file/byte bound')
                value = path.read_bytes()
                snapshot[key] = {'bytes': list(value), 'sha256': sha(value), 'mtime_ns': info.st_mtime_ns}
            except FileNotFoundError:
                snapshot[key] = None


def main():
    os.umask(0o077)
    phase = sys.argv[1]
    require(phase in ('prepare', 'normal', 'needs-profile', 'profile'), 'Unknown phase')
    output = owned_root() / 'panda-safety-518'
    if phase == 'prepare':
        output.mkdir(mode=0o700)
    require(output.is_dir() and not output.is_symlink(), 'Missing owned diagnostic output')
    if phase == 'needs-profile':
        normal = json.loads((output / 'normal.json').read_text())
        print('true' if normal.get('outcome') == 'timeout' else 'false')
        return
    report = dict(MARKERS, phase=phase, source_commit=SOURCE,
                  probe_commit=os.environ['GITHUB_SHA'], runner=os.environ['RUNNER_NAME'])
    try:
        if phase == 'prepare':
            prepare(output, report)
        elif phase == 'profile' and json.loads((output / 'normal.json').read_text()).get('outcome') != 'timeout':
            report['skipped'] = 'Normal observation did not time out; no instrumented replay needed'
        else:
            observe(output, phase, report)
    except Exception as error:
        report['evidence_collection_error'] = type(error).__name__ + ': ' + str(error)
    finally:
        save(output / f'{phase}.json', report)
        print(json.dumps({key: value for key, value in report.items()
                          if key not in ('completed_labels', 'proc_samples', 'dependencies')}))
    if 'evidence_collection_error' in report:
        raise SystemExit('Diagnostic evidence collection failed; inspect owned receipt')


if __name__ == '__main__':
    main()
