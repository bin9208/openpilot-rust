#!/usr/bin/env python3
"""Regenerate the original DBC reference in an owned Git-only Python package.

This oracle runs only in Actions, never in runtime startup or warm preparation.
It does not copy ignored generated files, bytecode or the user's Python package.
"""
import argparse
import hashlib
import json
import os
import runpy
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile


def main():
  parser = argparse.ArgumentParser()
  for name in ('source', 'provider', 'work', 'output'):
    parser.add_argument('--'+name, type=Path, required=True)
  args = parser.parse_args()
  runpy.run_path(str(Path(__file__).resolve().parents[1]/'runner_policy.py'))['require_environment'](self_hosted_only=False)
  source = args.source.resolve()

  def git(*arguments):
    return subprocess.check_output(['/usr/bin/git', '-C', source, *arguments])

  sha = lambda data: hashlib.sha256(data).hexdigest()
  commit = git('rev-parse', 'HEAD').decode().strip()
  contract = json.loads((source/'rust/native-dbc-source-contract.json').read_bytes())
  listing = git('ls-tree', '-r', '-z', 'HEAD', '--', 'opendbc_repo/opendbc/dbc', 'opendbc_repo/opendbc/__init__.py')
  blobs = {}
  for entry in listing.split(b'\0'):
    if not entry:
      continue
    metadata, path = entry.decode().split('\t')
    mode, kind, oid = metadata.split(' ')
    assert mode in ('100644', '100755') and kind == 'blob'
    data = git('cat-file', 'blob', oid)
    assert len(data) <= 8*1024**2
    blobs[path.removeprefix('opendbc_repo/')] = data
  growth = sum(len(data) for data in blobs.values())*4+64*1024**2
  free = shutil.disk_usage(args.work).free
  assert free >= growth, (free, growth)
  original = json.loads((args.provider/'rust/bin/radarcan-dbc/radarcan-assets.json').read_bytes())
  expected_inputs = {name.removeprefix('opendbc/dbc/'): sha(data) for name, data in blobs.items()
                     if name.startswith('opendbc/dbc/')}
  assert expected_inputs == original['source_sha256'] and len(expected_inputs) == 140
  assert len(original['asset_sha256']) == 103
  with tempfile.TemporaryDirectory(prefix='dbc-original-', dir=args.work) as temporary:
    owned = Path(temporary)
    package = owned/'package'
    for name, data in blobs.items():
      target = package/name
      target.parent.mkdir(parents=True, exist_ok=True)
      target.write_bytes(data)
    (owned/'tmp').mkdir()
    launcher = """import json,sys
from pathlib import Path
sys.path.insert(0,sys.argv[1])
from opendbc.dbc.generator.generator import generate_all
source=Path(sys.argv[1])/'opendbc/dbc'
generated=generate_all()
data={p.name:p.read_bytes().hex() for p in source.glob('*.dbc') if p.stem not in generated}
data.update({name+'.dbc':text.encode('utf-8').hex() for name,text in generated.items()})
Path(sys.argv[2]).write_text(json.dumps(data))
"""
    environment = dict(PATH=os.path.dirname(sys.executable)+':/usr/bin:/bin', HOME=str(owned),
        PYTHONPATH=str(package), PYTHONNOUSERSITE='1', PYTHONDONTWRITEBYTECODE='1',
        PYTHONPYCACHEPREFIX=str(owned/'pycache'), TMPDIR=str(owned/'tmp'), LC_ALL='C.UTF-8')
    result = owned/'result.json'
    subprocess.run([sys.executable, '-I', '-B', '-S', '-c', launcher, str(package), str(result)],
        cwd=owned, env=environment, check=True)
    data = {name: bytes.fromhex(value) for name, value in json.loads(result.read_bytes()).items()}
    assert {name: sha(value) for name, value in data.items()} == original['asset_sha256']
    for name, value in data.items():
      path = 'rust/bin/radarcan-dbc/'+name
      assert sha(value) == contract['outputs'][path] == sha((args.provider/path).read_bytes())
  assert git('rev-parse', 'HEAD').decode().strip() == commit
  assert not git('status', '--porcelain', '--untracked-files=all').strip()
  args.output.write_text(json.dumps(dict(schema=1, source_commit=commit,
      source_inputs_checked=len(expected_inputs), original_databases_regenerated_for_oracle=len(data),
      output_bytes=sum(map(len, data.values())), exact_byte_comparison=True,
      python=sys.version, runtime_data_regenerated=False, physical_device_execution=False), indent=2)+'\n')


if __name__ == '__main__':
  main()
