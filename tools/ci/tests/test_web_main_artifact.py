"""Exercise #512's real workflow archive script with sudo, in Actions only."""
import hashlib
import json
import os
from pathlib import Path
import stat
import subprocess
import tarfile
import tempfile
import unittest

import yaml


ROOT = Path(__file__).resolve().parents[3]
WORKFLOW = Path(os.environ.get('BACKLOG_WEB_WORKFLOW', ROOT / '.github/workflows/backlog-fixes-497.yml'))
STEP = 'Archive owned main web evidence for the runner'
CASES = ('term', 'supervisor', 'force')
ARCHIVE = 'web-main-evidence.tar.gz'
MANIFEST = 'web-main-evidence.sha256'

# Every privileged fixture operation, including teardown, checks the original
# caller-owned directory identity. Never clean a path taken from a negative env.
ROOT_GUARD = """
import json, os, shutil, stat, sys
from pathlib import Path
root = Path(sys.argv[1])
expected = json.loads(sys.argv[2])
assert os.geteuid() == 0
assert root.parent == Path('/tmp') and root.name.startswith('openpilot-ci.')
info = root.lstat()
assert stat.S_ISDIR(info.st_mode) and not root.is_symlink()
assert [info.st_dev, info.st_ino, info.st_uid] == expected
"""

CREATE_SOURCE = """
public = root / 'web-main'
public.mkdir(mode=0o700)
for name in ('owned-key.pem', 'owned-cert.pem'):
    path = public / name
    path.write_bytes(b'issue512-private-parent:' + name.encode())
    path.chmod(0o600)
external = root / 'external-key.pem'
external.write_bytes(b'issue512-private-external-key')
external.chmod(0o600)
for case in json.loads(sys.argv[3]):
    directory = public / case
    for part in (directory, directory / 'params', directory / 'params/d'):
        part.mkdir(mode=0o700)
    parameter = directory / 'params/d/CarrotNaviHudMapProfile'
    parameter.write_bytes(b'issue512-root-param:' + case.encode())
    parameter.chmod(0o600)
"""

SOURCE_METADATA = """
paths = [root / 'web-main', root / 'web-main/owned-key.pem', root / 'external-key.pem']
for case in json.loads(sys.argv[3]):
    directory = root / 'web-main' / case
    paths.extend((directory, directory / 'params', directory / 'params/d',
                  directory / 'params/d/CarrotNaviHudMapProfile'))
result = {}
for path in paths:
    info = path.lstat()
    result[str(path.relative_to(root))] = [info.st_uid, info.st_gid,
        stat.S_IMODE(info.st_mode), info.st_size, info.st_mtime_ns]
print(json.dumps(result))
"""


class WebMainArtifact512Tests(unittest.TestCase):
    def setUp(self):
        self.assertEqual(os.environ.get('GITHUB_ACTIONS'), 'true', 'sudo fixtures run only in Actions')
        self.assertNotEqual(os.geteuid(), 0, 'must prove the unprivileged upload reader boundary')
        self.owner = ':'.join(os.environ[key] for key in ('GITHUB_RUN_ID', 'GITHUB_RUN_ATTEMPT', 'GITHUB_JOB'))
        steps = yaml.load(WORKFLOW.read_text(), Loader=yaml.BaseLoader)['jobs']['focused']['steps']
        archive_steps = [step for step in steps if step.get('name') == STEP]
        self.assertEqual(len(archive_steps), 1, 'execute the actual registered archive step')
        self.script = archive_steps[0]['run']
        self.identities = {}

    def new_root(self):
        # job-temp sets TMPDIR; its default would nest this fixture and rightly
        # fail the production script's direct-/tmp guard.
        temporary = tempfile.TemporaryDirectory(prefix='openpilot-ci.', dir='/tmp')
        root = Path(temporary.name)
        info = root.lstat()
        self.identities[root] = [info.st_dev, info.st_ino, info.st_uid]
        self.addCleanup(temporary.cleanup)
        self.addCleanup(self.root_python, root, 'shutil.rmtree(root)')
        (root / 'owner').write_text(self.owner)
        return root

    def root_python(self, root, source, *args):
        result = subprocess.run(
            ['sudo', '-n', '/usr/bin/python3', '-c', ROOT_GUARD + source,
             str(root), json.dumps(self.identities[root]), *args],
            text=True, capture_output=True, timeout=15, check=False)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        return result.stdout

    def create_source(self, root, cases=CASES):
        self.root_python(root, CREATE_SOURCE, json.dumps(cases))

    def metadata(self, root, cases=CASES):
        return json.loads(self.root_python(root, SOURCE_METADATA, json.dumps(cases)))

    def run_archive(self, root, *, job_temp=None):
        environment = dict(os.environ, CI_JOB_TEMP=str(root if job_temp is None else job_temp))
        environment.pop('BASH_ENV', None)  # Do not source the outer job's tempfile wrapper.
        command = ['/bin/bash', '-euo', 'pipefail', '-c', self.script]
        process = subprocess.Popen(command, cwd=root, env=environment, stdout=subprocess.PIPE,
                                   stderr=subprocess.PIPE, text=True, start_new_session=True)
        try:
            try:
                stdout, stderr = process.communicate(timeout=15)
            except subprocess.TimeoutExpired:
                # A regression must not strand privileged readers on a FIFO.
                # The fresh session contains only this invocation and its sudo.
                subprocess.run(['sudo', '-n', '/bin/kill', '-KILL', '--', f'-{process.pid}'],
                               capture_output=True, timeout=5, check=True)
                process.communicate(timeout=5)
                self.fail('workflow archive script exceeded its 15-second fixture deadline')
        finally:
            # Popen's context manager would wait without a bound if escalation
            # itself failed. Reap only our child, with a deadline on every path.
            if process.returncode is None:
                process.kill()
                process.wait(timeout=5)
            process.stdout.close()
            process.stderr.close()
        return subprocess.CompletedProcess(command, process.returncode, stdout, stderr)

    def assert_archive(self, root, result, cases):
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn(f'main_evidence_cases={cases}', result.stdout)
        archive = root / ARCHIVE
        info = archive.stat()
        self.assertEqual(info.st_uid, os.getuid())
        self.assertEqual(stat.S_IMODE(info.st_mode), 0o600)
        digest = hashlib.sha256(archive.read_bytes()).hexdigest()
        self.assertEqual((root / MANIFEST).read_text(), f'{digest}  {archive}\n')
        self.assertEqual(list(root.glob('.web-main-evidence.*')), [])

    def assert_rejected(self, root, result):
        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        for name in (ARCHIVE, MANIFEST):
            self.assertFalse(os.path.lexists(root / name), name)
        self.assertEqual(list(root.glob('.web-main-evidence.*')), [])

    def test_root_only_params_are_readable_in_runner_owned_archive_with_source_metadata_intact(self):
        root = self.new_root()
        self.create_source(root)
        before = self.metadata(root)
        self.assertEqual(before['web-main'][:3], [0, 0, 0o700])
        for case in CASES:
            name = f'web-main/{case}/params/d/CarrotNaviHudMapProfile'
            self.assertEqual(before[name][:3], [0, 0, 0o600])
            with self.assertRaises(PermissionError):
                (root / name).read_bytes()
        self.assert_archive(root, self.run_archive(root), 3)
        with tarfile.open(root / ARCHIVE, 'r:gz') as archive:
            members = {member.name: member for member in archive.getmembers()}
            self.assertEqual(set(members), {case + suffix for case in CASES for suffix in
                ('', '/params', '/params/d', '/params/d/CarrotNaviHudMapProfile')})
            for case in CASES:
                member = members[f'{case}/params/d/CarrotNaviHudMapProfile']
                self.assertTrue(member.isfile())
                self.assertEqual((member.uid, member.gid, member.mode), (0, 0, 0o600))
                self.assertEqual(archive.extractfile(member).read(), b'issue512-root-param:' + case.encode())
        self.assertEqual(self.metadata(root), before)

    def test_private_keys_links_fifo_and_socket_are_excluded_without_blocking(self):
        root = self.new_root()
        self.create_source(root, ('term',))
        self.root_python(root, r"""
import socket
case = root / 'web-main/term'
(case / 'line\nbreak.json').write_bytes(b'issue512-public-newline-record')
(case / 'key-link.pem').symlink_to(root / 'external-key.pem')
(case / 'parent-link').symlink_to(root / 'web-main', target_is_directory=True)
(case / 'dangling-link').symlink_to(root / 'missing-key')
os.mkfifo(case / 'ignored.fifo', 0o600)
with socket.socket(socket.AF_UNIX) as connection:
    connection.bind(str(case / 'ignored.sock'))
""")
        before = self.metadata(root, ('term',))
        self.assert_archive(root, self.run_archive(root), 1)
        with tarfile.open(root / ARCHIVE, 'r:gz') as archive:
            members = archive.getmembers()
            self.assertEqual({member.name for member in members}, {
                'term', 'term/params', 'term/params/d', 'term/params/d/CarrotNaviHudMapProfile',
                'term/line\nbreak.json'})
            self.assertTrue(all(member.isdir() or member.isfile() for member in members))
            self.assertEqual(archive.extractfile('term/params/d/CarrotNaviHudMapProfile').read(),
                             b'issue512-root-param:term')
            self.assertEqual(archive.extractfile('term/line\nbreak.json').read(), b'issue512-public-newline-record')
        self.assertEqual(self.metadata(root, ('term',)), before)

    def test_no_case_directories_produces_empty_archive_excluding_private_parent_key(self):
        root = self.new_root()
        self.create_source(root, ())
        before = self.metadata(root, ())
        self.assert_archive(root, self.run_archive(root), 0)
        with tarfile.open(root / ARCHIVE, 'r:gz') as archive:
            self.assertEqual(archive.getmembers(), [])
        self.assertEqual(self.metadata(root, ()), before)

    def test_foreign_owner_indirect_path_and_invalid_case_are_rejected(self):
        for invalid in ('owner', 'owner-link', 'nested-path', 'public-link', 'case-link', 'case-file'):
            with self.subTest(invalid=invalid):
                root = self.new_root()
                self.create_source(root, ())
                job_temp = root
                if invalid == 'owner':
                    (root / 'owner').write_text('another:attempt:job')
                elif invalid == 'owner-link':
                    (root / 'owner').rename(root / 'real-owner')
                    (root / 'owner').symlink_to(root / 'real-owner')
                elif invalid == 'nested-path':
                    job_temp = root / 'nested'
                    job_temp.mkdir()
                    (job_temp / 'owner').write_text(self.owner)
                elif invalid == 'public-link':
                    self.root_python(root, """
(root / 'web-main').rename(root / 'private-source')
(root / 'web-main').symlink_to(root / 'private-source', target_is_directory=True)
""")
                elif invalid == 'case-link':
                    self.root_python(root, """
(root / 'outside-case').mkdir()
(root / 'outside-case/private.pem').write_bytes(b'issue512-outside-case')
(root / 'web-main/term').symlink_to(root / 'outside-case', target_is_directory=True)
""")
                else:
                    self.root_python(root, "(root / 'web-main/term').write_bytes(b'not a case directory')")
                result = self.run_archive(root, job_temp=job_temp)
                self.assert_rejected(root, result)
                # This also catches accidental publication under a rejected env.
                if invalid == 'nested-path':
                    self.assert_rejected(job_temp, result)

    def test_existing_archive_or_manifest_bytes_and_symlinks_are_never_replaced(self):
        for name in (ARCHIVE, MANIFEST):
            for kind in ('file', 'symlink'):
                with self.subTest(name=name, kind=kind):
                    root = self.new_root()
                    self.create_source(root, ())
                    sentinel = root / 'foreign-evidence'
                    sentinel.write_bytes(b'issue512-preserve-foreign-bytes')
                    destination = root / name
                    if kind == 'file':
                        destination.write_bytes(sentinel.read_bytes())
                    else:
                        destination.symlink_to(sentinel)
                    before = destination.lstat()
                    result = self.run_archive(root)
                    self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
                    after = destination.lstat()
                    for field in ('st_dev', 'st_ino', 'st_uid', 'st_gid', 'st_mode', 'st_size', 'st_mtime_ns'):
                        self.assertEqual(getattr(after, field), getattr(before, field), field)
                    if kind == 'symlink':
                        self.assertEqual(destination.readlink(), sentinel)
                    self.assertEqual(destination.read_bytes(), b'issue512-preserve-foreign-bytes')
                    self.assertEqual(sentinel.read_bytes(), b'issue512-preserve-foreign-bytes')
                    self.assertFalse(os.path.lexists(root / (MANIFEST if name == ARCHIVE else ARCHIVE)))
                    self.assertEqual(list(root.glob('.web-main-evidence.*')), [])


if __name__ == '__main__':
    unittest.main()
