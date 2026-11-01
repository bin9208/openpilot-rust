"""Actual pinned ARM libraries: reject ownerless leaves, verify the real owner."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import runpy
import subprocess

from companion_elf_scope import audit_requests, verify_report


def main():
  parser = argparse.ArgumentParser()
  for name in ('auditor', 'probe', 'batch', 'work'):
    parser.add_argument('--'+name, type=Path, required=True)
  args = parser.parse_args()
  source = Path(os.environ['GITHUB_WORKSPACE'])
  runpy.run_path(str(source/'tools/ci/runner_policy.py'))['require_github_native_arm']()
  assert args.work.resolve().is_relative_to(Path(os.environ['CI_JOB_TEMP']).resolve())
  args.work.mkdir()
  assert args.batch.resolve() == Path(os.environ['NATIVE_PROVIDER_BATCH']).resolve()
  assert hashlib.sha256(args.batch.read_bytes()).hexdigest() == os.environ['NATIVE_PROVIDER_BATCH_SHA256']
  batch = json.loads(args.batch.read_bytes())
  leaves = dict(numerics='numerics', geos='carrot-geos', brotli='brotli', acados='plannerd-acados')
  roots, elf_roots = {}, []
  for name, leaf in leaves.items():
    plan = json.loads((source/f'rust/native-{name}-provider.json').read_bytes())
    prefix = Path(batch['exports'][plan['profile']]['origin']['root'])
    roots[name] = prefix/leaf
    elf_roots += [prefix/path for path, member in plan['members'].items()
        if '.so' in Path(path).name and member['link'] is None]
  libraries = [*roots.values(), '/usr/lib/aarch64-linux-gnu', '/lib/aarch64-linux-gnu']
  commands = []

  def execute(argv, name, expected=0, env=None):
    argv = list(map(str, argv))
    with (args.work/(name+'.stdout')).open('w') as out, (args.work/(name+'.stderr')).open('w') as err:
      result = subprocess.run(argv, stdout=out, stderr=err, env=env, check=False)
    commands.append(dict(argv=argv, exit_code=result.returncode, expected_exit_code=expected))
    assert result.returncode == expected, (name, result.returncode)

  maps = args.work/'provider-maps.txt'
  execute([args.probe, *(roots[name] for name in leaves), maps], 'foreign-api',
      env=dict(os.environ, LD_LIBRARY_PATH=':'.join(map(str, libraries))))
  actual = json.loads((args.work/'foreign-api.stdout').read_bytes())
  assert actual == dict(numerics_dot_matrix_svd=True, geos_interpolation=True,
      brotli_roundtrip=True, acados_core_symbols=True, mpc_solver_tested=False)
  mapped = {row[5] for line in maps.read_text().splitlines() if len(row:=line.split(maxsplit=5)) == 6}
  assert all(str(roots['acados']/name) in mapped for name in (
      'libacados.so', 'libblasfeo.so', 'libhpipm.so', 'libqpOASES_e.so.3.1'))
  search = [argument for path in libraries for argument in ('--library', path)]
  unscoped = args.work/'ownerless.json'
  execute([args.auditor, '--root', roots['acados']/'libblasfeo.so',
      '--root', roots['acados']/'libhpipm.so', *search, '--output', unscoped], 'ownerless', expected=1)
  negative = json.loads(unscoped.read_bytes())
  assert negative['compatible'] is False and len(negative['roots']) == 2
  for row in negative['roots']:
    assert row['compatible'] is False and row['failures']
    for failure in row['failures']:
      assert failure['missing_versions'] == {}
      assert set(failure['missing_strong_symbols']) == {'sqrt', 'sqrtf'}
  requests = audit_requests(args.auditor, args.probe, elf_roots, roots['acados'], libraries, args.work)
  for request in requests:
    execute(request['argv'], request['name'])
    verify_report(request, json.loads(request['output'].read_bytes()))
  result = dict(source_commit=os.environ['GITHUB_SHA'], foreign_api=actual,
      ownerless_missing_math_rejected=True, original_roots=len(elf_roots)+1,
      scopes=[dict(name=row['name'], roots=len(row['roots']),
          load_owner=None if row['load_owner'] is None else str(row['load_owner'])) for row in requests],
      commands=commands)
  (args.work/'result.json').write_text(json.dumps(result, indent=2)+'\n')
  print(json.dumps(result))


if __name__ == '__main__':
  main()
