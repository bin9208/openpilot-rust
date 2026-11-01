"""Actual pinned ARM libraries: reject ownerless leaves, verify the real owner."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import runpy
import subprocess
import sys

from companion_elf_scope import audit_requests, mpc_audit_requests, verify_report


def main():
  parser = argparse.ArgumentParser()
  for name in ('auditor', 'probe', 'mpc-binary', 'batch', 'work'):
    parser.add_argument('--'+name, type=Path, required=True)
  args = parser.parse_args()
  # job-temp exposes rust/target through a symlink; ELF reports use the real
  # binary path. Bind the expected root to that same owned artifact up front.
  args.mpc_binary = args.mpc_binary.resolve(strict=True)
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
  def reject_ownerless(prefix, libraries, name):
    search = [argument for path in libraries for argument in ('--library', path)]
    unscoped = args.work/(name+'.json')
    execute([args.auditor, '--root', prefix/'libblasfeo.so',
        '--root', prefix/'libhpipm.so', *search, '--output', unscoped], name, expected=1)
    negative = json.loads(unscoped.read_bytes())
    assert negative['compatible'] is False and len(negative['roots']) == 2
    for row in negative['roots']:
      assert row['compatible'] is False and row['failures']
      for failure in row['failures']:
        assert failure['missing_versions'] == {}
        assert set(failure['missing_strong_symbols']) == {'sqrt', 'sqrtf'}

  reject_ownerless(roots['acados'], libraries, 'ownerless')
  requests = audit_requests(args.auditor, args.probe, elf_roots, roots['acados'], libraries, args.work)
  for request in requests:
    execute(request['argv'], request['name'])
    verify_report(request, json.loads(request['output'].read_bytes()))
  mpc_plan = json.loads((source/'rust/native-mpc-provider.json').read_bytes())
  mpc_root = Path(batch['exports'][mpc_plan['profile']]['origin']['root'])
  mpc_prefix = mpc_root/'plannerd-acados'
  mpc_libraries = [mpc_prefix, '/usr/lib/aarch64-linux-gnu', '/lib/aarch64-linux-gnu']
  execute([sys.executable, source/'tools/ci/fixtures/native-mpc-abi.py',
      '--provider', mpc_prefix, '--rust-binary', args.mpc_binary, '--output', args.work/'mpc-abi'],
      'mpc-api', env=dict(os.environ, LD_LIBRARY_PATH=':'.join(map(str, mpc_libraries))))
  comparison = json.loads((args.work/'mpc-api.stdout').read_bytes())
  assert comparison['bit_exact_solves'] == 12 and comparison['successful_statuses'] and comparison['finite_outputs']
  mpc_maps = {row[5] for line in (args.work/'mpc-abi/provider-maps.txt').read_text().splitlines()
      if len(row:=line.split(maxsplit=5)) == 6}
  for name in ('libacados.so', 'libacados_ocp_solver_lat.so', 'libacados_ocp_solver_long.so',
      'libblasfeo.so', 'libhpipm.so', 'libqpOASES_e.so.3.1'):
    assert {str(mpc_prefix/name), str((mpc_prefix/name).resolve())} & mpc_maps
  reject_ownerless(mpc_prefix, mpc_libraries, 'mpc-ownerless')
  mpc_roots = [mpc_root/path for path in mpc_plan['members'] if '.so' in Path(path).name]
  mpc_requests = mpc_audit_requests(args.auditor, args.mpc_binary, mpc_roots,
      mpc_prefix, mpc_libraries, args.work)
  for request in mpc_requests:
    execute(request['argv'], 'mpc-'+request['name'])
    verify_report(request, json.loads(request['output'].read_bytes()))
  result = dict(source_commit=os.environ['GITHUB_SHA'], foreign_api=actual,
      ownerless_missing_math_rejected=True, original_roots=len(elf_roots)+1,
      scopes=[dict(name=row['name'], roots=len(row['roots']),
          load_owner=None if row['load_owner'] is None else str(row['load_owner'])) for row in requests],
      mpc=dict(direct_c_abi_comparison=comparison, ownerless_missing_math_rejected=True,
          original_roots=len(mpc_roots)+1,
          scopes=[dict(name=row['name'], roots=len(row['roots']),
              load_owner=None if row['load_owner'] is None else str(row['load_owner'])) for row in mpc_requests]),
      commands=commands)
  (args.work/'result.json').write_text(json.dumps(result, indent=2)+'\n')
  print(json.dumps(result))


if __name__ == '__main__':
  main()
