"""Persistent downloads on registered runners; build and test output stays private."""
from contextlib import contextmanager
import fcntl
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import runpy
import shutil
import stat
import subprocess
import sys
import tempfile


def directory(path):
    path.mkdir(mode=0o700, exist_ok=True)
    info = path.lstat()
    if not stat.S_ISDIR(info.st_mode) or info.st_uid != os.getuid() or info.st_mode & 0o022:
        raise ValueError('download cache must be an owned, non-writable-by-others directory')
    return path


def cache_root():
    home = Path.home()
    if not home.is_absolute() or home.resolve(strict=True) != home:
        raise ValueError('canonical runner home required')
    cache = directory(home/'.cache')
    return directory(directory(cache/'openpilot-ci-v1')/platform.machine())


def setup(root, env):
    defaults = dict(PIP_CACHE_DIR=root/'pip', UV_CACHE_DIR=root/'uv',
                    npm_config_cache=root/'npm', GRADLE_USER_HOME=root/'gradle')
    result = {'CI_DOWNLOAD_CACHE_ROOT': str(root)}
    for name, path in defaults.items():
        # Preserve explicit per-job caches, including cold-cache comparison fixtures.
        if not env.get(name):
            directory(path)
            result[name] = str(path)
    for name, value in (('PIP_DISABLE_PIP_VERSION_CHECK', '1'), ('NPM_CONFIG_UPDATE_NOTIFIER', 'false')):
        if not env.get(name):
            result[name] = value
    return result


def apt_cache(root, view, architecture):
    if not re.fullmatch(r'[a-z0-9][a-z0-9-]*', architecture):
        raise ValueError('invalid native APT architecture')
    identity = [architecture.encode(), (view/'sources.list').read_bytes(),
                (view/'sources.list.d/official.sources').read_bytes()]
    digest = hashlib.sha256(b'\0'.join(identity)).hexdigest()
    cache = directory(directory(root/'apt')/digest)
    for name in ('lists', 'archives', 'metadata'):
        directory(cache/name)
    return cache


class InputCache:
    def __init__(self, root, name, key):
        if not re.fullmatch(r'native-[a-z0-9-]{1,64}', name) or not key or '\n' in key:
            raise ValueError('invalid input cache identity')
        self.identity = dict(schema=1, name=name, key=key)
        digest = hashlib.sha256(json.dumps(self.identity, sort_keys=True).encode()).hexdigest()
        self.base = directory(root/'inputs')
        self.entry = self.base/digest
        self.lock = self.base/(digest+'.lock')

    @contextmanager
    def locked(self):
        fd = os.open(self.lock, os.O_CREAT | os.O_RDWR | os.O_NOFOLLOW, 0o600)
        try:
            info = os.fstat(fd)
            if not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid() or info.st_nlink != 1:
                raise ValueError('redirected input cache lock')
            fcntl.flock(fd, fcntl.LOCK_EX)
            yield
        finally:
            os.close(fd)

    def present(self):
        if not self.entry.exists() and not self.entry.is_symlink():
            return False
        directory(self.entry)
        identity = self.entry/'identity.json'
        if identity.is_symlink() or json.loads(identity.read_text()) != self.identity:
            raise ValueError('input cache identity changed')
        payload = self.entry/'payload'
        if payload.is_symlink() or not payload.is_dir():
            raise ValueError('redirected input cache payload')
        return True

    def restore(self, target):
        with self.locked():
            if not self.present():
                return False
            if target.exists() or target.is_symlink():
                raise ValueError('refusing to replace private input data')
            # Copies/reflinks have independent inodes; consumers cannot mutate the cache.
            subprocess.run(['cp', '-a', '--reflink=auto', '--', str(self.entry/'payload'), str(target)], check=True)
            return True

    def save(self, source):
        if source.is_symlink() or not source.is_dir():
            raise ValueError('private input directory required')
        with self.locked():
            if self.present():
                return True
            size = sum(p.lstat().st_size for p in source.rglob('*') if p.is_file() and not p.is_symlink())
            if shutil.disk_usage(self.base).free < size:
                print('Input cache save skipped: insufficient space for its estimated copy growth', file=sys.stderr)
                return False
            with tempfile.TemporaryDirectory(prefix='.pending-', dir=self.base) as temporary:
                stage = Path(temporary)/'entry'
                stage.mkdir(mode=0o700)
                subprocess.run(['cp', '-a', '--reflink=auto', '--', str(source), str(stage/'payload')], check=True)
                (stage/'identity.json').write_text(json.dumps(self.identity, sort_keys=True))
                stage.rename(self.entry)
            return True


def private_target(name):
    job = Path(os.environ['CI_JOB_TEMP'])
    owner = ':'.join(os.environ[k] for k in ('GITHUB_RUN_ID', 'GITHUB_RUN_ATTEMPT', 'GITHUB_JOB'))
    if (job.parent != Path('/tmp') or not job.name.startswith('openpilot-ci.')
            or job.resolve(strict=True) != job or job.stat().st_uid != os.getuid()
            or (job/'owner').is_symlink() or (job/'owner').read_text() != owner):
        raise ValueError('owned private cache target required')
    return job/name


def main():
    runpy.run_path(str(Path(__file__).with_name('runner_policy.py')))['require_environment'](self_hosted_only=True)
    if os.geteuid() == 0:
        raise ValueError('unprivileged runner account required')
    root = cache_root()
    operation, *args = sys.argv[1:]
    if operation == 'setup' and not args:
        values = setup(root, os.environ)
        with Path(os.environ['GITHUB_ENV']).open('a') as stream:
            for name, value in values.items():
                if '\n' in value or '\r' in value:
                    raise ValueError('multiline cache environment')
                stream.write(name+'='+value+'\n')
        print(json.dumps(dict(runner=os.environ['RUNNER_NAME'], persistent_downloads=values,
                             cargo_home=os.environ.get('CARGO_HOME', str(Path.home()/'.cargo')),
                             rustup_home=os.environ.get('RUSTUP_HOME', str(Path.home()/'.rustup')))))
    elif operation == 'apt' and len(args) == 2:
        print(apt_cache(root, Path(args[0]), args[1]))
    elif operation in ('restore', 'save') and len(args) == 2:
        name, key = args
        cache = InputCache(root, name, key)
        hit = getattr(cache, operation)(private_target(name))
        with Path(os.environ['GITHUB_OUTPUT']).open('a') as stream:
            stream.write('cache-hit='+str(hit).lower()+'\n')
        print(json.dumps(dict(operation=operation, name=name, key=key, cache_hit=hit,
                              cache_service_transferred_bytes=0, runner=os.environ['RUNNER_NAME'])))
    else:
        raise ValueError('setup / apt VIEW ARCH / restore|save NAME KEY')


if __name__ == '__main__':
    main()
