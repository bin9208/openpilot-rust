"""Stable Actions archive paths; input tools retain their private job paths.

Only an owned staging directory is moved, on the same filesystem. Neither a
previous job's staging nor a shared cache is adopted or removed.
"""
import json
import os
from pathlib import Path
import re
import sys
import tempfile


class Context:
    def __init__(self):
        if (os.environ.get('GITHUB_ACTIONS') != 'true'
                or os.environ.get('RUNNER_ENVIRONMENT') != 'self-hosted'
                or os.environ.get('RUNNER_NAME') != 'bin-actions' or os.geteuid() == 0):
            raise ValueError('registered unprivileged cache job required')
        self.job = Path(os.environ['CI_JOB_TEMP'])
        self.workspace = Path(os.environ['GITHUB_WORKSPACE'])
        self.owner = ':'.join(os.environ[k] for k in ('GITHUB_RUN_ID', 'GITHUB_RUN_ATTEMPT', 'GITHUB_JOB'))
        if (self.job.parent != Path('/tmp') or not self.job.name.startswith('openpilot-ci.')
                or self.job.resolve(strict=True) != self.job or self.job.stat().st_uid != os.getuid()
                or (self.job/'owner').is_symlink() or (self.job/'owner').read_text() != self.owner
                or self.workspace.resolve(strict=True) != self.workspace
                or self.workspace.stat().st_uid != os.getuid()):
            raise ValueError('redirected or unrelated cache job')
        self.base = self.workspace/'.analysis/native-ci-cache'
        self.journal = self.job/'cache-stages'

    def directory(self, path):
        path.mkdir(exist_ok=True)
        if path.is_symlink() or not path.is_dir() or path.stat().st_uid != os.getuid():
            raise ValueError('redirected or unrelated staging directory')

    def paths(self, name):
        if not re.fullmatch(r'native-[a-z0-9-]{1,64}', name):
            raise ValueError('invalid native cache role')
        for path in (self.workspace/'.analysis', self.base, self.journal):
            self.directory(path)
        if self.base.stat().st_dev != self.job.stat().st_dev:
            raise ValueError('cache staging requires the job filesystem; no unbudgeted copy fallback')
        return self.base/name, self.job/name, self.journal/(name+'.json')

    def begin(self, name, mode):
        if mode not in ('restore', 'save'):
            raise ValueError('explicit cache restore or save required')
        stage, target, record = self.paths(name)
        if record.exists() or record.is_symlink() or target.is_symlink():
            raise ValueError('existing or redirected cache operation')
        if (mode == 'restore' and target.exists()) or (mode == 'save' and not target.is_dir()):
            raise ValueError('cache operation would replace or lose private inputs')
        stage.mkdir()  # Refuse every pre-existing staging directory.
        lease = dict(schema=1, owner=self.owner, name=name, mode=mode,
                     stage_inode=stage.stat().st_ino,
                     original_inode=target.stat().st_ino if mode == 'save' else None,
                     returned_inode=None)
        # Persist in the job before any payload move; always-cleanup can recover
        # cancellation between the move and cache upload.
        self.persist(record, lease)
        if mode == 'save':
            target.rename(stage/'payload')
        return (stage/'payload').relative_to(self.workspace).as_posix()

    def persist(self, record, lease):
        fd, temporary = tempfile.mkstemp(prefix='.pending-', dir=self.journal)
        try:
            with os.fdopen(fd, 'w') as stream:
                json.dump(lease, stream)
                stream.flush()
                os.fsync(stream.fileno())
            os.replace(temporary, record)
            directory = os.open(self.journal, os.O_RDONLY | os.O_DIRECTORY)
            try:
                os.fsync(directory)
            finally:
                os.close(directory)
        finally:
            Path(temporary).unlink(missing_ok=True)

    def finish(self, name):
        stage, target, record = self.paths(name)
        if record.is_symlink() or stage.is_symlink() or target.is_symlink():
            raise ValueError('redirected cache ownership record')
        lease = json.loads(record.read_text())
        expected = dict(schema=1, owner=self.owner, name=name, mode=lease.get('mode'),
                        stage_inode=lease.get('stage_inode'), original_inode=lease.get('original_inode'),
                        returned_inode=lease.get('returned_inode'))
        if (lease != expected or lease['mode'] not in ('restore', 'save')
                or type(lease['stage_inode']) is not int
                or (stage.exists() and (stage.stat().st_ino != lease['stage_inode']
                                        or stage.stat().st_uid != os.getuid()))):
            raise ValueError('cache staging ownership changed')
        payload = stage/'payload'
        if payload.is_symlink() or (payload.exists() and not payload.is_dir()):
            raise ValueError('cache payload is not a real directory')
        returned = lease['returned_inode']
        if returned is None:
            if not stage.is_dir():
                raise ValueError('uncompleted cache staging disappeared')
            if payload.exists() and not target.exists():
                returned = payload.stat().st_ino
            elif (lease['mode'] == 'save' and target.is_dir() and not payload.exists()
                  and target.stat().st_ino == lease['original_inode']):
                returned = target.stat().st_ino  # Cancellation before begin's move.
            elif lease['mode'] == 'restore' and not target.exists() and not payload.exists():
                returned = 0  # A real cache miss creates no private hit.
            else:
                raise ValueError('cache recovery would replace or lose private inputs')
            lease['returned_inode'] = returned
            self.persist(record, lease)  # Durable before the return rename.
        if type(returned) is not int or returned < 0:
            raise ValueError('invalid cache return identity')
        if payload.exists():
            if target.exists() or payload.stat().st_ino != returned:
                raise ValueError('cache return identity changed')
            payload.rename(target)
        if ((returned == 0 and target.exists())
                or (returned != 0 and (not target.is_dir() or target.stat().st_ino != returned))):
            raise ValueError('returned private cache identity changed')
        # Unknown contents are retained: never recursively delete a staging tree.
        if stage.exists():
            if any(stage.iterdir()):
                raise ValueError('unexpected staging content retained')
            stage.rmdir()
        record.unlink()

    def cleanup(self):
        if not self.journal.exists():
            return
        if self.journal.is_symlink():
            raise ValueError('redirected cache journal')
        for record in sorted(self.journal.iterdir()):
            if record.name.startswith('.pending-') and record.is_file() and not record.is_symlink():
                continue  # An interrupted atomic write stays in the owned job.
            if record.suffix != '.json' or record.is_symlink():
                raise ValueError('unknown cache ownership journal')
            self.finish(record.stem)


if __name__ == '__main__':
    context = Context()
    operation, *arguments = sys.argv[1:]
    if operation == 'begin' and len(arguments) == 2:
        name, mode = arguments
        path = context.begin(name, mode)
        with Path(os.environ['GITHUB_OUTPUT']).open('a') as output:
            output.write('path='+path+'\n')
    elif operation == 'finish' and len(arguments) == 1:
        context.finish(arguments[0])
    elif operation == 'cleanup' and not arguments:
        context.cleanup()
    else:
        raise SystemExit('begin NAME restore|save / finish NAME / cleanup')
