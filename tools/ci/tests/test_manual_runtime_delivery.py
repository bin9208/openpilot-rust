"""Cloud-only tests of owner approval, exact CI evidence and source separation."""
import copy
import importlib.util
import os
import hashlib
import io
import json
import tarfile
import tempfile
from pathlib import Path
import unittest
from unittest import mock

import yaml

ROOT = Path(__file__).resolve().parents[3]
SPEC = importlib.util.spec_from_file_location('manual_delivery', ROOT / 'rust/ci/runtime/manual_delivery.py')
DRIVER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(DRIVER)
SHA = 'a' * 40
REF = 'refs/heads/codex/fix-486-exact-runtime-delivery'


def request():
  return dict(state='open', user=dict(login='bin9208'), body='\n'.join((
    f'source_sha: {SHA}', 'ci_run_id: 42', f'controller_ref: {REF}', 'release_authorized: true')))


def evidence():
  run = dict(id=42, run_attempt=1, head_sha=SHA, head_branch='dev-md', event='push',
             path='.github/workflows/ci.yml', repository=dict(full_name=DRIVER.REPOSITORY),
             status='completed', conclusion='success')
  jobs = [dict(name=name, head_sha=SHA, status='completed', conclusion='success') for name in DRIVER.GATES]
  return run, copy.deepcopy(run), jobs


class ManualDeliveryTests(unittest.TestCase):
  def test_provider_receipt_uses_checked_out_source_and_keeps_controller_separate(self):
    controller = 'b' * 40
    with mock.patch.dict(os.environ, GITHUB_SHA=controller, RUNTIME_CONTROLLER_SHA='previous'), \
         mock.patch.object(DRIVER.subprocess, 'check_output', return_value=SHA + '\n'):
      with DRIVER.source_build_context(ROOT, SHA):
        # The original native OpenCV producer reads GITHUB_SHA verbatim.
        self.assertEqual(os.environ['GITHUB_SHA'], SHA)
        self.assertEqual(os.environ['RUNTIME_CONTROLLER_SHA'], controller)
      self.assertEqual(os.environ['GITHUB_SHA'], controller)
      self.assertEqual(os.environ['RUNTIME_CONTROLLER_SHA'], 'previous')

  def test_source_identity_context_restores_after_failure_and_rejects_other_checkout(self):
    controller = 'b' * 40
    with mock.patch.dict(os.environ, GITHUB_SHA=controller), \
         mock.patch.object(DRIVER.subprocess, 'check_output', return_value=SHA + '\n'):
      os.environ.pop('RUNTIME_CONTROLLER_SHA', None)
      with self.assertRaisesRegex(RuntimeError, 'producer failed'):
        with DRIVER.source_build_context(ROOT, SHA):
          raise RuntimeError('producer failed')
      self.assertEqual(os.environ['GITHUB_SHA'], controller)
      self.assertNotIn('RUNTIME_CONTROLLER_SHA', os.environ)
      with self.assertRaisesRegex(AssertionError, 'build source changed'):
        with DRIVER.source_build_context(ROOT, 'c' * 40):
          self.fail('different source was admitted')
      self.assertEqual(os.environ['GITHUB_SHA'], controller)

  def test_safe_restore_keeps_sealed_modes_and_archive_bytes(self):
    with tempfile.TemporaryDirectory() as folder:
      root = Path(folder) / 'source'
      root.mkdir()
      release = Path(folder) / 'release'
      release.mkdir()
      archive = release / f'openpilot-rust-{SHA}-aarch64.tar.gz'
      with tarfile.open(archive, 'w:gz') as payload:
        for name, mode in (('library', 0o664), ('executable', 0o775), ('readonly', 0o444)):
          member = tarfile.TarInfo(name)
          member.mode, member.size = mode, 3
          payload.addfile(member, io.BytesIO(b'elf'))
      original = archive.read_bytes()
      descriptor = dict(source_commit=SHA, archive=dict(name=archive.name, sha256=hashlib.sha256(original).hexdigest()))
      (release / 'runtime-release.json').write_text(json.dumps(descriptor))
      with tarfile.open(archive) as payload:
        payload.extractall(root, filter='data')
      self.assertEqual((root / 'library').stat().st_mode & 0o777, 0o644)
      DRIVER.restore_sealed_modes(root, release, SHA)
      for name, mode in (('library', 0o664), ('executable', 0o775), ('readonly', 0o444)):
        self.assertEqual((root / name).stat().st_mode & 0o777, mode)
      self.assertEqual(archive.read_bytes(), original)
      descriptor['archive']['sha256'] = '0' * 64
      (release / 'runtime-release.json').write_text(json.dumps(descriptor))
      with self.assertRaises(AssertionError):
        DRIVER.restore_sealed_modes(root, release, SHA)

  def test_explicit_owner_request_for_exact_source_is_admitted(self):
    DRIVER.validate_request(request(), SHA, '42', REF, 'bin9208')

  def test_missing_wrong_revoked_and_duplicate_approval_are_rejected(self):
    for fault in ('source', 'run', 'ref', 'actor', 'author', 'closed', 'pr', 'missing', 'duplicate', 'unapproved'):
      item = request()
      args = [SHA, '42', REF, 'bin9208']
      if fault == 'source': args[0] = 'b' * 40
      elif fault == 'run': args[1] = '43'
      elif fault == 'ref': args[2] = 'refs/heads/dev-md'
      elif fault == 'actor': args[3] = 'someone'
      elif fault == 'author': item['user']['login'] = 'someone'
      elif fault == 'closed': item['state'] = 'closed'
      elif fault == 'pr': item['pull_request'] = {}
      elif fault == 'missing': item['body'] = ''
      elif fault == 'duplicate': item['body'] += '\nrelease_authorized: true'
      elif fault == 'unapproved': item['body'] = item['body'].replace('true', 'false')
      with self.subTest(fault=fault), self.assertRaises(AssertionError):
        DRIVER.validate_request(item, *args)

  def test_completed_latest_push_and_all_real_gates_are_required(self):
    DRIVER.validate_ci(*evidence(), SHA, '42')
    for fault in ('sha', 'branch', 'pr', 'repository', 'running', 'failed', 'cancelled',
                  'workflow', 'superseded', 'attempt', 'missing', 'duplicate', 'job-source', 'job-skipped'):
      run, latest, jobs = evidence()
      if fault == 'sha': run['head_sha'] = 'b' * 40
      elif fault == 'branch': run['head_branch'] = 'dev'
      elif fault == 'pr': run['event'] = 'pull_request'
      elif fault == 'repository': run['repository']['full_name'] = 'someone/else'
      elif fault == 'running': run['status'] = 'in_progress'
      elif fault in ('failed', 'cancelled'): run['conclusion'] = fault
      elif fault == 'workflow': run['path'] = '.github/workflows/rust.yml'
      elif fault == 'superseded': latest['id'] = 43
      elif fault == 'attempt': latest['run_attempt'] = 2
      elif fault == 'missing': jobs.pop()
      elif fault == 'duplicate': jobs.append(copy.deepcopy(jobs[0]))
      elif fault == 'job-source': jobs[0]['head_sha'] = 'b' * 40
      elif fault == 'job-skipped': jobs[0]['conclusion'] = 'skipped'
      with self.subTest(fault=fault), self.assertRaises(AssertionError):
        DRIVER.validate_ci(run, latest, jobs, SHA, '42')

  def test_pinned_controller_stays_outside_immutable_source_checkout(self):
    workflow = yaml.load((ROOT / '.github/workflows/dev-md-runtime.yml').read_text(), Loader=yaml.BaseLoader)
    for name in ('package', 'publish'):
      steps = workflow['jobs'][name]['steps']
      checkouts = [(i, step) for i, step in enumerate(steps) if step.get('uses', '').startswith('actions/checkout@')]
      self.assertEqual(len(checkouts), 1)
      self.assertEqual(checkouts[0][1]['with']['ref'], '${{ needs.verify.outputs.source_sha }}')
      self.assertNotIn('if', checkouts[0][1])
      self.assertEqual(steps[checkouts[0][0] + 1]['uses'], './.github/actions/job-temp')
      index, retrieve = next((i, step) for i, step in enumerate(steps)
                             if step.get('name') == 'Retrieve pinned manual controller outside the source tree')
      self.assertGreater(index, checkouts[0][0] + 1)
      self.assertEqual(retrieve['if'], "inputs.release_request != ''")
      self.assertIn('?ref=$GITHUB_SHA', retrieve['run'])
      self.assertIn('"$RUNNER_TEMP/manual-delivery.py"', retrieve['run'])
      self.assertNotIn('git commit', retrieve['run'])
    source = (ROOT / 'rust/ci/runtime/manual_delivery.py').read_text()
    self.assertIn('runpy.run_path(str(root / "rust/ci/runtime/package.py"))', source)
    self.assertIn('delivery["build"]()', source)
    self.assertIn('delivery["restore_release"]()', source)
    self.assertIn('delivery["install"](True)', source)
    self.assertIn('identity["publisher_sha256"]', source)
    self.assertIn('identity["installer_sha256"]', source)


if __name__ == '__main__':
  unittest.main()
