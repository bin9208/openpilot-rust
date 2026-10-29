import importlib.util
import json
import multiprocessing
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

MODULE_PATH = Path(__file__).resolve().parents[1]/'runner_download_cache.py'
spec = importlib.util.spec_from_file_location('runner_download_cache', MODULE_PATH)
cache = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cache)
action_spec = importlib.util.spec_from_file_location('runner_action_cache', MODULE_PATH.with_name('runner_action_cache.py'))
actions = importlib.util.module_from_spec(action_spec)
action_spec.loader.exec_module(actions)


def save_in_process(root, source, ready):
    try:
        cache.InputCache(Path(root), 'native-fixture', 'locked-v1').save(Path(source))
        ready.put('ok')
    except Exception as error:
        ready.put(repr(error))


class DownloadCacheTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.source = self.root/'source'
        self.source.mkdir()
        (self.source/'blob').write_bytes(b'known input\n')
        (self.source/'relative-link').symlink_to('blob')

    def test_private_copy_is_independent_and_exact_key_required(self):
        store = cache.InputCache(self.root, 'native-fixture', 'pinned-v1')
        self.assertFalse(store.restore(self.root/'missing'))
        self.assertTrue(store.save(self.source))
        target = self.root/'restored'
        self.assertTrue(store.restore(target))
        self.assertEqual((target/'relative-link').read_bytes(), b'known input\n')
        self.assertNotEqual((target/'blob').stat().st_ino, (store.entry/'payload/blob').stat().st_ino)
        (target/'blob').write_bytes(b'private consumer changed its copy')
        self.assertEqual((store.entry/'payload/blob').read_bytes(), b'known input\n')
        self.assertFalse(cache.InputCache(self.root, 'native-fixture', 'pinned-v2').restore(self.root/'changed-key'))
        with self.assertRaises(ValueError):
            store.restore(target)

    def test_parallel_writers_publish_one_complete_entry(self):
        context = multiprocessing.get_context('fork')
        results = context.Queue()
        processes = [context.Process(target=save_in_process, args=(str(self.root), str(self.source), results)) for _ in range(3)]
        for process in processes:
            process.start()
            self.addCleanup(lambda p=process: p.kill() if p.is_alive() else None)
        for process in processes:
            process.join(timeout=10)
            self.assertEqual(process.exitcode, 0)
        self.assertEqual([results.get(timeout=2) for _ in processes], ['ok']*3)
        target = self.root/'parallel-restore'
        self.assertTrue(cache.InputCache(self.root, 'native-fixture', 'locked-v1').restore(target))
        self.assertEqual((target/'blob').read_bytes(), b'known input\n')
        self.assertFalse(list((self.root/'inputs').glob('.pending-*')))

    def test_changed_identity_and_redirected_paths_are_rejected(self):
        store = cache.InputCache(self.root, 'native-fixture', 'pinned-v1')
        store.save(self.source)
        (store.entry/'identity.json').write_text(json.dumps({'key': 'different'}))
        with self.assertRaises(ValueError):
            store.restore(self.root/'bad')
        (self.root/'redirect').symlink_to(self.source, target_is_directory=True)
        with self.assertRaises(ValueError):
            cache.directory(self.root/'redirect')
        other = cache.InputCache(self.root, 'native-fixture', 'pinned-v2')
        other.lock.symlink_to(self.source/'blob')
        with self.assertRaises(OSError):
            other.save(self.source)
        self.assertEqual((self.source/'blob').read_bytes(), b'known input\n')

    def test_dependency_overrides_and_build_directories_are_preserved(self):
        values = cache.setup(self.root, {'PIP_CACHE_DIR': '/explicit/pip', 'GRADLE_USER_HOME': '/explicit/gradle',
                                        'CARGO_HOME': '/explicit/cargo', 'RUSTUP_HOME': '/explicit/rustup',
                                        'CARGO_TARGET_DIR': '/private/target'})
        for name in ('PIP_CACHE_DIR', 'GRADLE_USER_HOME', 'CARGO_HOME', 'RUSTUP_HOME', 'CARGO_TARGET_DIR'):
            self.assertNotIn(name, values)
        self.assertEqual(values['UV_CACHE_DIR'], str(self.root/'uv'))

    def test_apt_key_ignores_job_path_but_distinguishes_sources_and_architecture(self):
        views = []
        for name in ('first-job', 'next-job'):
            view = self.root/name
            (view/'sources.list.d').mkdir(parents=True)
            (view/'sources.list').write_text('deb https://archive.ubuntu.com/ubuntu noble main\n')
            (view/'sources.list.d/official.sources').write_text('\n')
            views.append(view)
        first = cache.apt_cache(self.root, views[0], 'amd64')
        self.assertEqual(first, cache.apt_cache(self.root, views[1], 'amd64'))
        self.assertNotEqual(first, cache.apt_cache(self.root, views[1], 'arm64'))
        (views[1]/'sources.list').write_text('deb https://archive.ubuntu.com/ubuntu noble-updates main\n')
        self.assertNotEqual(first, cache.apt_cache(self.root, views[1], 'amd64'))

    def test_action_archive_hits_are_sha_addressed_and_tampering_fails(self):
        identity = dict(repository='actions/checkout', sha='a'*40)

        def download(argv, **kwargs):
            self.assertIn('https://codeload.github.com/actions/checkout/tar.gz/'+'a'*40, argv)
            Path(argv[argv.index('--output')+1]).write_bytes(b'known action archive')

        with patch.object(actions.subprocess, 'run', side_effect=download) as downloader:
            first = actions.prime(self.root, [identity])
            second = actions.prime(self.root, [identity])
            self.assertEqual(downloader.call_count, 1)
            self.assertFalse(first[0]['cache_hit'])
            self.assertTrue(second[0]['cache_hit'])
            self.assertEqual(second[0]['downloaded_bytes'], 0)
        archive = self.root/'actions_checkout'/('a'*40+'.tar.gz')
        archive.write_bytes(b'tampered')
        with self.assertRaises(ValueError):
            actions.prime(self.root, [identity])

    def test_runner_configuration_is_idempotent_and_preserves_existing_values(self):
        environment = self.root/'.env'
        environment.write_text('EXISTING_VALUE=preserved\nPIP_CACHE_DIR=/explicit/cache\n')
        environment.chmod(0o600)
        defaults = dict(PIP_CACHE_DIR='/new/pip', UV_CACHE_DIR='/new/uv')
        added = actions.configure(self.root, Path('/new/actions'), defaults)
        self.assertNotIn('PIP_CACHE_DIR', added)
        self.assertEqual(environment.read_text().splitlines(), [
            'EXISTING_VALUE=preserved', 'PIP_CACHE_DIR=/explicit/cache',
            'ACTIONS_RUNNER_ACTION_ARCHIVE_CACHE=/new/actions', 'UV_CACHE_DIR=/new/uv'])
        self.assertEqual(actions.configure(self.root, Path('/new/actions'), defaults), {})
        self.assertEqual(environment.stat().st_mode & 0o777, 0o600)


if __name__ == '__main__':
    unittest.main()
