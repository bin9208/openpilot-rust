#!/usr/bin/env python3
"""Run the production input pipeline on a job-owned real host-kernel cgroup.

The original genuine probe runs first and supplies independently exported small
components and authenticated raw caches. No original command or input pin is
replaced. This is ARM userspace emulation, not a physical ARM or AGNOS kernel.
"""
import copy
import json
import os
import runpy
from pathlib import Path
import shutil
import subprocess
from urllib.parse import urlsplit

from verify_native_bootstrap import digest


# This runs in the original AGNOS user space with the factory isolated Python.
# Only metadata is seeded offline. Native preparation below still runs the real
# source-built helper and its private host-kernel cgroup, without remapping sudo
# or claiming that this exercises the full source_entry build/boot command.
RESOLVE_INDEX = r'''
import importlib.util, json, os, sys, tempfile
from pathlib import Path
assert sys.flags.isolated and sys.flags.no_site and sys.flags.dont_write_bytecode
assert os.geteuid() != 0 and not os.getgroups()
source, policy, expected, offline = sys.argv[1:]
source, policy, offline = Path(source), Path(policy), Path(offline)
with tempfile.TemporaryDirectory(prefix='native-index-bytecode-', dir='/tmp') as bytecode:
  sys.pycache_prefix = bytecode
  spec = importlib.util.spec_from_file_location('native_index_entry', source/'tools/native/source_entry.py')
  entry = importlib.util.module_from_spec(spec)
  spec.loader.exec_module(entry)
  entry.seed.native()
  machine, pin = entry.machine_configuration(policy, expected)
  entry.validate_machine(machine, source)
  before = entry.runtime.source_identity(source)
  owner = entry.seed.owner_chain(os.environ['OPENPILOT_NATIVE_REQUEST_OWNER'])
  commands = entry.runtime.OwnedCommands(owner, Path(machine['stores']['params']))
  def observe():
    commands.observe()
    entry.machine_configuration(policy, pin)
    if entry.runtime.source_identity(source) != before:
      raise ValueError('owned input index source changed during resolution')
  with entry.runtime.operation(Path(machine['stores']['generations'])) as attempt:
    result = entry.resolve(machine, source, attempt, commands, observe, offline)
    result['source'] = before
    result['machine_configuration_sha256'] = pin
    entry.seed.write(attempt/'resolved.json', entry.seed.json_bytes(result))
  print(json.dumps(result))
'''


def main():
  assert os.geteuid() != 0 and not os.getgroups()
  runpy.run_path(str(Path(__file__).resolve().parent/'runner_policy.py'))['require_environment'](self_hosted_only=True)
  job = Path(os.environ['CI_JOB_TEMP'])
  assert job.parent == Path('/tmp') and job.name.startswith('openpilot-ci.') and job.resolve() == job
  owner = ':'.join(os.environ[k] for k in ('GITHUB_RUN_ID', 'GITHUB_RUN_ATTEMPT', 'GITHUB_JOB'))
  assert (job/'owner').read_text() == owner
  work = job/'native-agnos'
  context = json.loads((work/'native-bootstrap-context.json').read_text())
  assert context['schema'] == 1 and context['source_commit'] == os.environ['GITHUB_SHA']
  assert context['work'] == str(work)
  guest_work = Path(context['guest_work'])
  guest = context['guest']
  assert guest[0] == str(work/'proot') and '-0' not in guest
  assert digest(work/'proot') == context['proot_sha256']
  source = Path(os.environ['GITHUB_WORKSPACE'])
  evidence = work/'runtime-inputs-evidence'
  evidence.mkdir()
  cache = work/'bootstrap-current-cache'
  configuration = copy.deepcopy(json.loads((work/'bootstrap-current-config.json').read_text()))
  current = json.loads((work/'bootstrap-current-evidence/initial.json').read_text())
  fields = Path('/proc/self/stat').read_text().rsplit(') ', 1)[1].split()
  request_owner = f'{os.getpid()}:{int(fields[19])}'
  # Add aliases only within this job's already owned cache. Independent input
  # types keep separate prefixes even where a raw authenticated blob is shared.
  aliases = {
      'rust-assets': job/'native-rust-assets', 'cargo-assets': job/'native-cargo-assets',
      'python-assets': job/'native-python-assets', 'capnp-assets': job/'native-capnp-assets',
      'raylib-sdk-assets': job/'native-raylib-assets', 'ffmpeg-headers-assets': job/'native-ffmpeg-source',
      'opencv-headers-assets': job/'native-opencv-source', 'model-wheel-assets': job/'native-model-wheel-assets',
      'model-assets': job/'native-model-assets', 'source-lfs-assets': job/'native-source-lfs-assets',
      'usbgpu-model-assets': job/'native-usbgpu-model-assets',
  }
  binds = []
  for name, host in aliases.items():
    assert host.is_dir()
    (cache/name).mkdir(exist_ok=True)
    binds += ['-b', f'{host}:{guest_work}/bootstrap-current-cache/{name}']
  # libyuv and Eigen were originally prepared from two separate caches. Seed
  # only their authenticated immutable blobs into a new cache, never lock files.
  codecs = cache/'codecs-assets/sha256'
  codecs.mkdir(parents=True)
  for path, key in ((work/'native-codec-initial.json', 'libyuv'), (work/'native-eigen-initial.json', 'eigen')):
    receipt = json.loads(path.read_text())[key]['asset']
    raw_root = job/('native-codec-assets' if key == 'libyuv' else 'native-eigen-assets')
    host = raw_root/'sha256'/receipt['sha256']/'blob'
    assert host.stat().st_size == receipt['bytes'] and digest(host) == receipt['sha256']
    destination = codecs/receipt['sha256']
    destination.mkdir()
    os.link(host, destination/'blob', follow_symlinks=False)
  guest = [guest[0], '-b', '/sys/fs/cgroup', *binds, *guest[1:],
      f'OPENPILOT_NATIVE_REQUEST_OWNER={request_owner}']

  def execute(arguments, name):
    command = [*guest, *(str(arg).replace(str(work), str(guest_work)) for arg in arguments)]
    output = evidence/name
    with output.open('xb') as stdout, (evidence/(name+'.stderr')).open('xb') as stderr:
      result = subprocess.run(command, stdout=stdout, stderr=stderr, check=False)
    if result.returncode:
      raise RuntimeError(f'production runtime input command failed ({result.returncode}): {name}; retained {output}')
    return output

  providers = {}
  metadata_assets = {}
  publication_files = []
  def metadata_asset(path, url):
    value = dict(url=url, sha256=digest(path), bytes=path.stat().st_size)
    metadata_assets[value['sha256']] = path
    publication_files.append(dict(**value, path=str(path)))
    return value
  payload = 0
  roles = ('ffmpeg', 'raylib', 'opencv', 'numerics', 'geos', 'brotli', 'acados',
           'sdl', 'turbojpeg', 'ffmpeg-tools', 'freesans', 'panda', 'mpc', 'dbc', 'usbgpu')
  for role in roles:
    directory = ('provider-export' if role == 'ffmpeg' else role+'-provider-export'
                 if role in ('raylib', 'opencv') else role+'-export'
                 if role in ('panda', 'mpc', 'dbc', 'usbgpu') else 'companion-exports/'+role)
    # The original guest binds map these exact exported component directories.
    host_directory = (job/'native-provider-export' if role == 'ffmpeg'
        else job/('native-'+role+'-provider-export') if role in ('raylib', 'opencv')
        else job/('native-'+role+'-export') if role in ('panda', 'mpc', 'dbc', 'usbgpu')
        else job/'native-companion-exports'/role)
    descriptor = json.loads((host_directory/'provider.json').read_text())
    plan = json.loads((source/f'rust/native-{role}-provider.json').read_text())
    assert descriptor['profile'] == plan['profile'] and descriptor['provider_source_commit'] == plan['source_commit']
    archive_name = Path(urlsplit(descriptor['asset']['url']).path).name
    assert (host_directory/archive_name).is_file()
    descriptor_url = descriptor['asset']['url'].rsplit('/', 1)[0]+'/provider.json'
    metadata_asset(host_directory/'provider.json', descriptor_url)
    publication_files.append(dict(**descriptor['asset'], path=str(host_directory/archive_name)))
    providers[role] = dict(path=str(guest_work/directory/'provider.json'),
        sha256=digest(host_directory/'provider.json'), offline_archive=str(guest_work/directory/archive_name))
    payload += sum(p['bytes'] for p in plan['members'].values())
  fixture = work/'runtime-inputs-source'
  git = ['/usr/bin/git', '-c', 'user.name=Owned production input fixture', '-c', 'user.email=ci@example.invalid',
         '-c', 'commit.gpgsign=false', '-c', 'core.hooksPath=/dev/null']
  # Include two retained source views, provider prefix/composition metadata,
  # native model/compiler work and a separate release compiler cache. This is
  # an admission estimate for this CI operation, never a device budget.
  generated = sum(p.stat().st_size for p in (work/'native-model-catalogs').rglob('*') if p.is_file())
  growth = 8*1024**3+2*payload+2*generated+configuration['expected_source_growth_bytes']
  assert shutil.disk_usage(work).free >= growth
  configuration['resources']['filesystems'] = {str(guest_work): dict(maximum_growth_bytes=growth, minimum_free_bytes=64*1024**2)}
  configuration['resources']['memory_cgroup_parent'] = os.environ['NATIVE_TEST_CGROUP_PARENT']
  execute(['/usr/bin/env', 'GIT_LFS_SKIP_SMUDGE=1', *git, 'clone', '--shared', '--no-checkout',
      context['source_mount'], fixture], 'source-clone.txt')
  execute([*git, '-C', fixture, 'remote', 'set-url', 'origin', 'https://github.com/bin9208/openpilot-rust.git'], 'source-origin.txt')
  execute(['/usr/bin/env', 'GIT_LFS_SKIP_SMUDGE=1', *git, '-C', fixture, 'checkout', '--detach', context['source_commit']], 'source-checkout.txt')
  budgets = work/'native-project-growth-estimates.json'
  assert budgets.is_file()
  selection = dict(schema=1, providers=providers, compiler_growth_bytes=1024**3,
      budgets=dict(path=str(guest_work/budgets.name), sha256=digest(budgets)))
  selected_path = work/'production-input-selection.json'
  selected_path.write_text(json.dumps(selection)+'\n')
  configuration['runtime_inputs'] = dict(path=str(guest_work/selected_path.name), sha256=digest(selected_path))
  config_path = work/'production-bootstrap-config.json'
  config_path.write_text(json.dumps(configuration)+'\n')
  configuration_sha256 = digest(config_path)
  helper = Path(current['tools'])/'openpilot-runtime-inputs'
  command = [helper, '--bootstrap-tools', '--config', config_path, '--configuration-sha256', configuration_sha256, '--root', fixture]
  composition_roots = {}
  metrics = {}
  def prepare(arguments, phase):
    row = json.loads(execute(arguments, phase+'.json').read_text())
    prepared = row['runtime_inputs']
    assert prepared['runtime_inputs_verified'] and prepared['provider_roles'] == 15 and prepared['runtime_roles'] == 18
    assert prepared['compiler_input_roles'] == 9
    assert not prepared['full_runtime_build'] and not prepared['runtime_activated'] and not prepared['runtime_release_downloaded']
    folder = work/Path(prepared['native_configuration']).relative_to(guest_work).parent
    operations = json.loads((folder/'operations.json').read_text())
    assert operations['catalog']['compiler_inputs_bound'] and operations['catalog']['groups'] == 13
    assert operations['catalog']['binaries'] == 99 and not operations['catalog']['compiled']
    assert all(not operations['provider-'+role]['origin_archive_downloaded']
        and operations['provider-'+role]['asset']['transferred_bytes'] == 0 for role in roles)
    models = operations['models']
    assert models['transferred_bytes'] == 0 and not models['gpu_executed']
    if phase not in ('initial', 'git-index-initial'):
      assert all(a['fresh'] for a in operations['model-compiler-build']['build']['artifacts'])
      assert all(value['qcom_compiler_calls'] == value['generated_pipelines'] == 0 for value in models['kinds'].values())
      assert all(operations['provider-'+role]['copied_bytes'] == operations['provider-'+role]['asset']['transferred_bytes'] == 0 for role in roles)
      assert all(operations[role+'-sdk']['copied_bytes'] == operations[role+'-sdk']['linked_bytes'] == 0 for role in ('ffmpeg', 'opencv'))
      assert all(v == 0 for v in models['environment_preparation'].values())
    composition = json.loads((folder/'composition.json').read_text())
    baseline = 'git-index' if phase.startswith('git-index-') else 'direct-config'
    if baseline not in composition_roots:
      composition_roots[baseline] = composition['root']
    else:
      assert composition['root'] == composition_roots[baseline] and composition['copied_bytes'] == composition['linked_bytes'] == 0
    samples = [json.loads(line) for line in (folder.parent/'resources.jsonl').read_text().splitlines()]
    assert samples[-1]['state'] == 'success' and samples[-1]['memory']['kernel_peak_bytes'] > 0
    metrics[phase] = samples[-1]
    assert (work/'bootstrap-current-generations/active.json').read_bytes() == b'owned previous selection sentinel\n'
    assert (work/'bootstrap-current-generations/previous-log').read_bytes() == b'owned previous log sentinel\n'
    return prepared

  for phase in ('initial', 'reused', 'docs'):
    if phase == 'docs':
      path = fixture/'README.md'
      path.write_bytes(path.read_bytes()+b'\nOwned production runtime input docs fixture.\n')
      execute([*git, '-C', fixture, 'add', 'README.md'], 'docs-add.txt')
      execute([*git, '-C', fixture, 'commit', '-m', 'Owned production runtime input docs fixture'], 'docs-commit.txt')
    prepare(command, phase)
  # Derive the exact input candidate from the original verified exports. A
  # publication manifest is emitted only after the added resolver cases pass.
  seed_manifest_path = work/'bootstrap-seed-export/manifest.json'
  seed_manifest = json.loads(seed_manifest_path.read_text())
  assert seed_manifest['source']['commit'] == context['source_commit']
  release = 'https://github.com/bin9208/openpilot-rust/releases/download/'
  seed_pin = metadata_asset(seed_manifest_path, release+'native-bootstrap-'+seed_manifest['source']['commit']+'/manifest.json')
  for role, asset in seed_manifest['assets'].items():
    path = seed_manifest_path.parent/Path(urlsplit(asset['url']).path).name
    assert digest(path) == asset['sha256'] and path.stat().st_size == asset['bytes']
    publication_files.append(dict(**asset, path=str(path)))
  sdk_path = work/'native-system-sdk.json'
  sdk_pin = metadata_asset(sdk_path, release+'native-system-sdk-'+digest(sdk_path)+'/system-sdk.json')
  candidate = dict(schema=1, seed=seed_pin, system_sdk=sdk_pin, providers={})
  for role in roles:
    row = providers[role]
    # Resolve the exact host descriptor already admitted through the original
    # guest binds above, without inferring new hashes from the old runtime.
    path = metadata_assets[row['sha256']]
    descriptor = json.loads(path.read_text())
    candidate['providers'][role] = dict(url=descriptor['asset']['url'].rsplit('/', 1)[0]+'/provider.json',
        sha256=row['sha256'], bytes=path.stat().st_size)
  candidate_path = evidence/'native-bootstrap-inputs.candidate.json'
  candidate_path.write_text(json.dumps(candidate, sort_keys=True, indent=2)+'\n')
  # Exercise the production resolver with actual, independently hashed SDK and
  # provider descriptors. Commit the index only to this job-owned fixture, never
  # to the workflow source or to a public branch. Its source identity is distinct.
  metadata_growth = sum(path.stat().st_size+65536 for path in metadata_assets.values())
  # The indexed source and its documentation-only successor are independent
  # retained source views; the original operation's estimate did not include them.
  resolver_growth = metadata_growth+2*configuration['expected_source_growth_bytes']
  assert shutil.disk_usage(work).free >= resolver_growth+64*1024**2
  offline = work/'source-index-metadata'
  offline.mkdir()
  for checksum, path in metadata_assets.items():
    assert not path.is_symlink() and digest(path) == checksum
    os.link(path, offline/(checksum+'.json'), follow_symlinks=False)
  input_index = fixture/'rust/native-bootstrap-inputs.json'
  input_index.write_bytes(candidate_path.read_bytes())
  execute([*git, '-C', fixture, 'add', 'rust/native-bootstrap-inputs.json'], 'index-add.txt')
  execute([*git, '-C', fixture, 'commit', '-m', 'Owned actual input index fixture'], 'index-commit.txt')
  indexed_commit = execute([*git, '-C', fixture, 'rev-parse', 'HEAD'], 'index-source.txt').read_text().strip()
  assert indexed_commit != context['source_commit']
  machine = copy.deepcopy({key: value for key, value in configuration.items()
      if key not in ('system_sdk_receipt', 'system_sdk_sha256', 'runtime_inputs')})
  machine['resources']['filesystems'][str(guest_work)]['maximum_growth_bytes'] += resolver_growth
  machine.update(seed_cache=str(guest_work/'source-index-seeds'), updater_staging=str(guest_work/'source-index-updater'),
      budgets=json.loads(budgets.read_text()), compiler_growth_bytes=selection['compiler_growth_bytes'])
  machine_path = work/'source-index-machine.json'
  machine_path.write_text(json.dumps(machine)+'\n')
  machine_pin = digest(machine_path)
  resolved_rows = {}
  for phase in ('git-index-initial', 'git-index-reused', 'git-index-docs'):
    if phase == 'git-index-docs':
      path = fixture/'README.md'
      path.write_bytes(path.read_bytes()+b'\nOwned actual input index documentation-only fixture.\n')
      execute([*git, '-C', fixture, 'add', 'README.md'], 'index-docs-add.txt')
      execute([*git, '-C', fixture, 'commit', '-m', 'Owned actual input index docs fixture'], 'index-docs-commit.txt')
    requested = execute([*git, '-C', fixture, 'rev-parse', 'HEAD'], phase+'-source.txt').read_text().strip()
    resolved = json.loads(execute(['/usr/bin/python3', '-I', '-B', '-S', '-c', RESOLVE_INDEX,
        fixture, machine_path, machine_pin, offline], phase+'-resolved.json').read_text())
    assert resolved['source']['commit'] == requested
    assert (requested != indexed_commit) == (phase == 'git-index-docs')
    assert resolved['machine_configuration_sha256'] == machine_pin
    assert resolved['index_sha256'] == digest(candidate_path)
    receipts = resolved['receipts']
    assert set(receipts) == {'seed', 'system_sdk', *roles}
    for receipt in receipts.values():
      assert receipt['transferred_bytes'] == 0
      expected = receipt['bytes'] if phase == 'git-index-initial' else 0
      assert receipt['stored_bytes'] == receipt['offline_seed_bytes'] == expected
    prepared = prepare([helper, '--bootstrap-tools', '--config', resolved['configuration'],
        '--configuration-sha256', resolved['configuration_sha256'], '--root', fixture], phase)
    assert prepared['source'] == resolved['source']
    assert prepared['bootstrap_configuration_sha256'] == resolved['configuration_sha256']
    resolved_rows[phase] = resolved
  (evidence/'publication-candidate.json').write_text(json.dumps(dict(schema=1,
      workflow_source_commit=context['source_commit'], index_sha256=digest(candidate_path),
      source_run_id=int(os.environ['GITHUB_RUN_ID']), source_run_attempt=int(os.environ['GITHUB_RUN_ATTEMPT']),
      source_job=os.environ['GITHUB_JOB'],
      files=publication_files, published_release=False, committed_input_index=False,
      source_index_resolver_cases_passed=list(resolved_rows), physical_device_execution=False), indent=2)+'\n')
  (evidence/'summary.json').write_text(json.dumps(dict(schema=1, source_commit=context['source_commit'],
      cases=list(metrics), metrics=metrics, production_input_orchestrator_executed=True,
      provider_roles=15, runtime_roles=18, compiler_input_roles=9, actual_host_kernel_cgroup=True,
      process_architecture='aarch64', physical_arm_cpu=False, agnos_kernel=False,
      runtime_started=False, runtime_activated=False, public_component_download_tested=False,
      input_index_candidate_sha256=digest(candidate_path), input_index_committed=False, published_release=False,
      production_source_index_resolver_executed=True, owned_input_index_source_commit=indexed_commit,
      input_index_committed_to_workflow_source=False,
      input_index_cases=resolved_rows, source_entry_full_build_executed=False, source_entry_sudo_scope_exercised=False,
      resource_policy_is_ci_fixture_not_device_budget=True), indent=2)+'\n')


if __name__ == '__main__':
  main()
