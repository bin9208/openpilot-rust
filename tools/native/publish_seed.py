"""Publish only a successful native ARM release-seed export in manual Actions."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import sys


def module(name):
  spec = importlib.util.spec_from_file_location('seed_publication_'+name, Path(__file__).with_name(name+'.py'))
  value = importlib.util.module_from_spec(spec)
  spec.loader.exec_module(value)
  return value


publication, seed = module('publish_inputs'), module('bootstrap_seed')
PREFIX = 'source-entry-full/state/release-seed/'
BUILD_STEP = 'Check legacy and current kernel memory interfaces on native ARM'


class SeedGitHub(publication.GitHub):
  def json(self, method, route, value=None, missing=False):
    if method == 'POST' and route == 'releases':
      # Source-control preparation pins the tested commit with a lightweight
      # tag. Do not ask the Actions token to create a workflow-changing ref.
      ref = super().json('GET', 'git/ref/tags/'+value['tag_name'])
      if ref['object']['type'] != 'commit' or ref['object']['sha'] != value['target_commitish']:
        raise ValueError('seed release tag is not the already verified source')
      value = {key: item for key, item in value.items() if key != 'target_commitish'}
    return super().json(method, route, value, missing)


def validate(bundle, source, target):
  proof = bundle.json('source-entry-full/result.json')
  if (proof['kind'] != 'release-seed-export' or proof['source']['commit'] != source
      or proof['workflow_source'] != proof['source'] or proof['cleanup_verified'] is not True
      or proof['native_arm_execution'] is not True or proof['emulated'] is not False
      or proof['source_entry_full_build_executed'] is not False or proof['runtime_activated'] is not False
      or proof['manager_started'] is not False):
    raise ValueError('seed export lacks successful native ARM preparation-only evidence')
  filenames = {'manifest.json', 'manifest.sha256', *seed.FILES.values()}
  if {name.removeprefix(PREFIX) for name in bundle.members if name.startswith(PREFIX)} != filenames:
    raise ValueError('seed export inventory differs')
  target.mkdir()
  for name in filenames:
    item = bundle.member(PREFIX+name)
    if item.file_size > 64*1024**2:
      raise ValueError('seed export member exceeds its bound')
    with bundle.zip.open(item) as incoming, (target/name).open('xb') as outgoing:
      shutil.copyfileobj(incoming, outgoing, 1024**2)
  manifest = target/'manifest.json'
  pin = seed.digest(manifest)
  plan, _ = seed.read_plan(manifest, pin)
  if (plan['source'] != proof['source'] or pin != proof['exported']['descriptor_sha256']
      or (target/'manifest.sha256').read_text() != pin+'\n'
      or plan['protocols'] != dict(current_tools=1, request_owner=2, generation_boot=1)):
    raise ValueError('seed export source, descriptor or protocols differ')
  rows = []
  tag = 'native-bootstrap-'+source
  for role, name in seed.FILES.items():
    asset = plan['assets'][role]
    if (publication.asset(asset) != (tag, name) or (target/name).stat().st_size != asset['bytes']
        or seed.digest(target/name) != asset['sha256']):
      raise ValueError('seed export asset differs from its independent pin')
    rows.append(dict(asset, tag=tag, name=name, path=str(target/name)))
  artifact = proof['release_artifact']
  if (artifact['sha256'] != plan['assets']['helper']['sha256'] or artifact['bytes'] != plan['assets']['helper']['bytes']
      or not artifact['executable'].endswith('/release/openpilot-runtime-inputs')):
    raise ValueError('seed export is not the verified release helper')
  seed.elf(target/seed.FILES['helper'])  # Inspect bytes; never execute the ARM helper here.
  for name in ('manifest.json', 'manifest.sha256'):
    rows.append(dict(url=publication.BASE+tag+'/'+name, sha256=seed.digest(target/name),
        bytes=(target/name).stat().st_size, tag=tag, name=name, path=str(target/name)))
  return plan, pin, rows


def main():
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument('--source-run-id', type=int, required=True)
  parser.add_argument('--source-commit', required=True)
  parser.add_argument('--index-sha256', required=True)
  parser.add_argument('--publish', action='store_true')
  args = parser.parse_args()
  if (not publication.runner_policy.environment_allowed(os.environ)
      or os.environ.get('GITHUB_REPOSITORY') != publication.REPO
      or os.environ.get('GITHUB_EVENT_NAME') != 'workflow_dispatch'
      or not (sys.flags.isolated and sys.flags.no_site and sys.flags.dont_write_bytecode)
      or not publication.positive(args.source_run_id) or not publication.checksum(args.source_commit, 40)
      or not publication.checksum(args.index_sha256)):
    raise ValueError('seed publication requires isolated manual Actions and exact source pins')
  job = Path(os.environ['CI_JOB_TEMP'])
  identity = ':'.join(os.environ[k] for k in ('GITHUB_RUN_ID', 'GITHUB_RUN_ATTEMPT', 'GITHUB_JOB'))
  if (job.parent != Path('/tmp') or not job.name.startswith('openpilot-ci.') or job.resolve() != job
      or job.stat().st_uid != os.geteuid() or (job/'owner').read_text() != identity):
    raise ValueError('seed publication work is not owned by this job')
  root = job/'seed-publication'
  root.mkdir()
  client = SeedGitHub(root)
  run = client.json('GET', f'actions/runs/{args.source_run_id}')
  publication.trusted_run(run, args.source_commit)
  if (run['path'] != '.github/workflows/native-source-entry.yml' or run['event'] != 'workflow_dispatch'
      or run['status'] != 'completed' or run['conclusion'] != 'success'):
    raise ValueError('seed publication requires a successful source-entry export run')
  jobs = client.collect(f'actions/runs/{run["id"]}/attempts/{run["run_attempt"]}/jobs', 'jobs')
  if not any(j['conclusion'] == 'success' and any(s['name'] == BUILD_STEP and s['conclusion'] == 'success'
      for s in j['steps']) for j in jobs):
    raise ValueError('seed export lacks the native kernel-interface cases')
  checks = client.collect('actions/runs?head_sha='+args.source_commit, 'workflow_runs')
  passed = []
  for check in checks:
    if check['path'] != '.github/workflows/runtime-source-build.yml' or check['conclusion'] != 'success':
      continue
    publication.trusted_run(check, args.source_commit)
    names = {j['name'] for j in client.collect(f'actions/runs/{check["id"]}/jobs', 'jobs') if j['conclusion'] == 'success'}
    if all(f'Installed Git checkout startup ({runner})' in names for runner in ('ubuntu-24.04', 'ubuntu-24.04-arm')):
      passed.append(check['id'])
  if not passed:
    raise ValueError('seed publication requires exact-source ARM/X64 startup checks')
  artifacts = client.collect(f'actions/runs/{run["id"]}/artifacts', 'artifacts')
  artifact, = [a for a in artifacts if a['name'] == f'native-source-entry-{args.source_commit}-{run["run_attempt"]}' and not a['expired']]
  pin = publication.artifact_pin(artifact, run, args.source_commit, 128*1024**2)
  publication.room(root, 3*pin['bytes']+128*1024**2)
  archive = root/'seed.zip'
  client.download(f'actions/artifacts/{artifact["id"]}/zip', pin, archive)
  bundle = publication.Bundle(archive)
  try:
    if bundle.json('source-entry-full/result.json')['index_sha256'] != args.index_sha256:
      raise ValueError('seed export used a different input index')
    plan, descriptor, rows = validate(bundle, args.source_commit, root/'verified')
  finally:
    bundle.close()
  index = client.source_json(args.source_commit, 'rust/native-bootstrap-inputs.json')
  if hashlib.sha256(seed.json_bytes(index)).hexdigest() != args.index_sha256:
    raise ValueError('seed publication input index differs from its source pin')
  index['seed'] = next({k: row[k] for k in ('url', 'sha256', 'bytes')} for row in rows if row['name'] == 'manifest.json')
  installed = root/'installed-source'
  (installed/'rust').mkdir(parents=True)
  (installed/'rust/native-bootstrap-inputs.json').write_bytes(seed.json_bytes(index))
  metadata = installed/'native-inputs/metadata'
  metadata.mkdir(parents=True)
  shutil.copyfile(root/'verified/manifest.json', metadata/(descriptor+'.json'))
  assets = installed/'native-inputs/seeds'/descriptor
  assets.mkdir(parents=True)
  for name in seed.FILES.values():
    shutil.copyfile(root/'verified'/name, assets/name)
  states = [dict(ready=True, seed_export_run=run['id'], startup_check_runs=passed)]
  report = dict(schema=1, kind='release-seed-publication', source=plan['source'], artifact_id=artifact['id'],
      artifact_sha256=pin['sha256'], descriptor_sha256=descriptor, seed_assets=plan['assets'],
      candidate_index_sha256=seed.digest(installed/'rust/native-bootstrap-inputs.json'),
      gates=states, publish_requested=args.publish, full_runtime_published=False, helper_executed=False)
  def persist():
    (root/'report.json').write_bytes(seed.json_bytes(report))
  persist()
  if args.publish:
    publication.publish(client, rows, args.source_commit, descriptor, states, report, persist)
  print(json.dumps(report, indent=2))


if __name__ == '__main__':
  main()
