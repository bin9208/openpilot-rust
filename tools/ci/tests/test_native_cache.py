"""Real archive handoff across private jobs and cancellation ownership cases."""
import hashlib
import importlib.util
import json
import os
import runpy
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location('owned_native_cache', Path(__file__).resolve().parents[1]/'native_cache.py')
cache = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(cache)
ROLE = 'native-cache-fixture'
policy = runpy.run_path(str(Path(__file__).resolve().parents[1]/'runner_policy.py'))


class NativeCacheTests(unittest.TestCase):
    def setUp(self):
        self.root = Path(self.enterContext(tempfile.TemporaryDirectory()))
        self.workspace = self.root/'workspace'
        self.workspace.mkdir()

    def job(self, number, **runner):
        job = Path(self.enterContext(tempfile.TemporaryDirectory(prefix='openpilot-ci.', dir='/tmp')))
        owner = f'123:1:job{number}'
        (job/'owner').write_text(owner)
        environment = dict(GITHUB_ACTIONS='true', RUNNER_ENVIRONMENT='self-hosted', RUNNER_NAME='bin-actions',
                           CI_JOB_TEMP=str(job), GITHUB_WORKSPACE=str(self.workspace),
                           GITHUB_RUN_ID='123', GITHUB_RUN_ATTEMPT='1', GITHUB_JOB=f'job{number}')
        environment.update(runner)
        with patch.dict(os.environ, environment):
            return cache.Context()

    def test_registered_private_and_hosted_native_jobs_keep_private_cache_ownership(self):
        contexts = [dict(RUNNER_NAME='bin-actions2'), dict(RUNNER_NAME='bin-actions3'),
                    dict(RUNNER_ENVIRONMENT='github-hosted', RUNNER_NAME='Hosted X64', RUNNER_OS='Linux', RUNNER_ARCH='X64'),
                    dict(RUNNER_ENVIRONMENT='github-hosted', RUNNER_NAME='Hosted ARM', RUNNER_OS='Linux', RUNNER_ARCH='ARM64')]
        for number, runner in enumerate(contexts, 10):
            with self.subTest(runner=runner):
                context = self.job(number, **runner)
                target = self.inputs(context)
                context.begin(ROLE, 'save')
                context.cleanup()
                self.assertEqual((target/'blob').read_bytes(), b'independently pinned body\n')

    def test_policy_rejects_unregistered_or_mismatched_runner_evidence(self):
        allowed, jobs = policy['environment_allowed'], policy['job_is_allowed']
        private = dict(GITHUB_ACTIONS='true', RUNNER_ENVIRONMENT='self-hosted', RUNNER_NAME='bin-actions2')
        hosted = dict(GITHUB_ACTIONS='true', RUNNER_ENVIRONMENT='github-hosted', RUNNER_OS='Linux', RUNNER_ARCH='ARM64')
        self.assertTrue(allowed(private, self_hosted_only=True))
        self.assertTrue(allowed({**private, 'RUNNER_NAME': 'bin-actions3'}, self_hosted_only=True))
        self.assertTrue(allowed(hosted))
        self.assertFalse(allowed(hosted, self_hosted_only=True))
        self.assertFalse(allowed({**private, 'RUNNER_NAME': 'other-session'}))
        self.assertFalse(allowed({**hosted, 'RUNNER_OS': 'Windows'}))
        self.assertFalse(allowed({**hosted, 'GITHUB_ACTIONS': 'false'}))
        registered = dict(runner_name='bin-actions2', labels=['self-hosted', 'Linux', 'X64', 'support-630'])
        native = dict(runner_name='GitHub Actions', labels=['ubuntu-24.04-arm'])
        self.assertTrue(jobs(registered, self_hosted_only=True))
        self.assertTrue(jobs(dict(runner_name='bin-actions3', labels=['self-hosted', 'Linux', 'X64']),
                             self_hosted_only=True))
        self.assertTrue(jobs(native))
        self.assertFalse(jobs(native, self_hosted_only=True))
        self.assertFalse(jobs({**registered, 'runner_name': 'other-session'}))
        self.assertFalse(jobs({**registered, 'labels': ['self-hosted', 'Linux']}))
        self.assertFalse(jobs({**native, 'runner_name': ''}))
        self.assertFalse(jobs({**native, 'labels': ['self-hosted', 'ubuntu-24.04-arm']}))
        self.assertFalse(jobs({**native, 'labels': ['ubuntu-latest']}))

    def inputs(self, context):
        target = context.job/ROLE
        target.mkdir()
        (target/'blob').write_bytes(b'independently pinned body\n')
        (target/'blob').chmod(0o444)
        return target

    def test_archive_restores_to_a_different_job_without_old_temp_paths(self):
        first = self.job(1)
        target = self.inputs(first)
        expected = hashlib.sha256((target/'blob').read_bytes()).hexdigest()
        stable = first.begin(ROLE, 'save')
        self.assertNotIn(str(first.job), stable)
        archive = self.root/'cache.tar'
        with tarfile.open(archive, 'w') as stream:
            stream.add(self.workspace/stable, arcname=stable)
        first.finish(ROLE)
        self.assertEqual(hashlib.sha256((target/'blob').read_bytes()).hexdigest(), expected)
        second = self.job(2)
        self.assertNotEqual(first.job, second.job)
        self.assertEqual(second.begin(ROLE, 'restore'), stable)
        with tarfile.open(archive) as stream:
            stream.extractall(self.workspace, filter='data')
        second.finish(ROLE)
        self.assertEqual(hashlib.sha256((second.job/ROLE/'blob').read_bytes()).hexdigest(), expected)
        self.assertFalse((self.workspace/stable).parent.exists())

    def test_cancelled_save_returns_only_its_private_inputs(self):
        context = self.job(1)
        target = self.inputs(context)
        context.begin(ROLE, 'save')
        self.assertFalse(target.exists())
        context.cleanup()
        self.assertEqual((target/'blob').read_bytes(), b'independently pinned body\n')
        self.assertEqual(list(context.journal.iterdir()), [])

    def test_cache_miss_does_not_create_a_false_private_hit(self):
        context = self.job(1)
        context.begin(ROLE, 'restore')
        context.finish(ROLE)
        self.assertFalse((context.job/ROLE).exists())

    def test_interruption_after_return_rename_is_recoverable(self):
        context = self.job(1)
        target = self.inputs(context)
        stable = context.begin(ROLE, 'save')
        original = Path.rename
        def interrupted(path, destination):
            result = original(path, destination)
            if path == self.workspace/stable:
                raise InterruptedError('owned cancellation immediately after rename')
            return result
        with patch.object(Path, 'rename', interrupted):
            with self.assertRaises(InterruptedError):
                context.finish(ROLE)
        context.cleanup()
        self.assertEqual((target/'blob').read_bytes(), b'independently pinned body\n')
        self.assertEqual(list(context.journal.iterdir()), [])

    def test_other_job_staging_is_preserved_on_collision(self):
        first, second = self.job(1), self.job(2)
        self.inputs(first)
        stable = first.begin(ROLE, 'save')
        before = (self.workspace/stable/'blob').read_bytes()
        with self.assertRaises(FileExistsError):
            second.begin(ROLE, 'restore')
        second.cleanup()
        self.assertEqual((self.workspace/stable/'blob').read_bytes(), before)
        first.cleanup()

    def test_changed_lease_or_inode_is_not_adopted_by_cleanup(self):
        context = self.job(1)
        self.inputs(context)
        stable = context.begin(ROLE, 'save')
        record = context.journal/(ROLE+'.json')
        row = json.loads(record.read_text())
        row['stage_inode'] += 1
        record.write_text(json.dumps(row))
        with self.assertRaisesRegex(ValueError, 'ownership changed'):
            context.cleanup()
        self.assertTrue((self.workspace/stable/'blob').is_file())

    def test_redirected_input_is_never_cached_or_replaced(self):
        context = self.job(1)
        other = self.root/'other'
        other.mkdir()
        (context.job/ROLE).symlink_to(other, target_is_directory=True)
        with self.assertRaisesRegex(ValueError, 'redirected'):
            context.begin(ROLE, 'save')
        self.assertTrue((context.job/ROLE).is_symlink())


if __name__ == '__main__':
    unittest.main()
