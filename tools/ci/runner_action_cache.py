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
        report = dict(runner=os.environ['RUNNER_NAME'], runner_root=str(root), service=service,
            added_cache_variables=sorted(additions), archive_cache=str(archives),
            restart_required=os.environ.get('ACTIONS_RUNNER_ACTION_ARCHIVE_CACHE') != str(archives),
            source_commit=os.environ['GITHUB_SHA'])
        (Path(os.environ['CI_JOB_TEMP'])/'action-cache-config.json').write_text(json.dumps(report, indent=2)+'\n')
        print(json.dumps(report))
    else:
        raise ValueError('prime / configure')
