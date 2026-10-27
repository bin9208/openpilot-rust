#!/usr/bin/env python3
"""Admit current-Git model build inputs without smudging the source checkout."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile


def main():
  assert os.environ['RUNNER_NAME'] == 'bin-actions' and os.environ['RUNNER_ENVIRONMENT'] == 'self-hosted'
  source, work = Path(os.environ['GITHUB_WORKSPACE']).resolve(), Path(os.environ['CI_JOB_TEMP'])
  assert (work/'owner').read_text() == ':'.join(os.environ[k] for k in ('GITHUB_RUN_ID','GITHUB_RUN_ATTEMPT','GITHUB_JOB'))
  sys.path.insert(0,str(source/'rust/tools'))
  from native_model_generation import model_inputs
  from native_model_inputs import source_identity
  origin = source_identity(source)
  assert origin['commit'] == os.environ['GITHUB_SHA']
  models = model_inputs(source,origin['commit'])
  plan = dict(schema=1,reserve_bytes=65536,assets={'model-'+k:
      {n:p[n] for n in ('url','sha256','bytes')} for k,p in models.items() if p['lfs']})
  helper = Path((work/'native-capnp-inputs-cli.txt').read_text().strip())
  rows=[]
  with tempfile.TemporaryDirectory(prefix='model-assets-',dir=work) as temp:
    path=Path(temp)/'plan.json'
    path.write_text(json.dumps(plan))
    for phase in ('initial','reused'):
      output=work/f'native-model-assets-{phase}.json'
      with output.open('xb') as stream:
        subprocess.run([helper,'--prepare-assets','--plan',path,'--cache',work/'native-model-assets'],stdout=stream,check=True)
      row=json.loads(output.read_text())
      assert set(row)==set(plan['assets'])
      for name,r in row.items():
        assert r['sha256']==plan['assets'][name]['sha256'] and r['bytes']==plan['assets'][name]['bytes']
        if phase=='reused': assert r['transferred_bytes']==r['stored_bytes']==0
      rows.append(row)
  assert source_identity(source)==origin
  (work/'native-model-assets-summary.json').write_text(json.dumps(dict(schema=1,source=origin,
      models=models,repeated_http_body_bytes=0,model_generated=False,source_checkout_mutated=False),indent=2)+'\n')


if __name__=='__main__':
  main()
