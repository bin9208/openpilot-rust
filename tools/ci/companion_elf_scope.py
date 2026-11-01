"""Keep independent companion audits and the actual acados dependency scope."""
from pathlib import Path


def audit_requests(auditor, probe, elf_roots, acados, libraries, work):
  roots = [Path(probe), *map(Path, elf_roots)]
  owner = Path(acados) / 'libacados.so'
  dependencies = [Path(acados) / name for name in ('libblasfeo.so', 'libhpipm.so')]
  if len(set(roots)) != len(roots) or not {owner, *dependencies}.issubset(roots):
    raise ValueError('all distinct companion roots and the actual acados owner are required')
  independent = [root for root in roots if root not in dependencies]
  search = [argument for path in libraries for argument in ('--library', path)]
  requests = []
  for name, selected, load_owner in (
      ('independent', independent, None), ('acados', dependencies, owner)):
    output = Path(work) / f'native-companion-elf-{name}.json'
    argv = [auditor]
    if load_owner is not None:
      argv += ['--load-owner', load_owner]
    argv += [argument for root in selected for argument in ('--root', root)]
    argv += [*search, '--output', output]
    requests.append(dict(name=name, argv=argv, output=output, load_owner=load_owner,
        roots=selected if load_owner is None else [load_owner, *selected]))
  return requests


def verify_report(request, report, translate=str):
  expected = [translate(path) for path in request['roots']]
  observed = [row['root'] for row in report['roots']]
  if len(observed) != len(expected) or set(observed) != set(expected):
    raise AssertionError('companion ELF audit omitted or replaced an original root')
  owner = request['load_owner']
  if report.get('load_owner') != (None if owner is None else translate(owner)):
    raise AssertionError('companion ELF audit has the wrong load-owner scope')
  if report['compatible'] is not True or any(
      row['compatible'] is not True or row['failures'] for row in report['roots']):
    raise AssertionError('companion ELF root is incompatible')
