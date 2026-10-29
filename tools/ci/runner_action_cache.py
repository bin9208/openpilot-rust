"""Prime the runner's native SHA-addressed action archive cache."""
import hashlib
import json
import os
from pathlib import Path
import re
import runpy
import shutil
import stat
import subprocess
import sys
import tempfile

DOWNLOADS = runpy.run_path(str(Path(__file__).with_name('runner_download_cache.py')))


def action_root():
    path = Path(os.environ.get('ACTIONS_RUNNER_ACTION_ARCHIVE_CACHE',
                               str(DOWNLOADS['cache_root']()/'action-archives')))
    if not path.is_absolute() or path.parent.resolve(strict=True) != path.parent:
        raise ValueError('canonical action archive cache required')
    return DOWNLOADS['directory'](path)


def prime(root, actions):
    report = []
    for action in actions:
        repository, sha = action['repository'], action['sha']
        if not re.fullmatch(r'[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+', repository) or not re.fullmatch(r'[a-f0-9]{40}', sha):
            raise ValueError('invalid pinned action identity')
        base = DOWNLOADS['directory'](root/repository.replace('/', '_'))
        target = base/(sha+'.tar.gz')
        receipt = base/(sha+'.json')
        # Share the same safe lock implementation as immutable input publication.
        lock = DOWNLOADS['InputCache'](root, 'native-action-archive', repository+'@'+sha)
        with lock.locked():
            hit = target.exists() and receipt.exists()
            if target.is_symlink() or receipt.is_symlink():
                raise ValueError('redirected action archive')
            if hit:
                record = json.loads(receipt.read_text())
                with target.open('rb') as stream:
                    digest = hashlib.file_digest(stream, 'sha256').hexdigest()
                if record != dict(repository=repository, sha=sha, sha256=digest, bytes=target.stat().st_size):
                    raise ValueError('action archive differs from its pinned download receipt')
            else:
                if shutil.disk_usage(base).free < 128*1024**2:
                    raise ValueError('insufficient space for estimated action archive download growth (128 MiB)')
                fd, temporary = tempfile.mkstemp(prefix='.download-', dir=base)
                os.close(fd)
                try:
                    subprocess.run(['curl', '--proto', '=https', '--tlsv1.2', '--fail', '--location',
                        '--retry', '5', '--retry-all-errors', '--max-filesize', str(128*1024**2),
                        '--silent', '--show-error', 'https://codeload.github.com/'+repository+'/tar.gz/'+sha,
                        '--output', temporary], check=True)
                    archive = Path(temporary)
                    with archive.open('rb') as stream:
                        digest = hashlib.file_digest(stream, 'sha256').hexdigest()
                    record = dict(repository=repository, sha=sha, sha256=digest, bytes=archive.stat().st_size)
                    # An interrupted pair is refetched by exact SHA on the next call.
                    # The runner only observes a complete archive after this rename.
                    receipt.write_text(json.dumps(record, sort_keys=True)+'\n')
                    os.replace(archive, target)
                finally:
                    Path(temporary).unlink(missing_ok=True)
            report.append(dict(repository=repository, sha=sha, cache_hit=hit,
                               downloaded_bytes=0 if hit else target.stat().st_size))
    return report


def runner_root():
    pid = os.getpid()
    while pid > 1:
        process = Path('/proc')/str(pid)
        executable = (process/'exe').resolve(strict=True)
        if executable.name == 'Runner.Listener':
            root = executable.parent.parent
            if not (root/'run.sh').is_file() or root.stat().st_uid != os.getuid():
                raise ValueError('runner installation is not owned by this account')
            return root
        pid = int((process/'stat').read_text().rsplit(')', 1)[1].split()[1])
    raise ValueError('registered runner listener ancestor required')


def configure(root, archive_root, defaults):
    path = root/'.env'
    if path.is_symlink():
        raise ValueError('redirected runner environment')
    previous = path.read_text() if path.exists() else ''
    if path.exists() and (not stat.S_ISREG(path.lstat().st_mode) or path.stat().st_uid != os.getuid()):
        raise ValueError('owned runner environment required')
    existing = {line.partition('=')[0]: line.partition('=')[2]
                for line in previous.splitlines() if '=' in line}
    values = {'ACTIONS_RUNNER_ACTION_ARCHIVE_CACHE': str(archive_root), **defaults}
    additions = {key: value for key, value in values.items() if not existing.get(key)}
    if any('\n' in value or '\r' in value for value in additions.values()):
        raise ValueError('multiline runner cache path')
    if additions:
        fd, temporary = tempfile.mkstemp(prefix='.cache-env-', dir=root)
        try:
            with os.fdopen(fd, 'w') as stream:
                stream.write(previous)
                if previous and not previous.endswith('\n'):
                    stream.write('\n')
                for key, value in additions.items():
                    stream.write(key+'='+value+'\n')
                stream.flush()
                os.fsync(stream.fileno())
            os.chmod(temporary, stat.S_IMODE(path.stat().st_mode) if path.exists() else 0o600)
            os.replace(temporary, path)
        finally:
            Path(temporary).unlink(missing_ok=True)
    return additions


def observed_archive_hits(root):
    paths = sorted((root/'_diag').glob('Worker_*.log'), key=lambda p: p.stat().st_mtime, reverse=True)
    if not paths:
        return []
    with paths[0].open('rb') as stream:
        stream.seek(max(0, paths[0].stat().st_size-256*1024))
        lines = stream.read().decode('utf-8', errors='replace').splitlines()
    # Return only the runner's archive-copy diagnostic, never environment/log contents.
    return [line for line in lines if 'Found action archive ' in line and ' in cache directory ' in line]


def schedule_reload(root, service, marker):
    if not service or not re.fullmatch(r'actions\.runner\.[A-Za-z0-9_.-]+\.service', service):
        return False
    # A transient unit survives its owning Actions worker. It can only restart
    # after this runner's final proof job leaves its marker AND all workers exit.
    program = '''import json, pathlib, subprocess, sys, time
root, marker, service = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2]), sys.argv[3]
worker = root/'bin/Runner.Worker'
for attempt in range(3600):
    active = False
    for process in pathlib.Path('/proc').glob('[0-9]*'):
        try:
            if (process/'exe').resolve(strict=True) == worker:
                active = True
                break
        except (OSError, RuntimeError):
            continue
    if marker.is_file() and not active:
        result = subprocess.run(['/usr/bin/sudo', '-n', '/usr/bin/systemctl', 'restart', service])
        marker.with_suffix('.json').write_text(json.dumps(dict(service=service, exit_code=result.returncode)))
        raise SystemExit(result.returncode)
    time.sleep(2)
raise SystemExit('cache reload expired without interrupting an Actions worker')
'''
    unit = 'openpilot-cache-644-'+os.environ['RUNNER_NAME']+'-'+os.environ['GITHUB_RUN_ID']
    result = subprocess.run(['/usr/bin/sudo', '-n', '/usr/bin/systemd-run', '--quiet', '--collect',
        '--unit='+unit, '--on-active=15s', '--property=User='+str(os.getuid()),
        '/usr/bin/python3', '-I', '-B', '-S', '-c', program, str(root), str(marker), service],
        capture_output=True, text=True)
    if result.returncode:
        print('Idle runner reload could not be scheduled: '+result.stderr.strip(), file=sys.stderr)
    return result.returncode == 0


if __name__ == '__main__':
    runpy.run_path(str(Path(__file__).with_name('runner_policy.py')))['require_environment'](self_hosted_only=True)
    if os.geteuid() == 0:
        raise ValueError('unprivileged runner required')
    operation, = sys.argv[1:]
    archives = action_root()
    if operation == 'prime':
        manifest = json.loads(Path(__file__).with_name('action-archives.json').read_text())
        print(json.dumps(dict(archive_cache=str(archives), actions=prime(archives, manifest['actions']))))
    elif operation == 'configure':
        root = runner_root()
        values = DOWNLOADS['setup'](DOWNLOADS['cache_root'](), {})
        additions = configure(root, archives, values)
        service = (root/'.service').read_text().strip() if (root/'.service').is_file() else None
        restart_required = os.environ.get('ACTIONS_RUNNER_ACTION_ARCHIVE_CACHE') != str(archives)
        marker = DOWNLOADS['cache_root']()/('reload-'+os.environ['GITHUB_RUN_ID']+'-'+os.environ['RUNNER_NAME']+'.ready')
        scheduled = schedule_reload(root, service, marker) if restart_required else False
        report = dict(runner=os.environ['RUNNER_NAME'], runner_root=str(root), service=service,
            added_cache_variables=sorted(additions), archive_cache=str(archives),
            restart_required=restart_required, idle_reload_scheduled=scheduled,
            actual_runner_archive_hits=observed_archive_hits(root),
            source_commit=os.environ['GITHUB_SHA'])
        (Path(os.environ['CI_JOB_TEMP'])/'action-cache-config.json').write_text(json.dumps(report, indent=2)+'\n')
        print(json.dumps(report))
    else:
        raise ValueError('prime / configure')
