import importlib.util
import json
import multiprocessing
from pathlib import Path
import tempfile
import unittest

MODULE_PATH = Path(__file__).resolve().parents[1]/'runner_download_cache.py'
spec = importlib.util.spec_from_file_location('runner_download_cache', MODULE_PATH)
cache = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cache)


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


if __name__ == '__main__':
    unittest.main()
