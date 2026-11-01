import copy
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from companion_elf_scope import audit_requests, verify_report


class CompanionScopeTests(unittest.TestCase):
  def setUp(self):
    self.acados = Path('/work/acados')
    self.roots = [self.acados / name for name in (
        'libacados.so', 'libblasfeo.so', 'libhpipm.so', 'libqpOASES_e.so', 'libqpOASES_e.so.3.1')]
    self.roots += [Path('/work/numerics/openblas.so'), Path('/work/geos/libgeos.so'),
                   Path('/work/brotli/libblasfeo.so')]
    self.requests = audit_requests('/work/audit', '/work/probe', self.roots,
        self.acados, ['/work/libs'], '/work')

  def report(self, request):
    value = dict(compatible=True, roots=[dict(root=str(path), compatible=True, failures=[])
        for path in request['roots']])
    if request['load_owner'] is not None:
      value['load_owner'] = str(request['load_owner'])
    return value

  def test_every_original_root_remains_and_only_the_two_actual_dependencies_share_acados(self):
    independent, loaded = self.requests
    self.assertEqual(set(independent['roots']) | set(loaded['roots']),
        {Path('/work/probe'), *self.roots})
    self.assertEqual(set(independent['roots']) & set(loaded['roots']), {self.acados / 'libacados.so'})
    self.assertNotIn('--load-owner', independent['argv'])
    self.assertIn(Path('/work/brotli/libblasfeo.so'), independent['roots'])
    self.assertEqual(loaded['load_owner'], self.acados / 'libacados.so')
    self.assertEqual(set(loaded['roots'][1:]),
        {self.acados / 'libblasfeo.so', self.acados / 'libhpipm.so'})
    for request in self.requests:
      verify_report(request, self.report(request))

  def test_incomplete_or_duplicate_original_inventory_is_rejected(self):
    for roots in (self.roots[1:], self.roots + [self.roots[0]]):
      with self.assertRaises(ValueError):
        audit_requests('/work/audit', '/work/probe', roots, self.acados, [], '/work')

  def test_missing_replaced_or_duplicate_reported_root_cannot_pass(self):
    request = self.requests[0]
    for change in ('missing', 'replaced', 'duplicate'):
      report = self.report(request)
      if change == 'missing':
        report['roots'].pop()
      elif change == 'replaced':
        report['roots'][-1]['root'] = '/unrelated/other.so'
      else:
        report['roots'][-1] = copy.deepcopy(report['roots'][0])
      with self.subTest(change=change), self.assertRaises(AssertionError):
        verify_report(request, report)

  def test_wrong_or_unexpected_load_owner_cannot_pass(self):
    for request in self.requests:
      report = self.report(request)
      report['load_owner'] = '/unrelated/owner'
      with self.assertRaises(AssertionError):
        verify_report(request, report)

  def test_compatible_top_level_never_hides_a_failed_root(self):
    request = self.requests[1]
    report = self.report(request)
    report['roots'][-1]['failures'] = [{'missing_strong_symbols': ['unknown_symbol']}]
    with self.assertRaises(AssertionError):
      verify_report(request, report)
