"""Real Git/LFS checkout regressions; run on hosted CI, never against GitHub."""

import configparser
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import io
import json
import os
from pathlib import Path
import subprocess
import tempfile
import threading
import unittest


ROOT = Path(__file__).resolve().parents[3]
PRODUCTION_ENDPOINT = 'https://github.com/bin9208/openpilot-rust.git/info/lfs'
TARGET_REF = 'refs/remotes/rust/dev-md'
MODEL = b'\x08\x01synthetic-lfs-model-fixture\x00' * 40
OID = hashlib.sha256(MODEL).hexdigest()
POINTER = f'version https://git-lfs.github.com/spec/v1\noid sha256:{OID}\nsize {len(MODEL)}\n'


@unittest.skipUnless(
  os.environ.get('GITHUB_ACTIONS') == 'true' and os.environ.get('RUNNER_ENVIRONMENT') == 'github-hosted',
  'Real Git/LFS regressions run only on GitHub-hosted Actions',
)
class LfsEndpointTests(unittest.TestCase):
  def setUp(self):
    self.temporary = tempfile.TemporaryDirectory(prefix='lfs-endpoint-')
    self.addCleanup(self.temporary.cleanup)
    self.repo = Path(self.temporary.name) / 'checkout'
    self.repo.mkdir()
    # In particular, discard the enclosing workflow's SKIP_SMUDGE and Git config.
    self.env = {key: value for key, value in os.environ.items() if not key.startswith('GIT_')}
    self.env.update({
      'GIT_CONFIG_NOSYSTEM': '1',
      'GIT_CONFIG_GLOBAL': os.devnull,
      'GIT_TERMINAL_PROMPT': '0',
      'GIT_LFS_SKIP_SMUDGE': '0',
      'GIT_LFS_SKIP_DOWNLOAD_ERRORS': '0',
      'LC_ALL': 'C',
      'NO_PROXY': '127.0.0.1,localhost',
    })
    self.requests = []
    requests = self.requests

    class Handler(BaseHTTPRequestHandler):
      def log_message(self, *_args):
        pass

      def respond(self, status, body, content_type):
        self.send_response(status)
        self.send_header('Content-Type', content_type)
        self.send_header('Content-Length', str(len(body)))
        self.end_headers()
        self.wfile.write(body)

      def do_POST(self):
        request = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        requests.append(('POST', self.path, request))
        if request.get('operation') != 'download':
          self.respond(400, b'downloads only', 'text/plain')
          return
        if self.path not in ('/original.git/info/lfs/objects/batch', '/rust.git/info/lfs/objects/batch'):
          self.respond(404, b'unknown endpoint', 'text/plain')
          return
        objects = []
        for obj in request['objects']:
          result = {'oid': obj['oid'], 'size': obj['size']}
          if self.path.startswith('/rust.git/') and obj == {'oid': OID, 'size': len(MODEL)}:
            result['authenticated'] = True
            result['actions'] = {
              'download': {'href': f'http://127.0.0.1:{self.server.server_port}/objects/{OID}'},
            }
          else:
            result['error'] = {'code': 404, 'message': 'Object does not exist on the server'}
          objects.append(result)
        body = json.dumps({'transfer': 'basic', 'objects': objects}).encode()
        self.respond(200, body, 'application/vnd.git-lfs+json')

      def do_GET(self):
        requests.append(('GET', self.path, None))
        if self.path == f'/objects/{OID}':
          self.respond(200, MODEL, 'application/octet-stream')
        else:
          self.respond(404, b'unknown object', 'text/plain')

    self.server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
    self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
    self.thread.start()
    self.addCleanup(self.stop_server)
    self.base = f'http://127.0.0.1:{self.server.server_port}'
    self.endpoint = self.base + '/rust.git/info/lfs'
    self.wrong_endpoint = self.base + '/original.git/info/lfs'

    self.git('init', '--initial-branch=original', '--template=')
    # Missing git-lfs is a CI failure, not a skipped regression.
    self.git('lfs', 'version')
    self.git('lfs', 'install', '--local', '--skip-repo')
    for key, value in {
      'user.name': 'LFS fixture',
      'user.email': 'lfs@example.invalid',
      'commit.gpgsign': 'false',
      'lfs.transfer.maxretries': '1',
      'lfs.fetchrecentalways': 'false',
      'lfs.remote.autodetect': 'false',
      'lfs.remote.searchall': 'false',
      'lfs.skipdownloaderrors': 'false',
    }.items():
      self.git('config', '--local', key, value)
    self.git('remote', 'add', 'origin', self.base + '/original.git')
    self.git('remote', 'add', 'rust', self.base + '/rust.git')
    self.git('config', 'branch.original.remote', 'origin')
    self.git('config', 'branch.original.merge', 'refs/heads/original')
    (self.repo / '.gitattributes').write_text('*.onnx filter=lfs diff=lfs merge=lfs -text\n')
    self.git('add', '.gitattributes')
    self.git('commit', '-m', 'Original branch without model')
    self.original = self.git('rev-parse', 'HEAD').stdout.strip()
    self.object_path = self.repo / '.git/lfs/objects' / OID[:2] / OID[2:4] / OID

  def stop_server(self):
    self.server.shutdown()
    self.server.server_close()
    self.thread.join(timeout=5)

  def git(self, *args, check=True):
    result = subprocess.run(
      ['git', *args], cwd=self.repo, env=self.env, capture_output=True, text=True, timeout=30,
    )
    if check:
      self.assertEqual(result.returncode, 0, f'git {args}\n{result.stdout}\n{result.stderr}')
    return result

  def prepare_target(self, with_pin):
    self.git('checkout', '-b', 'candidate')
    if with_pin:
      config = configparser.ConfigParser()
      config.read_string((ROOT / '.lfsconfig').read_text())
      self.assertEqual(config['lfs']['url'], PRODUCTION_ENDPOINT)
      # Exercise the shipped configuration with only its service address
      # translated to loopback; all Git/LFS filtering and transfers stay real.
      config['lfs']['url'] = self.endpoint
      output = io.StringIO()
      config.write(output)
      (self.repo / '.lfsconfig').write_text(output.getvalue())
      self.git('add', '.lfsconfig')
    (self.repo / 'model.onnx').write_text(POINTER)
    self.git('add', 'model.onnx')
    self.git('commit', '-m', 'Independent candidate model')
    self.target = self.git('rev-parse', 'HEAD').stdout.strip()
    self.git('update-ref', TARGET_REF, self.target)
    self.git('checkout', 'original')
    self.assertFalse(self.object_path.exists(), 'Fixture must start with an empty object cache')
    self.assertEqual(self.requests, [], 'Creating a pointer must not download model bytes')

  def assert_materialized(self):
    data = (self.repo / 'model.onnx').read_bytes()
    self.assertEqual(data, MODEL)
    self.assertEqual(hashlib.sha256(data).hexdigest(), OID)
    self.assertEqual(self.object_path.read_bytes(), MODEL)
    self.assertEqual(self.git('rev-parse', 'HEAD').stdout.strip(), self.target)
    self.assertEqual(self.git('branch', '--show-current').stdout.strip(), 'original')
    self.assertEqual(self.git('config', 'branch.original.remote').stdout.strip(), 'origin')
    self.assertEqual(self.git('remote', 'get-url', 'origin').stdout.strip(), self.base + '/original.git')
    self.assertEqual(self.git('config', 'filter.lfs.required').stdout.strip(), 'true')
    for key in ('filter.lfs.process', 'filter.lfs.smudge'):
      self.assertNotIn('--skip', self.git('config', key).stdout)
    self.assertEqual(self.git('config', 'lfs.skipdownloaderrors').stdout.strip(), 'false')

  def assert_wrong_origin_reset(self):
    failed = self.git('reset', '--hard', TARGET_REF, check=False)
    self.assertNotEqual(failed.returncode, 0, failed.stdout + failed.stderr)
    self.assertIn('404', failed.stderr)
    self.assertFalse(self.object_path.exists())
    self.assertTrue(self.requests)
    self.assertTrue(all(path == '/original.git/info/lfs/objects/batch' for _, path, _ in self.requests))
    self.git('reset', '--hard', self.original)
    self.assertEqual(self.git('rev-parse', 'HEAD').stdout.strip(), self.original)

  def recover_with_forced_endpoint(self):
    # Deliberately retain the stale local URL: -c must override it without
    # permanently redirecting this original clone or changing its upstream.
    self.git('config', '--local', 'lfs.url', self.wrong_endpoint)
    failed = self.git('lfs', 'fetch', '--include=', '--exclude=', 'rust', TARGET_REF, check=False)
    self.assertNotEqual(failed.returncode, 0, 'A named remote alone must not bypass lfs.url')
    self.assertFalse(self.object_path.exists())
    self.requests.clear()
    self.git(
      '-c', f'lfs.url={self.endpoint}', '-c', 'lfs.fetchrecentalways=false',
      '-c', 'lfs.skipdownloaderrors=false', '-c', 'lfs.remote.autodetect=false',
      '-c', 'lfs.remote.searchall=false', 'lfs', 'fetch', '--include=', '--exclude=', 'rust', TARGET_REF,
    )
    self.assertEqual(self.git('rev-parse', 'HEAD').stdout.strip(), self.original)
    self.assertFalse((self.repo / 'model.onnx').exists(), 'Prefetch must not change the original worktree')
    self.assertEqual(self.object_path.read_bytes(), MODEL)
    self.assertEqual(
      [path for _, path, _ in self.requests], ['/rust.git/info/lfs/objects/batch', f'/objects/{OID}'],
    )
    self.requests.clear()
    self.git(
      '-c', f'lfs.url={self.endpoint}', '-c', 'lfs.skipdownloaderrors=false', 'reset', '--hard', TARGET_REF,
    )
    self.git('lfs', 'checkout')
    self.assert_materialized()
    self.assertEqual(self.requests, [], 'Reset must use the prefetched cache')
    self.assertEqual(self.git('config', '--local', 'lfs.url').stdout.strip(), self.wrong_endpoint)

  def test_shipped_pin_resolves_model_despite_original_branch_upstream(self):
    self.prepare_target(with_pin=True)
    self.git('reset', '--hard', TARGET_REF)
    self.assert_materialized()
    self.assertEqual(
      [path for _, path, _ in self.requests], ['/rust.git/info/lfs/objects/batch', f'/objects/{OID}'],
    )
    self.assertEqual(self.git('config', '--local', '--get', 'lfs.url', check=False).returncode, 1)

  def test_unpinned_original_origin_404_recovers_with_one_time_fetch(self):
    self.prepare_target(with_pin=False)
    self.assert_wrong_origin_reset()
    self.recover_with_forced_endpoint()

  def test_local_lfs_url_overrides_tracked_pin_but_force_flag_recovers(self):
    self.prepare_target(with_pin=True)
    self.git('config', '--local', 'lfs.url', self.wrong_endpoint)
    self.assert_wrong_origin_reset()
    self.recover_with_forced_endpoint()


if __name__ == '__main__':
  unittest.main()
