#!/usr/bin/env python3
"""#599: execute a compiler feasibility probe in the genuine pinned AGNOS image.

Actions only. This does not boot an AGNOS kernel, flash a device or build the full
runtime. It checks native toolchain/ABI facts and the actual Rust encoder against
the unchanged source codec methods in native ARM AGNOS user space under PRoot.
"""
import argparse
import hashlib
import json
import lzma
import os
import platform
import re
import resource
import runpy
import shutil
import stat
import subprocess
import time
from pathlib import Path

RAW_BYTES = 4718592000
SPARSE_SHA = 'a67410a888105ecce7bcc05e17cda77fd10572aa4380e9de242a7c92f3497114'
RAW_SHA = '375c5d22335770ac08750660bb5b29a3331c550d6a9875b35f36b88af792ea44'
RUSTUP_SHA = '15f6e4ce9f583b929c996c91562bad6d4454f3281de858b02cdfdef615fac433'
NATIVE_PROOT = runpy.run_path(str(Path(__file__).with_name('native_proot.py')))


def digest(path):
  with Path(path).open('rb') as stream:
    return hashlib.file_digest(stream, 'sha256').hexdigest()


def room(path, growth):
  free = shutil.disk_usage(path).free
  print(json.dumps(dict(path=str(path), available_bytes=free, expected_additional_bytes=growth)), flush=True)
  if free < growth:
    raise RuntimeError('insufficient space for this operation; other data retained')


def run(argv, log, *, required=True, stdout_path=None, env=None):
  started = time.monotonic()
  with log.open('ab') as output:
    output.write((json.dumps(list(map(str, argv))) + '\n').encode())
    output.flush()
    if stdout_path is None:
      result = subprocess.run(list(map(str, argv)), stdout=output, stderr=subprocess.STDOUT, check=False, env=env)
    else:
      with stdout_path.open('wb') as stdout:
        result = subprocess.run(list(map(str, argv)), stdout=stdout, stderr=output, check=False, env=env)
  record = dict(argv=list(map(str, argv)), exit_code=result.returncode, elapsed_seconds=time.monotonic() - started)
  print(json.dumps(record), flush=True)
  if required and result.returncode:
    raise RuntimeError(f'probe command failed; see {log.name}: {record}')
  return record


def download(url, path, sha, log):
  run(['curl', '--proto', '=https', '--tlsv1.2', '--fail', '--location', '--retry', '5', '--retry-all-errors',
       '--connect-timeout', '20', '--max-time', '600', '--output', path, url], log)
  assert digest(path) == sha, 'bootstrap tool hash mismatch'
  path.chmod(0o700)


def native_driver_probe(guest_command, work, source_mount, evidence, log, guest_work):
  """Build the real repository driver, then exercise it on owned source only."""
  room(work, 2 * 1024**3)

  def execute(argv, name=None):
    output = work / name if name else None
    evidence['commands'].append(run(guest_command(argv), log, stdout_path=output))
    return output.read_text() if output else None

  actual_source = execute(['git', '-C', source_mount, 'rev-parse', 'HEAD'], 'native-project-source.txt').strip()
  assert actual_source == evidence['source_commit'], 'native project source differs from Actions SHA'
  execute(['cargo', 'build', '--locked', '--manifest-path', f'{source_mount}/rust/Cargo.toml',
           '-p', 'openpilot-runtime-build', '--bin', 'openpilot-runtime-build', '--bin', 'openpilot-runtime-plan',
           '--target-dir', work / 'project-target', '--message-format=json'], 'native-project-build.jsonl')
  driver = work / 'project-target/debug/openpilot-runtime-build'
  with driver.open('rb') as executable:
    header = executable.read(64)
  assert len(header) == 64 and header[:6] == b'\x7fELF\x02\x01' and int.from_bytes(header[18:20], 'little') == 183
  evidence['native_project_builder'] = dict(source_commit=actual_source, sha256=digest(driver),
                                          elf_architecture='AArch64', full_runtime_build=False)
  cargo = execute(['rustup', 'which', '--toolchain', '1.94.0', 'cargo'], 'native-project-cargo-path.txt').strip()
  planner = work / 'project-target/debug/openpilot-runtime-plan'
  catalog = json.loads(execute([planner, '--root', source_mount, '--cargo', cargo, '--check'],
                               'native-project-catalog.json'))
  assert catalog['source']['commit'] == actual_source
  assert catalog['groups'] == 13 and catalog['binaries'] == 99
  assert not catalog['compiled'] and not catalog['activated']
  assert catalog['compiler_inputs_bound'] is None
  assert len(catalog['compiler_inputs'])==13
  for name in ('07','08','10'):
    assert catalog['compiler_inputs'][name]['FFMPEG_DIR']==''
  for name in ('00','01','02'):
    assert catalog['compiler_inputs'][name]['STARTUP_UI_RAYLIB_ROOT']==''
  for requirements in catalog['compiler_inputs'].values():
    assert requirements['OPENPILOT_CARGO_VENDOR']=='vendor'
  evidence['native_project_catalog'] = dict(source_commit=actual_source, planner_sha256=digest(planner),
      catalog_sha256=catalog['catalog_sha256'], groups=catalog['groups'], binaries=catalog['binaries'],
      transitive_compiler_input_contract_audited=True,compiler_inputs_bound=False,
      full_runtime_build=False)
  execute(['/usr/bin/python3',f'{source_mount}/tools/ci/verify_native_compiler_features.py',
      '--cargo',cargo,'--root',source_mount,'--source-commit',actual_source,
      '--output',work/'native-compiler-features'],'native-project-compiler-features.json')
  feature_report=json.loads((work/'native-compiler-features/result.json').read_text())
  assert feature_report['status']=='PASS' and len(feature_report['groups'])==13
  evidence['native_project_catalog']['independent_cargo_tree_groups_matched']=13
  toolchain = str(Path(cargo).parent)
  fixture = work / 'driver-fixture'
  shutil.copytree(work / 'fixture/src', fixture / 'rust/src')
  for name in ('Cargo.toml', 'Cargo.lock'):
    shutil.copyfile(work / 'fixture' / name, fixture / 'rust' / name)
  git = ['git', '-c', 'user.name=Owned AGNOS driver fixture', '-c', 'user.email=agnos-driver@example.invalid', '-C', fixture]
  execute([*git, 'init'])

  def commit():
    execute([*git, 'add', '.'])
    execute([*git, 'commit', '-m', 'Owned AGNOS native driver input'])

  commit()
  recipe = work / 'driver-recipe.json'
  recipe.write_text(json.dumps(dict(schema=1, target='aarch64-unknown-linux-gnu',
                                   packages=['agnos-native-compiler-probe'], binaries=['first', 'second'],
                                   features=[], inputs={}, input_trees={},
                                   environment={'OPENPILOT_NATIVE_INCREMENTAL': '1'},
                                   expected_growth_bytes=32 * 1024**2)) + '\n')

  def build(phase):
    output = execute([driver, '--root', fixture, '--recipe', recipe, '--cache', work / 'driver-cache',
                      '--toolchain', toolchain], f'runtime-driver-{phase}.json')
    report = json.loads(output)
    artifacts = {Path(row['executable']).name: row for row in report['artifacts']}
    assert artifacts.keys() == {'first', 'second'} and report['native_tools']
    for name, row in artifacts.items():
      # Executable paths are guest paths and must execute there, not on the host.
      execute([row['executable']], f'runtime-driver-{phase}-{name}.stdout')
    return report, artifacts

  initial, first = build('initial')
  assert all(not row['fresh'] for row in first.values())
  repeated, repeat = build('repeated')
  assert repeated['source'] == initial['source'] and repeated['cache_key'] == initial['cache_key']
  assert all(row['fresh'] and row['sha256'] == first[name]['sha256'] for name, row in repeat.items())
  (fixture / 'README.md').write_text('Owned AGNOS docs-only change\n')
  commit()
  docs, doc_bins = build('docs-only')
  assert docs['source']['commit'] != initial['source']['commit'] and docs['cache_key'] == initial['cache_key']
  assert all(row['fresh'] and row['sha256'] == first[name]['sha256'] for name, row in doc_bins.items())
  first_source = fixture / 'rust/src/bin/first.rs'
  first_source.write_text('fn main(){println!("native AGNOS Rust std first 43");}\n')
  commit()
  small, changed = build('one-binary-change')
  assert small['cache_key'] == initial['cache_key'] and small['native_tools'] == initial['native_tools']
  assert not changed['first']['fresh'] and changed['first']['sha256'] != first['first']['sha256']
  assert changed['second']['fresh'] and changed['second']['sha256'] == first['second']['sha256']
  assert (work / 'runtime-driver-one-binary-change-first.stdout').read_text() == 'native AGNOS Rust std first 43\n'
  assert (work / 'runtime-driver-one-binary-change-second.stdout').read_text() == 'native AGNOS Rust std second 42\n'
  assert execute(['git', '-C', source_mount, 'rev-parse', 'HEAD'], 'native-project-source-after.txt').strip() == actual_source
  assert not execute(['git', '-C', source_mount, 'status', '--porcelain', '--untracked-files=all'], 'native-project-status-after.txt').strip()
  evidence['native_project_driver_cases'] = dict(cases=4, source_project_unchanged=True,
      repeated_and_docs_only_all_fresh=True, one_binary_change_unrelated_fresh=True,
      actual_guest_execution=True, full_generation_seal=False, device_execution=False)

  # Run the separately authenticated compiler/Cargo/std prefix inside AGNOS.
  # Host archive preparation and ELF admission alone cannot establish this.
  host_receipt = json.loads((work.parent / 'native-rust-first.json').read_text())
  host_store = work.parent / 'native-rust-toolchains'
  guest_store = Path(guest_work) / 'pinned-rust'
  pinned_root = guest_store / Path(host_receipt['root']).relative_to(host_store)
  pinned_receipt = guest_store / Path(host_receipt['tree_receipt']).relative_to(host_store)
  pinned = work / 'driver-pinned-rust-recipe.json'
  configuration = json.loads(recipe.read_text())
  configuration['inputs'] = {str(pinned_receipt): host_receipt['tree_sha256']}
  configuration['input_trees'] = {str(pinned_root): str(pinned_receipt)}
  pinned.write_text(json.dumps(configuration) + '\n')
  argv = [driver, '--root', fixture, '--recipe', pinned, '--cache', work / 'driver-cache',
          '--toolchain', pinned_root / 'bin']
  pinned_reports = []
  for phase in ('initial', 'reused'):
    result = json.loads(execute(argv, f'runtime-driver-pinned-rust-{phase}.json'))
    artifacts = {Path(row['executable']).name: row for row in result['artifacts']}
    assert artifacts.keys() == {'first', 'second'}
    assert all(row['fresh'] == (phase == 'reused') for row in artifacts.values())
    assert result['rustc_sha256'] == initial['rustc_sha256']
    assert result['cargo_sha256'] == initial['cargo_sha256']
    for name, row in artifacts.items():
      actual = execute([row['executable']], f'runtime-driver-pinned-rust-{phase}-{name}.stdout')
      assert actual == f'native AGNOS Rust std {name} {43 if name == "first" else 42}\n'
    pinned_reports.append(result)
  assert pinned_reports[0]['cache_key'] == pinned_reports[1]['cache_key']
  before = {Path(row['executable']).name: row['sha256'] for row in pinned_reports[0]['artifacts']}
  assert before == {Path(row['executable']).name: row['sha256'] for row in pinned_reports[1]['artifacts']}
  host_std = next((Path(host_receipt['root']) / 'lib/rustlib/aarch64-unknown-linux-gnu/lib').glob('libstd-*.rlib'))
  original_mode = host_std.stat().st_mode & 0o7777
  with host_std.open('rb') as stream:
    original_byte = stream.read(1)
  assert original_byte

  def mutate(value):
    host_std.chmod(0o644)
    try:
      with host_std.open('r+b') as stream:
        stream.write(value)
        stream.flush()
        os.fsync(stream.fileno())
    finally:
      host_std.chmod(original_mode)

  mutate(bytes([original_byte[0] ^ 1]))
  try:
    rejected = run(guest_command(argv), log, required=False, stdout_path=work / 'runtime-driver-pinned-rust-tamper.stdout')
    evidence['commands'].append(rejected)
    assert rejected['exit_code'] != 0, 'modified standard library was accepted'
    for row in pinned_reports[-1]['artifacts']:
      name = Path(row['executable']).name
      actual = execute([row['executable']], f'runtime-driver-pinned-rust-preserved-{name}.stdout')
      assert actual == f'native AGNOS Rust std {name} {43 if name == "first" else 42}\n'
  finally:
    mutate(original_byte)
  evidence['pinned_native_rust_cases'] = dict(cases=3, repeated_compiler_artifacts_fresh=True,
      standard_library_same_size_tamper_rejected=True, previous_executables_preserved=True,
      compiler_and_cargo_match_original_distribution=True, tree_sha256=host_receipt['tree_sha256'],
      actual_guest_execution=True, full_runtime_build=False, device_execution=False)


def native_cereal_probe(guest_command, work, source_mount, evidence, log, guest_work):
  """Use the admitted native C++ SDK against the actual complete schemas."""
  owned = work.parent
  # Run the source-built ARM preparer itself, using the authenticated raw cache.
  # The host-native tree remains an independent expected receipt.
  def execute(argv, name):
    evidence['commands'].append(run(guest_command(argv),log,stdout_path=work/name))
    return work/name
  build_native_inputs(guest_command, work, source_mount, evidence, log)
  preparer = work/'project-target/debug/openpilot-runtime-inputs'
  registry_prepare = [preparer,'--prepare-cargo','--plan',f'{source_mount}/rust/native-cargo-inputs.json',
                      '--lock',f'{source_mount}/rust/Cargo.lock','--cache',Path(guest_work)/'cargo-assets',
                      '--store',Path(guest_work)/'cargo-sources']
  registry = json.loads(execute(registry_prepare,'native-cereal-cargo-inputs-reuse.json').read_text())
  registry_repeat = json.loads(execute(registry_prepare,'native-cereal-cargo-inputs-repeated.json').read_text())
  host_registry = json.loads((owned/'native-cargo-first.json').read_text())
  assert registry['packages'] == registry_repeat['packages'] == host_registry['packages'] == 458
  assert registry['tree_sha256'] == registry_repeat['tree_sha256'] == host_registry['tree_sha256']
  assert all(report['copied_bytes'] == report['linked_bytes'] == 0 for report in (registry,registry_repeat))
  assert all(row['transferred_bytes'] == row['stored_bytes'] == 0 for report in (registry,registry_repeat) for row in report['assets'].values())
  evidence['native_cargo_inputs'] = dict(packages=458,tree_sha256=registry['tree_sha256'],
      actual_arm_preparer_sha256=digest(preparer),mode='verify and reuse the host-prepared seed in AGNOS user space',
      archive_body_bytes=0,copied_bytes=0,linked_bytes=0,compiler_executed_by_preparer=False,device_execution=False)
  prepare = [preparer,'--prepare-capnp','--plan',f'{source_mount}/rust/native-capnp-assets.json',
             '--cache',Path(guest_work)/'capnp-assets','--store',work/'native-capnp-prefixes']
  first = json.loads(execute(prepare,'native-cereal-preparation-initial.json').read_text())
  repeat = json.loads(execute(prepare,'native-cereal-preparation-reused.json').read_text())
  expected = json.loads((owned/'native-capnp-first.json').read_text())
  assert first['root'] == repeat['root'] and first['tree_sha256'] == repeat['tree_sha256'] == expected['tree_sha256']
  assert first['copied_bytes'] > 0 and repeat['copied_bytes'] == 0
  assert not first['compiler_executed'] and not first['runtime_activated']
  assert all(row['transferred_bytes'] == row['stored_bytes'] == 0 for report in (first,repeat) for row in report['assets'].values())
  guest_sdk = Path(first['root'])
  sdk = work / guest_sdk.relative_to(guest_work)
  rust = json.loads((owned / 'native-rust-first.json').read_text())
  rust_root = Path(guest_work) / 'pinned-rust' / Path(rust['root']).relative_to(owned / 'native-rust-toolchains')
  schema_compiler = guest_sdk / 'usr/bin/capnp'
  libraries = guest_sdk / 'usr/lib/aarch64-linux-gnu'
  receipt = owned / 'native-capnp-tree.json'
  receipt_sha = digest(receipt)
  inputs = Path((owned / 'native-capnp-inputs-cli.txt').read_text().strip())
  cargo = [rust_root / 'bin/cargo', 'build', '--locked', '--manifest-path', f'{source_mount}/rust/Cargo.toml',
           '-p', 'openpilot-cereal', '--example', 'native_roundtrip', '--target-dir', work/'project-target',
           '--message-format=json']
  environment = ['/usr/bin/env', f'OPENPILOT_CAPNP={schema_compiler}', f'LD_LIBRARY_PATH={libraries}',
                 f'RUSTC={rust_root}/bin/rustc']
  evidence['commands'].append(run(guest_command([*environment, schema_compiler, '--version']), log,
                                  stdout_path=work/'native-cereal-capnp-version.txt'))
  assert (work/'native-cereal-capnp-version.txt').read_text().strip() == "Cap'n Proto version 1.0.1"
  room(work, 2*1024**3)
  for phase in ('initial', 'reused'):
    command = [*environment, *cargo, *(['--offline'] if phase == 'reused' else [])]
    output = work/f'native-cereal-{phase}.jsonl'
    evidence['commands'].append(run(guest_command(command),log,stdout_path=output))
    rows = [json.loads(line) for line in output.read_text().splitlines()]
    example = [row for row in rows if row.get('reason') == 'compiler-artifact'
               and row.get('executable') and row['target']['name'] == 'native_roundtrip']
    assert len(example) == 1 and example[0]['fresh'] == (phase == 'reused')
    executable = work/'project-target/debug/examples/native_roundtrip'
    with executable.open('rb') as stream:
      header = stream.read(64)
    assert len(header) == 64 and header[:7] == b'\x7fELF\x02\x01\x01' and int.from_bytes(header[18:20],'little') == 183
    evidence['commands'].append(run(guest_command([example[0]['executable']]),log,
                                    stdout_path=work/f'native-cereal-{phase}-wire.txt'))
    assert (work/f'native-cereal-{phase}-wire.txt').read_bytes() == (owned/'native-cereal-original-wire.txt').read_bytes()
    if phase == 'initial':
      initial_hash = digest(executable)
    else:
      assert digest(executable) == initial_hash
  evidence['commands'].append(run([inputs,'--root',sdk,'--output',work/'native-cereal-sdk-after.json'],log))
  assert digest(work/'native-cereal-sdk-after.json') == receipt_sha
  evidence['native_cereal_sdk'] = dict(source_commit=evidence['source_commit'],example_sha256=initial_hash,
      capnp_version='1.0.1',capnp_sha256=digest(sdk/'usr/bin/capnp'),sdk_tree_sha256=receipt_sha,
      actual_arm_preparer_sha256=digest(preparer),native_preparation_zero_transfer_repeat=True,
      original_python_full_schema_cases=2,exact_wire_bytes_match=True,repeated_artifact_fresh=True,
      sdk_unchanged=True,actual_guest_execution=True,full_runtime_build=False,device_execution=False)



def native_bootstrap_seed_probe(guest_command, work, source_mount, evidence, log, guest_work):
  """Consume a separately pinned small seed, without a current runtime release."""
  room(work, 2*1024**3)
  script = f'{source_mount}/tools/native/bootstrap_seed.py'
  def execute(argv, name):
    evidence['commands'].append(run(guest_command(argv),log,stdout_path=work/name))
    return work/name
  binary = work/'project-target/debug/openpilot-runtime-inputs'
  cargo = (work/'native-project-cargo-path.txt').read_text().strip()
  vendor = json.loads((work/'native-cereal-cargo-inputs-reuse.json').read_text())['vendor']
  pinned_rust = json.loads((work.parent/'native-rust-first.json').read_text())
  guest_rust = Path(guest_work)/'pinned-rust'/Path(pinned_rust['root']).relative_to(work.parent/'native-rust-toolchains')
  exported = json.loads(execute(['/usr/bin/python3',script,'export','--source',source_mount,
      '--binary',binary,'--cargo',cargo,'--vendor',vendor,'--rust-licenses',guest_rust/'share/licenses/rust',
      '--output',work/'bootstrap-seed-export'],
      'native-bootstrap-export.json').read_text())
  assert exported['source']['commit'] == evidence['source_commit'] and not exported['published_release']
  cache = work/'bootstrap-seed-cache'
  manifest = work/'bootstrap-seed-export/manifest.json'
  prepare = ['/usr/bin/python3',script,'prepare','--manifest',manifest,
      '--manifest-sha256',exported['descriptor_sha256'],'--cache',cache,
      '--offline-assets',work/'bootstrap-seed-export','--verify-native']
  prepared = []
  for phase in ('initial','reused'):
    result = json.loads(execute(prepare,f'native-bootstrap-{phase}.json').read_text())
    assert result['seed_source'] == exported['source'] and result['native_abi_checked']
    assert result['transferred_bytes'] == {'helper':0,'notices':0}
    assert not result['seed_executed'] and not result['compiler_executed'] and not result['runtime_activated']
    assert (result['stored_bytes'] > 0) == (phase == 'initial')
    prepared.append(result)
  assert prepared[0]['root'] == prepared[1]['root']
  helper = Path(prepared[0]['helper'])
  host_helper = work/helper.relative_to(guest_work)
  assert digest(host_helper) == digest(binary)
  rust = json.loads(execute([helper,'--prepare-rust','--plan',f'{source_mount}/rust/native-rust-toolchain.json',
      '--cache',Path(guest_work)/'rust-assets','--store',work/'bootstrap-rust'],
      'native-bootstrap-rust.json').read_text())
  assert rust['copied_bytes'] > 0 and all(r['transferred_bytes'] == 0 for r in rust['assets'].values())
  registry = json.loads(execute([helper,'--prepare-cargo','--plan',f'{source_mount}/rust/native-cargo-inputs.json',
      '--lock',f'{source_mount}/rust/Cargo.lock','--cache',Path(guest_work)/'cargo-assets',
      '--store',Path(guest_work)/'cargo-sources'],'native-bootstrap-cargo.json').read_text())
  assert registry['copied_bytes'] == registry['linked_bytes'] == 0
  assert registry['tree_sha256'] == json.loads((work.parent/'native-cargo-first.json').read_text())['tree_sha256']
  assert all(r['transferred_bytes'] == 0 for r in registry['assets'].values())
  toolchain = Path(rust['root'])/'bin'
  command = ['/usr/bin/env',f'RUSTC={toolchain}/rustc',toolchain/'cargo','build','--locked','--offline',
      '--manifest-path',work/'driver-fixture/rust/Cargo.toml','--target-dir',work/'bootstrap-compiler-check',
      '--message-format=json']
  initial = None
  for phase in ('initial','reused'):
    rows = execute(command,f'native-bootstrap-compiler-{phase}.jsonl')
    artifacts = [r for r in map(json.loads,rows.read_text().splitlines()) if r.get('reason') == 'compiler-artifact' and r.get('executable')]
    assert {r['target']['name'] for r in artifacts} == {'first','second'}
    assert all(r['fresh'] == (phase == 'reused') for r in artifacts)
    hashes = {}
    for r in artifacts:
      name = r['target']['name']
      host_binary = work/Path(r['executable']).relative_to(guest_work)
      hashes[name] = digest(host_binary)
      output = execute([r['executable']],f'native-bootstrap-compiler-{phase}-{name}.txt').read_text()
      assert output == f'native AGNOS Rust std {name} {43 if name == "first" else 42}\n'
    if phase == 'initial': initial = hashes
    else: assert hashes == initial
  evidence['native_bootstrap_seed'] = dict(seed_source=exported['source'],seed_sha256=digest(binary),
      descriptor_sha256=exported['descriptor_sha256'],native_default_abi_checked=True,
      offline_artifact_input=True,public_release_download_tested=False,published_release=False,
      seed_executed=True,compiler_executed=True,
      new_native_rust_prefix=True,actual_std_programs=2,repeated_artifacts_fresh=True,
      project_runtime_release_downloaded=False,full_runtime_build=False,device_execution=False)
  # Only this added bootstrap test enters a job-owned host-kernel scope. The
  # original guest commands and the other engineering probes remain unchanged.
  preserve = 'GITHUB_ACTIONS,RUNNER_ENVIRONMENT,RUNNER_NAME,RUNNER_OS,RUNNER_ARCH,CI_JOB_TEMP,GITHUB_RUN_ID,GITHUB_RUN_ATTEMPT,GITHUB_JOB,GITHUB_SHA,GITHUB_WORKSPACE,PATH'
  scope = Path(os.environ['GITHUB_WORKSPACE'])/'tools/ci/native_resource_scope.py'
  driver = Path(os.environ['GITHUB_WORKSPACE'])/'tools/ci/verify_native_bootstrap.py'
  evidence['commands'].append(run(['sudo','-n',f'--preserve-env={preserve}','/usr/bin/python3',scope,
      'enter','--','/usr/bin/python3',driver], log, stdout_path=work/'native-bootstrap-current-driver.txt'))
  evidence['native_bootstrap_current_tools'] = json.loads((work/'bootstrap-current-evidence/summary.json').read_text())


def native_system_sdk_probe(guest_command, work, rootfs, evidence, log, guest_work):
  """Bind the verified factory namespace to the actual native build driver."""
  room(work, 64*1024**2)
  receipt = work/'native-system-sdk.json'
  expected = digest(receipt)
  snapshot = json.loads(receipt.read_text())
  assert snapshot['kind'] == 'agnos-system-sdk' and snapshot['plan']['factory_image_sha256'] == RAW_SHA
  recipe = json.loads((work/'driver-recipe.json').read_text())
  guest_receipt = str(Path(guest_work)/receipt.name)
  recipe['inputs'] = {guest_receipt:expected}
  recipe['input_trees'] = {'/':guest_receipt}
  configuration = work/'driver-system-sdk-recipe.json'
  configuration.write_text(json.dumps(recipe)+'\n')
  driver = work/'project-target/debug/openpilot-runtime-build'
  toolchain = str(Path((work/'native-project-cargo-path.txt').read_text().strip()).parent)
  argv = [driver,'--root',work/'driver-fixture','--recipe',configuration,
          '--cache',work/'driver-cache','--toolchain',toolchain]

  def execute(command, name, required=True):
    result = run(guest_command(command),log,required=required,stdout_path=work/name)
    evidence['commands'].append(result)
    return result

  reports = []
  for phase in ('initial','reused'):
    name = f'runtime-driver-system-sdk-{phase}.json'
    execute(argv,name)
    report = json.loads((work/name).read_text())
    assert report['verified_input_trees'] == {'/':expected}
    artifacts = {Path(row['executable']).name:row for row in report['artifacts']}
    assert artifacts.keys() == {'first','second'}
    assert all(row['fresh'] == (phase == 'reused') for row in artifacts.values())
    for binary,row in artifacts.items():
      output = f'runtime-driver-system-sdk-{phase}-{binary}.stdout'
      execute([row['executable']],output)
      assert (work/output).read_text() == f'native AGNOS Rust std {binary} {43 if binary == "first" else 42}\n'
    reports.append(report)
  assert reports[0]['cache_key'] == reports[1]['cache_key']
  identity = lambda report: {Path(row['executable']).name:row['sha256'] for row in report['artifacts']}
  assert identity(reports[0]) == identity(reports[1])

  # Job-private extracted image only. Keep size/mode unchanged and restore even
  # on assertion failure. Neither the runner system nor any device is modified.
  header = rootfs/'usr/include/stdio.h'
  assert header.is_file() and not header.is_symlink()
  original_hash, original_mode, original_size = digest(header), header.stat().st_mode & 0o7777, header.stat().st_size
  with header.open('rb') as stream:
    original_byte = stream.read(1)
  assert original_byte
  def mutate(value):
    with header.open('r+b') as stream:
      stream.write(value)
      stream.flush()
      os.fsync(stream.fileno())
    assert header.stat().st_size == original_size and header.stat().st_mode & 0o7777 == original_mode
  mutate(bytes([original_byte[0]^1]))
  try:
    offset = log.stat().st_size
    rejected = execute(argv,'runtime-driver-system-sdk-tamper.stdout',False)
    assert rejected['exit_code'] != 0, 'same-size modified native header was accepted'
    with log.open('rb') as stream:
      stream.seek(offset)
      diagnostic = stream.read(65536).decode('utf-8',errors='replace')
    (work/'runtime-driver-system-sdk-tamper-diagnostic.stdout').write_text(diagnostic)
    assert 'system SDK differs from its independently pinned namespace receipt at usr/include/stdio.h' in diagnostic
    for row in reports[-1]['artifacts']:
      binary = Path(row['executable']).name
      output = f'runtime-driver-system-sdk-preserved-{binary}.stdout'
      execute([row['executable']],output)
      assert (work/output).read_text() == f'native AGNOS Rust std {binary} {43 if binary == "first" else 42}\n'
  finally:
    mutate(original_byte)
  assert digest(header) == original_hash
  host_preparer = Path((work.parent/'native-capnp-inputs-cli.txt').read_text().strip())
  profile = Path(os.environ['GITHUB_WORKSPACE'])/'rust/native-system-sdk.json'
  recaptured = work/'native-system-sdk-after.json'
  evidence['commands'].append(run([host_preparer,'--capture-system-sdk','--root',rootfs,
      '--plan',profile,'--output',recaptured],log))
  assert digest(recaptured) == expected, 'factory SDK namespace changed during guest execution'
  evidence['native_system_sdk'] = dict(receipt_sha256=expected,profile_sha256=digest(profile),
      factory_image_sha256=RAW_SHA,namespace_entries=len(snapshot['entries']),
      pinned_files=sum(e['kind']=='file' for e in snapshot['entries'].values()),
      initial_and_repeat_actual_native_driver=True,repeated_artifacts_fresh=True,
      same_size_header_tamper_rejected=True,previous_executables_preserved=True,
      factory_namespace_restored=True,scope='declared compiler namespaces only',
      complete_compiler_search_closure_proven=False,full_runtime_build=False,device_execution=False)
  # The full-generation boundary additionally rejects omitted/reduced profiles.
  # These owned fixtures exercise the real 99-bin/13-group roster, not a full
  # runtime build or a replacement for the genuine live namespace checks above.
  output=work/'native-generation-sdk-tests.txt'
  evidence['commands'].append(run(guest_command(['cargo','test','--locked','--manifest-path',
      f'{source_mount}/rust/Cargo.toml','-p','openpilot-runtime-build','--lib',
      'system_sdk::generation_input_tests','--target-dir',work/'project-target',
      '--','--nocapture']),log,stdout_path=output))
  assert '4 passed; 0 failed;' in output.read_text()
  evidence['native_generation_sdk_requirements']=dict(cases=4,roster_binaries=99,roster_groups=13,
      all_groups_same_live_sdk_required=True,reduced_or_other_factory_profile_rejected=True,
      ordinary_tree_substitution_rejected=True,boot_metadata_independent_of_removed_input_cache=True,
      fixture_profile_validation_only=True,full_generation_built=False,device_execution=False)


def native_codec_probe(guest_command, work, source_mount, evidence, log, guest_work):
  """Prepare the real pinned wheels on ARM, then link and execute their APIs."""
  room(work,1024**3)
  def execute(argv,name):
    evidence['commands'].append(run(guest_command(argv),log,stdout_path=work/name))
    return work/name
  execute(['cargo','build','--locked','--manifest-path',f'{source_mount}/rust/Cargo.toml',
      '-p','openpilot-dev-tools','--bin','stage-encoder-target','--target-dir',work/'project-target',
      '--message-format=json'],'native-codec-tool-build.jsonl')
  preparer=work/'project-target/debug/stage-encoder-target'
  command=[preparer,'--prepare-wheels','--root',source_mount,'--cache',Path(guest_work)/'codec-assets',
      '--store',work/'native-codec-prefixes']
  first=json.loads(execute(command,'native-codec-initial.json').read_text())
  repeat=json.loads(execute(command,'native-codec-reused.json').read_text())
  host=json.loads((work.parent/'native-codec-first.json').read_text())
  assert set(first)==set(repeat)==set(host)=={'ffmpeg','libyuv'}
  for name,row in first.items():
    assert row['tree_sha256']==repeat[name]['tree_sha256']==host[name]['tree_sha256']
    assert row['copied_bytes']>0 and repeat[name]['copied_bytes']==0
    assert all(r['asset']['transferred_bytes']==r['asset']['stored_bytes']==0 for r in (row,repeat[name]))
    assert not row['compiler_executed'] and not row['runtime_activated']
  programs={
    'libyuv':('''#include <libyuv.h>
#include <cstdio>
int main() {
  const unsigned char y[4]={235,235,235,235},u[1]={128},v[1]={128};
  unsigned char argb[16]={0};
  if(libyuv::I420ToARGB(y,2,u,1,v,1,argb,8,2,2)) return 11;
  for(auto value:argb) if(value!=255) return 12;
  std::puts("native libyuv white pixels matched");
}
''',['yuv'],'native libyuv white pixels matched\n'),
    'ffmpeg':('''extern "C" {
#include <libavutil/avutil.h>
#include <libavutil/version.h>
}
#include <cstdio>
static_assert(LIBAVUTIL_VERSION_MAJOR==59);
int main() {
  if(avutil_version()!=LIBAVUTIL_VERSION_INT) return 13;
  std::puts("native FFmpeg avutil header/library ABI matched");
}
''',['avutil'],'native FFmpeg avutil header/library ABI matched\n'),
  }
  hashes={}
  for name,(code,libraries,expected) in programs.items():
    source=work/f'native-codec-{name}.cc'
    source.write_text(code)
    prefix=Path(first[name]['root'])
    output=work/f'native-codec-{name}'
    execute(['g++','-std=c++17','-I',prefix/'include',source,
        *(prefix/'lib'/f'lib{library}.a' for library in libraries),'-pthread','-lm','-ldl','-o',output],
        f'native-codec-{name}-build.txt')
    assert execute([output],f'native-codec-{name}-run.txt').read_text()==expected
    with output.open('rb') as stream: header=stream.read(64)
    assert header[:6]==b'\x7fELF\x02\x01' and int.from_bytes(header[18:20],'little')==183
    hashes[name]=dict(source_sha256=digest(source),elf_sha256=digest(output),
        tree_sha256=first[name]['tree_sha256'],archive_sha256=first[name]['asset']['sha256'])
  evidence['native_codec_wheels']=dict(preparer_sha256=digest(preparer),components=hashes,
      actual_arm_preparer=True,repeated_body_bytes=0,repeated_copied_bytes=0,
      independently_expected_host_tree_matches=True,actual_native_cpp_programs=2,
      libswscale_built=False,complete_codec_regression=False,full_runtime_build=False,device_execution=False)


def native_ffmpeg_headers_probe(guest_command,work,source_mount,evidence,log,guest_work):
  """Verify the actual final-runtime header version, without substituting 7.1."""
  room(work,128*1024**2)
  def execute(argv,name):
    evidence['commands'].append(run(guest_command(argv),log,stdout_path=work/name))
    return work/name
  execute(['cargo','build','--locked','--manifest-path',f'{source_mount}/rust/Cargo.toml',
      '-p','openpilot-dev-tools','--bin','build-runtime-ffmpeg','--target-dir',work/'project-target',
      '--message-format=json'],'native-ffmpeg611-tool-build.jsonl')
  preparer=work/'project-target/debug/build-runtime-ffmpeg'
  command=[preparer,'--prepare-released-headers','--plan',f'{source_mount}/rust/native-ffmpeg-headers.json',
      '--cache',Path(guest_work)/'ffmpeg-source','--store',work/'native-ffmpeg611-headers']
  first=json.loads(execute(command,'native-ffmpeg611-headers-initial.json').read_text())
  repeated=json.loads(execute(command,'native-ffmpeg611-headers-reused.json').read_text())
  host=json.loads((work.parent/'native-ffmpeg-headers-first.json').read_text())
  assert first['public_headers']==repeated['public_headers']==host['public_headers']==140
  assert first['tree_sha256']==repeated['tree_sha256']==host['tree_sha256']
  assert first['root']==repeated['root'] and first['copied_bytes']>0 and repeated['copied_bytes']==0
  assert all(r['asset']['transferred_bytes']==r['asset']['stored_bytes']==0 for r in (first,repeated))
  assert not first['compiler_executed'] and not first['provider_libraries_prepared'] and not first['actual_target_execution']
  source=work/'native-ffmpeg611-headers.cc'
  source.write_text('''extern "C" {
#include <libavcodec/avcodec.h>
#include <libavformat/avformat.h>
#include <libavutil/avutil.h>
#include <libavutil/avconfig.h>
#include <libavdevice/avdevice.h>
#include <libavfilter/avfilter.h>
#include <libswscale/swscale.h>
#include <libswresample/swresample.h>
#include <libpostproc/postprocess.h>
}
#if !defined(__aarch64__) || !defined(__AARCH64EL__) || AV_HAVE_BIGENDIAN != 0 || AV_HAVE_FAST_UNALIGNED != 1
#error Actual_native_compiler_does_not_match_original_generated_headers
#endif
static_assert(sizeof(void*)==8);
static_assert(LIBAVCODEC_VERSION_INT==AV_VERSION_INT(60,31,102));
static_assert(LIBAVFORMAT_VERSION_INT==AV_VERSION_INT(60,16,100));
static_assert(LIBAVUTIL_VERSION_INT==AV_VERSION_INT(58,29,100));
static_assert(LIBAVDEVICE_VERSION_INT==AV_VERSION_INT(60,3,100));
static_assert(LIBAVFILTER_VERSION_INT==AV_VERSION_INT(9,12,100));
static_assert(LIBSWSCALE_VERSION_INT==AV_VERSION_INT(7,5,100));
static_assert(LIBSWRESAMPLE_VERSION_INT==AV_VERSION_INT(4,12,100));
static_assert(LIBPOSTPROC_VERSION_INT==AV_VERSION_INT(57,3,100));
int native_header_identity(){return LIBAVUTIL_VERSION_INT;}
''')
  output=work/'native-ffmpeg611-headers.o'
  execute(['g++','-std=c++17','-I',Path(first['root'])/'include','-c',source,'-o',output],
      'native-ffmpeg611-header-compile.txt')
  with output.open('rb') as stream: header=stream.read(64)
  assert header[:6]==b'\x7fELF\x02\x01' and int.from_bytes(header[16:18],'little')==1 and int.from_bytes(header[18:20],'little')==183
  after=json.loads(execute(command,'native-ffmpeg611-headers-after.json').read_text())
  assert after['tree_sha256']==first['tree_sha256'] and after['copied_bytes']==after['asset']['transferred_bytes']==0
  evidence['native_released_ffmpeg_headers']=dict(preparer_sha256=digest(preparer),public_headers=140,
      tree_sha256=first['tree_sha256'],source_archive_sha256=first['asset']['sha256'],
      actual_arm_preparer=True,independently_expected_host_tree_matches=True,
      actual_native_header_object_sha256=digest(output),header_probe_source_sha256=digest(source),
      repeated_body_bytes=0,repeated_copied_bytes=0,provider_version='6.1.1',
      libraries_linked=False,provider_execution_tested=False,full_runtime_build=False,device_execution=False)


def native_provider_probe(guest_command,work,source_mount,evidence,log,guest_work):
  """Consume only the small foreign component; old project binaries are absent."""
  room(work,128*1024**2)
  def execute(argv,name):
    evidence['commands'].append(run(guest_command(argv),log,stdout_path=work/name))
    return work/name
  exported=json.loads((work.parent/'native-provider-export.json').read_text())
  host=json.loads((work.parent/'native-provider-consumer-initial.json').read_text())
  helper=work/'project-target/debug/build-runtime-ffmpeg'
  command=[helper,'--prepare-released-provider','--plan',f'{source_mount}/rust/native-ffmpeg-provider.json',
      '--descriptor',Path(guest_work)/'provider-export/provider.json','--descriptor-sha256',exported['descriptor_sha256'],
      '--cache',Path(guest_work)/'provider-assets','--store',work/'native-provider-prefixes']
  first=json.loads(execute(command,'native-provider-initial.json').read_text())
  repeated=json.loads(execute(command,'native-provider-reused.json').read_text())
  assert first['tree_sha256']==repeated['tree_sha256']==host['tree_sha256']==exported['tree_sha256']
  assert first['provider_source_commit']==repeated['provider_source_commit']==exported['provider_source_commit']
  assert first['copied_bytes']>0 and repeated['copied_bytes']==0
  assert all(row['offline_seed_bytes']==row['asset']['transferred_bytes']==row['asset']['stored_bytes']==0 for row in (first,repeated))
  assert all(not row['origin_archive_downloaded'] and not row['compiler_executed'] and not row['runtime_activated'] for row in (first,repeated))
  prefix=Path(first['root'])
  headers=Path(json.loads((work/'native-ffmpeg611-headers-initial.json').read_text())['root'])
  source=work/'native-provider-abi.cc'
  source.write_text('''extern "C" {
#include <libavcodec/avcodec.h>
#include <libavformat/avformat.h>
#include <libavutil/avutil.h>
#include <libavdevice/avdevice.h>
#include <libavfilter/avfilter.h>
#include <libswscale/swscale.h>
#include <libswresample/swresample.h>
#include <libpostproc/postprocess.h>
}
#include <cstdio>
int main(){
  if(avcodec_version()!=LIBAVCODEC_VERSION_INT || avformat_version()!=LIBAVFORMAT_VERSION_INT ||
     avutil_version()!=LIBAVUTIL_VERSION_INT || avdevice_version()!=LIBAVDEVICE_VERSION_INT ||
     avfilter_version()!=LIBAVFILTER_VERSION_INT || swscale_version()!=LIBSWSCALE_VERSION_INT ||
     swresample_version()!=LIBSWRESAMPLE_VERSION_INT || postproc_version()!=LIBPOSTPROC_VERSION_INT) return 19;
  std::puts("native released FFmpeg 6.1.1 ABI matched");
}
''')
  program=work/'native-provider-abi'
  execute(['g++','-std=c++17','-I',headers/'include',source,'-L',prefix/'lib',
      f'-Wl,-rpath-link,{prefix}/lib','-lavdevice','-lavfilter','-lavformat','-lavcodec','-lpostproc',
      '-lswresample','-lswscale','-lavutil','-o',program],'native-provider-abi-build.txt')
  execute(['cargo','build','--locked','--manifest-path',f'{source_mount}/rust/Cargo.toml','-p','openpilot-dev-tools',
      '--bin','runtime-elf-audit','--target-dir',work/'project-target','--message-format=json'],'native-provider-auditor-build.jsonl')
  audit=work/'native-provider-abi-audit.json'
  execute([work/'project-target/debug/runtime-elf-audit','--root',program,'--root',prefix/'bin/ffmpeg',
      '--root',prefix/'bin/ffprobe','--library',prefix/'lib','--library','/usr/lib/aarch64-linux-gnu',
      '--library','/lib/aarch64-linux-gnu','--output',audit],'native-provider-audit-summary.json')
  assert json.loads(audit.read_text())['compatible']
  environment=['/usr/bin/env',f'LD_LIBRARY_PATH={prefix}/lib']
  assert execute([*environment,program],'native-provider-abi-run.txt').read_text()=='native released FFmpeg 6.1.1 ABI matched\n'
  for name in ('ffmpeg','ffprobe'):
    version=execute([*environment,prefix/'bin'/name,'-version'],f'native-provider-{name}-version.txt').read_text()
    assert version.startswith(f'{name} version 6.1.1')
  after=json.loads(execute(command,'native-provider-after-execution.json').read_text())
  assert after['tree_sha256']==first['tree_sha256'] and after['copied_bytes']==after['asset']['transferred_bytes']==0
  evidence['native_released_provider']=dict(provider_source_commit=first['provider_source_commit'],
      tree_sha256=first['tree_sha256'],component_sha256=first['asset']['sha256'],component_bytes=first['asset']['bytes'],
      descriptor_sha256=exported['descriptor_sha256'],actual_arm_consumer=True,origin_archive_downloaded_by_consumer=False,
      header_and_shared_library_versions_matched=8,strict_elf_audit_compatible=True,foreign_cli_programs_executed=2,
      abi_program_sha256=digest(program),abi_source_sha256=digest(source),independent_host_tree_matches=True,
      offline_component_seed=True,published_release=False,public_component_download_tested=False,
      project_binaries_reused=False,complete_codec_regression=False,full_runtime_build=False,device_execution=False)


def native_python_probe(guest_command,work,source_mount,evidence,log,guest_work):
  """Run the pinned Python itself; model wheels/generation remain separate."""
  plan=json.loads((Path(os.environ['GITHUB_WORKSPACE'])/'rust/native-python-toolchain.json').read_text())
  room(work,256*1024**2+plan['reserve_bytes'])
  host=json.loads((work.parent/'native-python-initial.json').read_text())
  def execute(argv,name):
    record=run(guest_command(argv),log,stdout_path=work/name)
    evidence['commands'].append(record)
    return record
  command=[work/'project-target/debug/openpilot-runtime-inputs','--prepare-python',
      '--plan',f'{source_mount}/rust/native-python-toolchain.json',
      '--cache',Path(guest_work)/'python-assets','--store',work/'native-python-prefixes']
  rows=[]
  for phase in ('initial','reused'):
    execute(command,f'native-python-{phase}.json')
    row=json.loads((work/f'native-python-{phase}.json').read_text())
    assert row['version']==plan['version'] and row['target']==plan['target']
    assert row['tree_sha256']==host['tree_sha256'] and row['identity']==host['identity']
    assert row['asset']['transferred_bytes']==row['asset']['stored_bytes']==0
    assert (row['copied_bytes']>0)==(phase=='initial')
    if phase=='reused': assert row['expected_additional_bytes']==0
    assert not row['installer_executed'] and not row['interpreter_executed'] and not row['runtime_activated']
    rows.append(row)
  assert rows[0]['root']==rows[1]['root']
  execute(['/usr/bin/python3','--version'],'native-python-factory-version.txt')
  prefix=Path(rows[0]['root'])
  execute([rows[0]['python'],'-I','-B','-S',f'{source_mount}/tools/ci/fixtures/native-python-health.py',
      '--prefix',prefix,'--output',work/'native-python-health.json'],'native-python-health.txt')
  health=json.loads((work/'native-python-health.json').read_text())
  assert health['version']==plan['version'] and health['architecture']=='aarch64'
  assert health['isolated'] and not health['bytecode_written'] and not health['model_generated']
  assert not health['gpu_executed'] and not health['physical_device_execution']
  # Every ELF member, including extension modules not imported by the health
  # fixture, must have a compatible closure against this prefix/factory SDK.
  actual_prefix=work/prefix.relative_to(guest_work)
  receipt=json.loads((work/Path(rows[0]['tree_receipt']).relative_to(guest_work)).read_text())
  roots=[]
  library_dirs={prefix/'lib',prefix/'lib/python3.12/lib-dynload'}
  for path,entry in receipt['entries'].items():
    if entry['kind']!='file': continue
    with (actual_prefix/path).open('rb') as stream:
      magic=stream.read(4)
    if magic==b'\x7fELF':
      roots.append(prefix/path)
      library_dirs.add((prefix/path).parent)
  assert prefix/'bin/python3.12' in roots
  audit=work/'native-python-abi.json'
  execute([work/'project-target/debug/runtime-elf-audit',*(arg for path in roots for arg in ('--root',path)),
      *(arg for path in sorted(library_dirs) for arg in ('--library',path)),
      '--library','/usr/lib/aarch64-linux-gnu','--library','/lib/aarch64-linux-gnu','--output',audit],
      'native-python-abi-summary.json')
  assert json.loads(audit.read_text())['compatible'] is True
  execute(command,'native-python-after-health.json')
  after=json.loads((work/'native-python-after-health.json').read_text())
  assert after['tree_sha256']==rows[0]['tree_sha256']
  assert after['copied_bytes']==after['asset']['transferred_bytes']==after['asset']['stored_bytes']==0
  evidence['native_python']=dict(version=health['version'],target=plan['target'],
      archive_sha256=plan['asset']['sha256'],tree_sha256=rows[0]['tree_sha256'],
      actual_arm_preparer=True,actual_arm_interpreter_executed=True,host_expected_tree_matches=True,
      strict_all_elf_closure_compatible=True,elf_members=len(roots),
      stdlib_checks=health['standard_library_checks'],prefix_unchanged_after_execution=True,
      repeated_http_body_bytes=0,repeated_copied_bytes=0,model_generated=False,
      python_model_environment_complete=False,runtime_activated=False,physical_device_execution=False,
      execution_mode=NATIVE_PROOT['MODE'], native_arm_execution=True, emulated=False)


def native_model_wheels_probe(guest_command,work,source_mount,evidence,log,guest_work):
  """Run the exact Python preparation dependencies and genuine QCOM compiler."""
  source=Path(os.environ['GITHUB_WORKSPACE'])
  plan=json.loads((source/'rust/native-model-wheels.json').read_text())
  room(work,sum(p['maximum_unpacked_bytes'] for p in plan['packages'].values())+8*1024**2+512*1024**2)
  python=json.loads((work/'native-python-initial.json').read_text())
  host=json.loads((work.parent/'native-model-wheels-initial.json').read_text())
  def execute(argv,name):
    record=run(guest_command(argv),log,stdout_path=work/name)
    evidence['commands'].append(record)
    return record
  command=[python['python'],'-I','-B','-S',f'{source_mount}/rust/tools/native_model_inputs.py',
      '--root',source_mount,'--preparer',work/'project-target/debug/openpilot-runtime-inputs',
      '--cache',Path(guest_work)/'model-wheel-assets','--store',work/'native-model-wheel-packages',
      '--environment-store',work/'native-model-wheel-environments']
  rows=[]
  for phase in ('initial','reused'):
    execute([*command,'--output',work/f'native-model-wheels-{phase}.json'],f'native-model-wheels-{phase}.txt')
    row=json.loads((work/f'native-model-wheels-{phase}.json').read_text())
    assert row['source']==host['source'] and row['source']['commit']==evidence['source_commit']
    assert row['tree_sha256']==host['tree_sha256'] and row['identity']==host['identity']
    assert set(row['packages'])==set(host['packages'])=={'numpy','zstandard'}
    for name in row['packages']:
      assert row['packages'][name]['tree_sha256']==host['packages'][name]['tree_sha256']
    assert row['transferred_bytes']==row['stored_bytes']==0
    assert (row['copied_bytes']>0)==(phase=='initial') and (row['linked_bytes']>0)==(phase=='initial')
    assert not row['wheels_imported'] and not row['compiler_executed'] and not row['model_generated']
    assert not row['installer_executed'] and not row['runtime_activated']
    rows.append(row)
  rust=json.loads((work.parent/'native-rust-first.json').read_text())
  rust_root=Path(guest_work)/'pinned-rust'/Path(rust['root']).relative_to(work.parent/'native-rust-toolchains')
  execute(['/usr/bin/env',f'RUSTC={rust_root}/bin/rustc','OPENPILOT_NATIVE_INCREMENTAL=1',rust_root/'bin/cargo',
      'build','--locked','--offline','--manifest-path',f'{source_mount}/rust/Cargo.toml',
      '-p','openpilot-model-runtime','--bin','offline-qcom-compile','--bin','model-run','--bin','qcom-model-run',
      '--target-dir',work/'project-target','--message-format=json'],'native-model-compiler-build.jsonl')
  artifacts=[json.loads(line) for line in (work/'native-model-compiler-build.jsonl').read_text().splitlines()]
  binaries={row['target']['name']:row for row in artifacts
      if row.get('reason')=='compiler-artifact' and row.get('executable')}
  assert set(binaries)=={'offline-qcom-compile','model-run','qcom-model-run'}
  for row in binaries.values():
    assert 'native-skip-miri' in row['features']
  compiler=work/'project-target/debug/offline-qcom-compile'
  compiler_row=binaries['offline-qcom-compile']
  import sys
  sys.path.insert(0,str(source/'rust/tools'))
  from native_model_generation import source_namespace
  source_tree=subprocess.check_output(['git','-C',source,'rev-parse','HEAD^{tree}'],text=True).strip()
  (work/'native-model-compiler-build.json').write_text(json.dumps(dict(schema=1,
      source=dict(commit=evidence['source_commit'],tree=source_tree),
      compiler_sha256=digest(compiler),compiler_bytes=compiler.stat().st_size,
      features=sorted(compiler_row['features']),profile=compiler_row['profile'],
      rust_toolchain_identity=rust['identity'],rust_toolchain_tree_sha256=rust['tree_sha256'],
      cargo_lock_sha256=digest(source/'rust/Cargo.lock')),indent=2)+'\n')
  build_path=work/'native-model-compiler-build.json'
  build_record=json.loads(build_path.read_text())
  build_record['compiler_sources_sha256']=source_namespace(source,compiler=True)
  build_record['cargo_sources_tree_sha256']=json.loads((work/'native-cereal-cargo-inputs-reuse.json').read_text())['tree_sha256']
  build_record['system_sdk_receipt_sha256']=digest(work/'native-system-sdk.json')
  build_path.write_text(json.dumps(build_record,indent=2)+'\n')
  execute([python['python'],'-I','-B','-S',f'{source_mount}/tools/ci/fixtures/native-model-environment.py',
      '--source',source_mount,'--wheels',work/'native-model-wheels-initial.json',
      '--compiler',compiler,'--output',work/'native-model-environment.json'],'native-model-environment.txt')
  report=json.loads((work/'native-model-environment.json').read_text())
  assert report['wheels_tree_sha256']==rows[0]['tree_sha256']
  assert report['cpu_llvm_tensor_matches'] and report['numerics_and_zstandard_match']
  assert report['invalid_qcom_source_rejected'] and report['original_offline_pipeline_imported']
  assert report['unbound_external_initializer_and_attribute_rejected']
  assert report['compiler_executable_sha256']==digest(compiler)
  assert not report['gpu_executed'] and not report['full_model_generation'] and not report['physical_device_execution']
  prefix=Path(rows[0]['root'])
  actual_prefix=work/prefix.relative_to(guest_work)
  receipt=json.loads((work/Path(rows[0]['tree_receipt']).relative_to(guest_work)).read_text())
  roots=[compiler,work/'project-target/debug/model-run',work/'project-target/debug/qcom-model-run',report['cpu_llvm']['path'],report['qcom']['path']]
  libraries=set()
  for path,entry in receipt['entries'].items():
    if entry['kind']!='file': continue
    with (actual_prefix/path).open('rb') as stream: magic=stream.read(4)
    if magic==b'\x7fELF':
      roots.append(prefix/path)
      libraries.add((prefix/path).parent)
  audit=work/'native-model-environment-abi.json'
  execute([work/'project-target/debug/runtime-elf-audit',*(arg for path in roots for arg in ('--root',path)),
      *(arg for path in sorted(libraries) for arg in ('--library',path)),
      '--library','/usr/lib/aarch64-linux-gnu','--library','/lib/aarch64-linux-gnu','--output',audit],
      'native-model-environment-abi-summary.json')
  assert json.loads(audit.read_text())['compatible'] is True
  execute([*command,'--output',work/'native-model-wheels-after-health.json'],'native-model-wheels-after-health.txt')
  after=json.loads((work/'native-model-wheels-after-health.json').read_text())
  assert after['tree_sha256']==rows[0]['tree_sha256']
  assert after['copied_bytes']==after['linked_bytes']==after['transferred_bytes']==after['stored_bytes']==0
  evidence['native_model_preparation_dependencies']=dict(wheels_tree_sha256=rows[0]['tree_sha256'],
      python_version=report['python_version'],packages=['numpy','zstandard'],source_built_arm_compiler=True,
      cpu_llvm=report['cpu_llvm'],qcom=report['qcom'],qcom_compiler_sha256=report['compiler_executable_sha256'],
      qcom_kernel_sha256=report['qcom_kernel_sha256'],invalid_qcom_source_rejected=True,
      unbound_external_initializer_and_attribute_rejected=True,
      strict_elf_closure_compatible=True,prefix_unchanged_after_execution=True,
      repeated_http_body_bytes=0,repeated_copied_bytes=0,repeated_linked_bytes=0,
      full_model_generation=False,full_model_regression=False,full_runtime_build=False,gpu_executed=False,
      physical_device_execution=False,execution_mode=NATIVE_PROOT['MODE'], native_arm_execution=True, emulated=False)


def native_model_generation_probe(guest_command,work,source_mount,evidence,log,guest_work):
  """Generate four real model pipelines, then prove zero generation/compiler calls."""
  python=json.loads((work/'native-python-initial.json').read_text())
  command=[python['python'],'-I','-B','-S','-X',f'pycache_prefix={guest_work}/model-generation-bytecode',
      f'{source_mount}/rust/tools/native_model_generation.py',
      '--root',source_mount,'--preparer',work/'project-target/debug/openpilot-runtime-inputs',
      '--compiler',work/'project-target/debug/offline-qcom-compile',
      '--compiler-build',work/'native-model-compiler-build.json',
      '--consumer',work/'project-target/debug/model-run',
      '--bundle-consumer',work/'project-target/debug/qcom-model-run',
      '--python-cache',Path(guest_work)/'python-assets','--python-store',work/'native-python-prefixes',
      '--wheel-cache',Path(guest_work)/'model-wheel-assets','--wheel-store',work/'native-model-wheel-packages',
      '--environment-store',work/'native-model-wheel-environments',
      '--model-cache',Path(guest_work)/'model-assets','--output',work/'native-model-catalogs',
      '--export-store',work/'native-model-exports']
  rows=[]
  for phase in ('initial','reused'):
    record=run(guest_command([*command,'--report',work/f'native-model-generation-{phase}.json']),
        log,stdout_path=work/f'native-model-generation-{phase}.txt')
    evidence['commands'].append(record)
    row=json.loads((work/f'native-model-generation-{phase}.json').read_text())
    assert row['source']['commit']==evidence['source_commit']
    assert row['full_model_generation'] and not row['full_model_regression'] and not row['full_runtime_build']
    assert not row['gpu_executed'] and not row['physical_device_execution']
    assert row['transferred_bytes']==row['stored_bytes']==0
    assert all(value==0 for value in row['environment_preparation'].values())
    assert len(row['bundle_validation'])==4 and all(not b['gpu_executed'] for b in row['bundle_validation'])
    exported=row['export']
    assert not exported['runtime_activated'] and not exported['consumer']['gpu_executed']
    assert not exported['consumer']['runtime_activated']
    assert len(exported['runtime_inputs']['trees'])==len(exported['runtime_inputs']['files'])==1
    assert all('.model-cache' not in p and not p.endswith('/lock') for p in exported['files'])
    if phase=='reused':
      assert exported['root']==rows[0]['export']['root'] and exported['tree_sha256']==rows[0]['export']['tree_sha256']
      assert exported['copied_bytes']==exported['linked_bytes']==0
    assert set(row['kinds'])=={'driving','driver'}
    for kind,value in row['kinds'].items():
      assert value['consumer_validated'] and not value['runtime_activated']
      assert value['generated_pipelines']==(2 if phase=='initial' else 0)
      assert value['reused_pipelines']==(0 if phase=='initial' else 2)
      assert (value['qcom_compiler_calls']>0)==(phase=='initial')
      if phase=='reused':
        assert value['linked_output_bytes']==value['published_generated_output_bytes']==0
        assert value['generation']==rows[0]['kinds'][kind]['generation']
        assert value['inputs']==rows[0]['kinds'][kind]['inputs']
        assert value['origins']==rows[0]['kinds'][kind]['origins']
    rows.append(row)
  evidence['native_model_generation']=dict(source_commit=evidence['source_commit'],
      compiler_build_source_commit=evidence['source_commit'],
      cameras=[[1928,1208],[1344,760]],kinds=['driving','driver'],
      original_models=rows[0]['original_models'],initial=rows[0]['kinds'],repeated=rows[1]['kinds'],
      repeated_http_body_bytes=0,repeated_qcom_compiler_calls=0,repeated_generated_pipelines=0,
      full_model_generation=True,full_model_regression=False,full_runtime_build=False,
      gpu_executed=False,physical_device_execution=False,
      measurement_limit='per-command wall time and output counters; aggregate RAM/swap/disk peak remains separate',
      execution_mode=NATIVE_PROOT['MODE'], native_arm_execution=True, emulated=False)
  # Owned source commits exercise the actual producer's reuse key across Git
  # revisions. The compiler remains the original source-built binary; do not
  # retag it as built from either fixture revision. No source push or activation.
  source=Path(os.environ['GITHUB_WORKSPACE'])
  tracked=subprocess.check_output(['git','-C',source,'ls-files','-z']).decode().rstrip('\0').split('\0')
  room(work,sum((source/name).lstat().st_size for name in tracked)+16*1024**2)
  fixture=work/'native-model-fixture-source'
  def execute(argv,name):
    record=run(guest_command(argv),log,stdout_path=work/name)
    evidence['commands'].append(record)
    return (work/name).read_text()
  execute(['git','-c','filter.lfs.required=false','-c','filter.lfs.smudge=','-c','filter.lfs.process=',
      'clone','--shared',source_mount,fixture],'native-model-fixture-clone.txt')
  fixture_cases=[]
  for case,name,before,after in (
      ('docs','docs/naver/native-model-generation-cache-600.md',None,'\nOwned CI docs-only source fixture.\n'),
      ('small-logger','rust/crates/loggerd/src/main.rs','Continuous native route logger.',
       'Continuous native route logger (owned model cache fixture).')):
    path=fixture/name
    text=path.read_text()
    if before is None: text+=after
    else:
      assert before in text
      text=text.replace(before,after,1)
    path.write_text(text)
    execute(['git','-C',fixture,'add',name],f'native-model-{case}-add.txt')
    execute(['git','-C',fixture,'-c','user.name=Owned native model fixture',
        '-c','user.email=fixture@example.invalid','-c','commit.gpgsign=false','-c','core.hooksPath=/dev/null',
        'commit','-m',f'Owned {case} model generation reuse fixture'],f'native-model-{case}-commit.txt')
    fixture_sha=execute(['git','-C',fixture,'rev-parse','HEAD'],f'native-model-{case}-sha.txt').strip()
    owned_command=command.copy()
    owned_command[owned_command.index('--root')+1]=fixture
    # Use the fixture's script too, rather than importing a helper from the
    # original source and merely reporting a different source SHA.
    owned_command[owned_command.index(f'{source_mount}/rust/tools/native_model_generation.py')]=fixture/'rust/tools/native_model_generation.py'
    execute([*owned_command,'--report',work/f'native-model-generation-{case}.json'],f'native-model-generation-{case}.txt')
    row=json.loads((work/f'native-model-generation-{case}.json').read_text())
    assert row['source']['commit']==fixture_sha and fixture_sha!=evidence['source_commit']
    assert row['transferred_bytes']==row['stored_bytes']==0
    assert all(value==0 for value in row['environment_preparation'].values())
    assert row['export']['root']==rows[0]['export']['root'] and row['export']['tree_sha256']==rows[0]['export']['tree_sha256']
    assert row['export']['copied_bytes']==row['export']['linked_bytes']==0
    for kind,value in row['kinds'].items():
      assert value['generated_pipelines']==value['qcom_compiler_calls']==value['linked_output_bytes']==0
      assert value['reused_pipelines']==2 and value['consumer_validated']
      assert value['inputs']==rows[0]['kinds'][kind]['inputs']
      assert value['generation']==rows[0]['kinds'][kind]['generation']
      assert value['origins']==rows[0]['kinds'][kind]['origins']
    fixture_cases.append(dict(case=case,request_source=row['source'],kinds=row['kinds'],
        compiler_build_source_commit=evidence['source_commit'],runtime_compiled=False))
  evidence['native_model_generation']['actual_source_reuse_cases']=fixture_cases
  old=json.loads((work/'native-runtime-dbc-inputs.json').read_text())
  model_input=rows[0]['export']['runtime_inputs']
  for key in ('files','trees'):
    assert not set(old[key]) & set(model_input[key])
    old[key].update(model_input[key])
  assert len(old['files'])==len(old['trees'])==12
  (work/'native-model-runtime-inputs.json').write_text(json.dumps(old,indent=2)+'\n')
  pins=json.loads((work/'native-project-input-pins.json').read_text())
  pins['runtime_inputs']=old
  (work/'native-model-input-pins.json').write_text(json.dumps(pins,indent=2)+'\n')
  cargo=Path(guest_work)/'pinned-rust'/Path(json.loads((work.parent/'native-rust-first.json').read_text())['root']).relative_to(work.parent/'native-rust-toolchains')/'bin/cargo'
  execute([work/'project-target/debug/openpilot-runtime-plan','--root',source_mount,'--cargo',cargo,
      '--native-inputs',work/'native-model-input-pins.json','--budgets',work/'native-project-growth-estimates.json',
      '--output',work/'native-model-bound-plan'], 'native-model-bound-plan-summary.json')
  summary=json.loads((work/'native-model-bound-plan-summary.json').read_text())
  assert not summary['compiled'] and not summary['activated']
  bound=json.loads((work/'native-model-bound-plan/build-plan.json').read_text())
  original=json.loads((work/'native-project-bindings-initial/build-plan.json').read_text())
  assert bound['runtime_inputs']==old and bound['groups']==original['groups']
  evidence['native_model_generation']['export']=rows[0]['export']
  # Run the actual RadarCAN database loader against all unchanged original data.
  room(work,3*1024**3)
  group=next(g for g in bound['groups'].values() if 'openpilot-radarcan' in g['binaries'])
  environment={**group['environment'],'CARGO_INCREMENTAL':'0',
      'CARGO_PROFILE_DEV_DEBUG':'0','RUSTC':str(cargo.parent/'rustc'),'RUSTDOC':str(cargo.parent/'rustdoc')}
  env=['/usr/bin/env',*(f'{k}={v}' for k,v in environment.items())]
  features=[f for f in group['features'] if f.split('/')[0] in ('openpilot-card','openpilot-msgq')]
  target=work/'native-dbc-consumer-target'
  execute([*env,cargo,'--config','source.crates-io.replace-with="openpilot-native"',
      '--config','source.openpilot-native.directory='+json.dumps(environment['OPENPILOT_CARGO_VENDOR']),
      'build','--locked','--offline','--manifest-path',f'{source_mount}/rust/Cargo.toml',
      '--target','aarch64-unknown-linux-gnu','-p','openpilot-radarcan','--example','native_dbc_inputs',
      '--features',','.join(features),'--target-dir',target],'native-dbc-consumer-build.txt')
  provider=json.loads((work/'native-dbc-initial.json').read_text())
  execute([target/'aarch64-unknown-linux-gnu/debug/examples/native_dbc_inputs',
      Path(provider['root'])/'rust/bin/radarcan-dbc'],'native-dbc-consumer.json')
  consumer=json.loads((work/'native-dbc-consumer.json').read_text())
  assert consumer['databases']==103 and consumer['messages']>0 and not consumer['runtime_activated']
  evidence['native_dbc_inputs']['rust_database_consumer']=consumer


def native_encoder_probe(guest_command,work,source_mount,evidence,log,guest_work):
  """Build the actual runtime binary with the catalog features and pinned inputs."""
  room(work,3*1024**3)
  def execute(argv,name):
    evidence['commands'].append(run(guest_command(argv),log,stdout_path=work/name))
    return work/name
  owned=work.parent
  exported=json.loads((owned/'native-provider-export.json').read_text())
  helper=work/'project-target/debug/build-runtime-ffmpeg'
  command=[helper,'--prepare-released-prefix','--header-plan',f'{source_mount}/rust/native-ffmpeg-headers.json',
      '--provider-plan',f'{source_mount}/rust/native-ffmpeg-provider.json',
      '--descriptor',Path(guest_work)/'provider-export/provider.json','--descriptor-sha256',exported['descriptor_sha256'],
      '--header-cache',Path(guest_work)/'ffmpeg-source','--provider-cache',Path(guest_work)/'provider-assets',
      '--store',work/'native-ffmpeg-prefixes']
  prefix=json.loads(execute(command,'native-encoder-prefix-initial.json').read_text())
  host=json.loads((owned/'native-ffmpeg-prefix-initial.json').read_text())
  assert prefix['tree_sha256']==host['tree_sha256'] and prefix['identity']==host['identity']
  assert prefix['linked_bytes']>0 and not prefix['compiler_executed'] and not prefix['runtime_activated']
  for part in ('headers','provider'):
    assert prefix[part]['asset']['transferred_bytes']==prefix[part]['asset']['stored_bytes']==0
  sdk=json.loads((work/'native-cereal-preparation-initial.json').read_text())
  registry=json.loads((work/'native-cereal-cargo-inputs-reuse.json').read_text())
  yuv=json.loads((work/'native-codec-initial.json').read_text())['libyuv']
  rust=json.loads((owned/'native-rust-first.json').read_text())
  rust_root=Path(guest_work)/'pinned-rust'/Path(rust['root']).relative_to(owned/'native-rust-toolchains')
  rust_receipt=Path(guest_work)/'pinned-rust'/Path(rust['tree_receipt']).relative_to(owned/'native-rust-toolchains')
  environment=dict(FFMPEG_DIR=prefix['root'],ENCODER_LIBYUV_LIB=str(Path(yuv['root'])/'lib'),
      OPENPILOT_CAPNP=str(Path(sdk['root'])/'usr/bin/capnp'),OPENPILOT_CARGO_VENDOR=registry['vendor'],
      LIBCLANG_PATH='/usr/lib/llvm-18/lib',
      LD_LIBRARY_PATH=f"{prefix['root']}/lib:{sdk['root']}/usr/lib/aarch64-linux-gnu",
      RUSTFLAGS=f"-C link-arg=-Wl,-rpath-link,{prefix['root']}/lib")
  for name in ('CC','CXX','AR','RANLIB'):
    path={'CC':'/usr/bin/gcc','CXX':'/usr/bin/g++','AR':'/usr/bin/ar','RANLIB':'/usr/bin/ranlib'}[name]
    environment[name]=environment[f'HOST_{name}']=path
  inputs={str(rust_receipt):rust['tree_sha256']}
  trees={str(rust_root):str(rust_receipt)}
  for row in (prefix,sdk,registry,yuv):
    inputs[row['tree_receipt']]=row['tree_sha256']
    trees[row['root']]=row['tree_receipt']
  system_receipt=str(Path(guest_work)/'native-system-sdk.json')
  inputs[system_receipt]=digest(work/'native-system-sdk.json')
  trees['/']=system_receipt
  features=['openpilot-encoderd/native','openpilot-encoderd/visionipc-ion','openpilot-msgq/visionipc-ion']
  recipe=work/'native-encoder-recipe.json'
  recipe.write_text(json.dumps(dict(schema=1,target='aarch64-unknown-linux-gnu',
      packages=['openpilot-encoderd','openpilot-msgq'],binaries=['openpilot-encoderd'],features=features,
      inputs=inputs,input_trees=trees,environment=environment,expected_growth_bytes=3*1024**3))+'\n')
  driver=work/'project-target/debug/openpilot-runtime-build'
  build=[driver,'--root',source_mount,'--recipe',recipe,'--cache',work/'native-encoder-cache',
      '--toolchain',rust_root/'bin']
  initial=json.loads(execute(build,'native-encoder-build-initial.json').read_text())
  assert initial['source']['commit']==evidence['source_commit']
  assert len(initial['artifacts'])==1 and not initial['artifacts'][0]['fresh']
  binary=initial['artifacts'][0]
  assert Path(binary['executable']).name=='openpilot-encoderd'
  assert set(initial['verified_input_trees'])==set(trees)
  linker=execute(['readlink','-f','/usr/bin/cc'],'native-encoder-linker.txt').read_text().strip()
  example_environment={**environment,'RUSTC':str(rust_root/'bin/rustc'),
      'CARGO_ENCODED_RUSTFLAGS':'\x1f'.join([*environment['RUSTFLAGS'].split(),'-C',f'linker={linker}']),
      'CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER':linker}
  env=['/usr/bin/env',*(f'{key}={value}' for key,value in example_environment.items())]
  example_output=execute([*env,rust_root/'bin/cargo','build','--locked','--offline','--release',
      '--manifest-path',f'{source_mount}/rust/Cargo.toml','--target','aarch64-unknown-linux-gnu',
      '-p','openpilot-encoderd','-p','openpilot-msgq','--features',','.join(features),'--example','encoder_codec_trace',
      '--target-dir',initial['target_directory'],'--message-format=json'], 'native-encoder-example-build.jsonl')
  rows=[json.loads(line) for line in example_output.read_text().splitlines()]
  examples=[row['executable'] for row in rows if row.get('reason')=='compiler-artifact'
      and row.get('executable') and row['target']['name']=='encoder_codec_trace']
  jpeg=[str(Path(row['out_dir'])/'jpeg') for row in rows if row.get('reason')=='build-script-executed'
      and Path(row['out_dir']).parent.name.startswith('openpilot-jpeg-')]
  assert len(examples)==len(jpeg)==1
  execute([*env,'/usr/bin/python3',f'{source_mount}/rust/tools/check_encoder_codecs.py',
      '--rust',examples[0],'--jpeg-build',jpeg[0],'--ffmpeg-prefix',prefix['root'],
      '--libyuv-prefix',yuv['root'],'--scope','agnos-arm-userspace','--timeout','600',
      '--output',work/'native-encoder-codec-reference'],'native-encoder-codec-result.json')
  oracle=json.loads((work/'native-encoder-codec-reference/receipt.json').read_text())
  assert oracle['status']=='PASS' and len(oracle['results'])==5
  assert not oracle['physical_device_execution']
  audit=work/'native-encoder-elf-audit.json'
  execute([work/'project-target/debug/runtime-elf-audit','--root',binary['executable'],
      '--root',examples[0],'--root',work/'native-encoder-codec-reference/source-oracle',
      '--library',Path(prefix['root'])/'lib','--library','/usr/lib/aarch64-linux-gnu',
      '--library','/lib/aarch64-linux-gnu','--output',audit],'native-encoder-audit-summary.json')
  assert json.loads(audit.read_text())['compatible']
  repeated=json.loads(execute(build,'native-encoder-build-reused.json').read_text())
  assert repeated['source']==initial['source'] and repeated['cache_key']==initial['cache_key']
  assert len(repeated['artifacts'])==1 and repeated['artifacts'][0]['fresh']
  assert repeated['artifacts'][0]['sha256']==binary['sha256']
  after=json.loads(execute(command,'native-encoder-prefix-reused.json').read_text())
  assert after['tree_sha256']==prefix['tree_sha256'] and after['copied_bytes']==after['linked_bytes']==0
  assert all(after[p]['copied_bytes']==after[p]['asset']['transferred_bytes']==after[p]['asset']['stored_bytes']==0
      for p in ('headers','provider'))
  evidence['native_rust_encoder']=dict(source_commit=initial['source']['commit'],
      binary_sha256=binary['sha256'],features=features,recipe_sha256=initial['recipe_sha256'],
      prefix_tree_sha256=prefix['tree_sha256'],independent_host_tree_matches=True,
      exact_input_trees_verified=len(trees),repeated_runtime_binary_fresh=True,
      original_source_codec_cases=5,packet_bytes_and_metadata_equal=True,
      oracle_receipt_sha256=digest(work/'native-encoder-codec-reference/receipt.json'),
      strict_elf_audit_compatible=True,actual_arm_native_compiler=True,
      execution_mode=NATIVE_PROOT['MODE'], native_arm_execution=True, emulated=False,full_runtime_build=False,
      runtime_activated=False,daemon_started=False,physical_device_execution=False)


def native_eigen_probe(guest_command,work,source_mount,evidence,log,guest_work):
  """Consume the original header-only dependency, without a Python shim."""
  host=json.loads((work.parent/'native-eigen-initial.json').read_text())['eigen']
  room(work,host['copied_bytes']+16*1024**2)
  def execute(argv,name):
    evidence['commands'].append(run(guest_command(argv),log,stdout_path=work/name))
    return work/name
  preparer=work/'project-target/debug/stage-encoder-target'
  command=[preparer,'--prepare-wheels','--packages','eigen','--root',source_mount,
      '--cache',Path(guest_work)/'eigen-assets','--store',work/'native-eigen-prefixes']
  reports=[]
  for phase in ('initial','reused'):
    report=json.loads(execute(command,f'native-eigen-{phase}.json').read_text())
    assert set(report)=={'eigen'}
    row=report['eigen']
    assert row['tree_sha256']==host['tree_sha256'] and row['identity']==host['identity']
    assert row['asset']['transferred_bytes']==row['asset']['stored_bytes']==0
    assert (row['copied_bytes']>0)==(phase=='initial')
    assert not row['compiler_executed'] and not row['runtime_activated']
    reports.append(row)
  include=Path(reports[0]['root'])
  source=work/'native-eigen-ldlt.cc'
  source.write_text('''#include <eigen3/Eigen/Dense>
#include <cstdio>
#include <cstdlib>
#if EIGEN_WORLD_VERSION != 3 || EIGEN_MAJOR_VERSION != 4 || EIGEN_MINOR_VERSION != 0
#error Native_Eigen_version_differs_from_original_3_4_0
#endif
int main(int argc,char** argv){
  if(argc!=5) return 22;
  Eigen::Matrix2d matrix; matrix << std::strtod(argv[1],nullptr),0.0,0.0,std::strtod(argv[2],nullptr);
  Eigen::Vector2d rhs; rhs << std::strtod(argv[3],nullptr),std::strtod(argv[4],nullptr);
  const Eigen::Vector2d result=matrix.ldlt().solve(rhs);
  if(result[0]!=3.0 || result[1]!=5.0 || (matrix*result-rhs).cwiseAbs().maxCoeff()!=0.0) return 21;
  std::puts("native Eigen 3.4.0 LDLT matched");
}
''')
  room(work,128*1024**2)
  binary=work/'native-eigen-ldlt'
  execute(['g++','-std=c++17','-O1','-I',include,source,'-o',binary],'native-eigen-ldlt-build.txt')
  # Compile the actual existing helper consumed by both native Rust estimators.
  # This is a header/API compile check, not their complete numerical regression.
  helper_source=f'{source_mount}/rednose_repo/rednose/helpers/ekf_sym.cc'
  helper=work/'native-eigen-ekf-sym.o'
  execute(['g++','-std=c++17','-O0','-fPIC','-I',include,'-I',f'{source_mount}/rednose_repo',
      '-I',f'{source_mount}/rednose_repo/rednose','-c',helper_source,'-o',helper],
      'native-eigen-ekf-sym-build.txt')
  with helper.open('rb') as stream: header=stream.read(64)
  assert header[:6]==b'\x7fELF\x02\x01' and int.from_bytes(header[16:18],'little')==1 and int.from_bytes(header[18:20],'little')==183
  audit=work/'native-eigen-elf-audit.json'
  execute([work/'project-target/debug/runtime-elf-audit','--root',binary,
      '--library','/usr/lib/aarch64-linux-gnu','--library','/lib/aarch64-linux-gnu','--output',audit],
      'native-eigen-audit-summary.json')
  assert json.loads(audit.read_text())['compatible']
  assert execute([binary,'2','4','6','20'],'native-eigen-ldlt-run.txt').read_text()=='native Eigen 3.4.0 LDLT matched\n'
  after=json.loads(execute(command,'native-eigen-after-execution.json').read_text())['eigen']
  assert after['tree_sha256']==host['tree_sha256'] and after['copied_bytes']==after['asset']['transferred_bytes']==after['asset']['stored_bytes']==0
  evidence['native_eigen_headers']=dict(wrapper_commit='40e5d76de1b33a86c5181b63db6782d8f06da1da',
      archive_sha256=host['asset']['sha256'],archive_bytes=2250657,tree_sha256=host['tree_sha256'],
      actual_arm_preparer=True,independent_host_tree_matches=True,repeated_body_bytes=0,repeated_copied_bytes=0,
      existing_ekf_helper_source_sha256=digest(Path(os.environ['GITHUB_WORKSPACE'])/'rednose_repo/rednose/helpers/ekf_sym.cc'),
      existing_ekf_helper_object_sha256=digest(helper),basic_ldlt_source_sha256=digest(source),
      basic_ldlt_binary_sha256=digest(binary),strict_elf_audit_compatible=True,
      actual_native_cpp_compile_and_execution=True,complete_estimator_regression=False,
      full_runtime_build=False,physical_device_execution=False)


def native_raylib_probe(guest_command,work,source_mount,evidence,log,guest_work):
  """Verify current loader headers/exports and static ABI, without GPU calls."""
  host=json.loads((work.parent/'native-raylib-sdk-initial.json').read_text())
  exported=json.loads((work.parent/'native-raylib-provider-export.json').read_text())
  host_provider=json.loads((work.parent/'native-raylib-provider-initial.json').read_text())
  room(work,host['copied_bytes']+host_provider['copied_bytes']+128*1024**2)
  def execute(argv,name):
    evidence['commands'].append(run(guest_command(argv),log,stdout_path=work/name))
    return work/name
  execute(['cargo','build','--locked','--manifest-path',f'{source_mount}/rust/Cargo.toml',
      '-p','openpilot-dev-tools','--bin','stage-startup-ui-raylib','--target-dir',work/'project-target',
      '--message-format=json'],'native-raylib-preparer-build.jsonl')
  sdk_command=[work/'project-target/debug/stage-startup-ui-raylib','--prepare-native-inputs',
      '--root',source_mount,'--cache',Path(guest_work)/'raylib-assets','--store',work/'native-raylib-sdk-prefixes']
  provider_command=[work/'project-target/debug/build-runtime-ffmpeg','--prepare-released-provider',
      '--plan',f'{source_mount}/rust/native-raylib-provider.json',
      '--descriptor',Path(guest_work)/'raylib-provider-export/provider.json',
      '--descriptor-sha256',exported['descriptor_sha256'],'--cache',Path(guest_work)/'raylib-components',
      '--store',work/'native-raylib-provider-prefixes']
  sdk_rows=[]
  provider_rows=[]
  for phase in ('initial','reused'):
    sdk=json.loads(execute(sdk_command,f'native-raylib-sdk-{phase}.json').read_text())
    provider=json.loads(execute(provider_command,f'native-raylib-provider-{phase}.json').read_text())
    assert sdk['tree_sha256']==host['tree_sha256'] and sdk['identity']==host['identity']
    assert provider['tree_sha256']==host_provider['tree_sha256']==exported['tree_sha256']
    assert provider['provider_source_commit']==exported['provider_source_commit']
    for row in (sdk,provider):
      assert row['asset']['transferred_bytes']==row['asset']['stored_bytes']==0
      assert (row['copied_bytes']>0)==(phase=='initial')
      assert not row['compiler_executed'] and not row['runtime_activated']
    assert provider['offline_seed_bytes']==0 and not provider['origin_archive_downloaded']
    sdk_rows.append(sdk)
    provider_rows.append(provider)
  include=Path(sdk_rows[0]['root'])/'include'
  prefix=Path(provider_rows[0]['root'])
  # Convert guest paths only for the host's bounded, read-only source inspection.
  def local(path):
    return work/Path(path).relative_to(guest_work)
  original_header=prefix/'share/licenses/raylib/raylib.h'
  assert digest(local(include/'raylib.h'))==digest(local(original_header))=='047e7255f93f8c34039cab906ad76136706b5c7b4c5b5b065d84141963ee9b6b'
  relative='rust/crates/startup-ui/native/raylib_loader.cc'
  loader=Path(os.environ['GITHUB_WORKSPACE'])/relative
  source=loader.read_text()
  required_block=re.search(r'const\s+char\s*\*\s*required\[\]\s*=\s*\{(.*?)\};',source,re.S)
  assert required_block, 'current startup loader symbol contract was not found'
  required=set(re.findall(r'"([A-Za-z_][A-Za-z_0-9]*)"',required_block.group(1)))
  forwarded=set(re.findall(r'^FORWARD\([^,]+,\s*([A-Za-z_][A-Za-z_0-9]*)\s*,',source,re.M))
  assert required and forwarded
  required|=forwarded|{'openpilot_startup_ui_raylib_contract'}
  library=prefix/'lib/libopenpilot-raylib.so'
  symbols=execute(['readelf','--wide','--dyn-syms',library],'native-raylib-symbols.txt').read_text()
  defined=set()
  for line in symbols.splitlines():
    fields=line.split()
    if len(fields)>=8 and fields[0].endswith(':') and fields[4] in ('GLOBAL','WEAK') and fields[5] in ('DEFAULT','PROTECTED') and fields[6]!='UND':
      defined.add(fields[7].split('@')[0])
  assert required<=defined, f'current raylib loader exports missing: {sorted(required-defined)}'
  output=work/'native-raylib-loader.o'
  execute(['g++','-std=c++17','-O0','-fPIC','-I',include,'-c',f'{source_mount}/{relative}',
      '-o',output],'native-raylib-loader-build.txt')
  with output.open('rb') as stream: header=stream.read(64)
  assert header[:6]==b'\x7fELF\x02\x01' and int.from_bytes(header[16:18],'little')==1 and int.from_bytes(header[18:20],'little')==183
  audit=work/'native-raylib-elf-audit.json'
  execute([work/'project-target/debug/runtime-elf-audit','--root',library,
      '--library',prefix/'lib','--library','/usr/lib/aarch64-linux-gnu',
      '--library','/lib/aarch64-linux-gnu','--library','/usr/lib','--output',audit],
      'native-raylib-audit-summary.json')
  assert json.loads(audit.read_text())['compatible']
  for name,command,first in (('sdk',sdk_command,sdk_rows[0]),('provider',provider_command,provider_rows[0])):
    after=json.loads(execute(command,f'native-raylib-{name}-after-inspection.json').read_text())
    assert after['tree_sha256']==first['tree_sha256']
    assert after['copied_bytes']==after['asset']['transferred_bytes']==after['asset']['stored_bytes']==0
  evidence['native_raylib_inputs']=dict(sdk_tree_sha256=sdk_rows[0]['tree_sha256'],
      archive_sha256=host['asset']['sha256'],archive_bytes=21707647,
      provider_tree_sha256=provider_rows[0]['tree_sha256'],provider_source_commit=exported['provider_source_commit'],
      component_sha256=provider_rows[0]['asset']['sha256'],component_bytes=provider_rows[0]['asset']['bytes'],
      descriptor_sha256=exported['descriptor_sha256'],library_sha256=digest(local(library)),
      current_loader_source_sha256=digest(loader),current_loader_object_sha256=digest(output),
      current_loader_required_exports=len(required),current_loader_forwarded_exports=len(forwarded),
      original_header_matches=True,strict_elf_audit_compatible=True,actual_arm_preparer=True,
      independent_host_trees_match=True,repeated_body_bytes=0,repeated_copied_bytes=0,
      origin_archive_downloaded_by_consumer=False,project_binaries_reused=False,
      offline_component_seed=True,published_release=False,public_component_download_tested=False,
      library_loaded=False,graphics_execution_tested=False,full_runtime_build=False,physical_device_execution=False)


def native_opencv_headers_probe(guest_command,work,source_mount,evidence,log,guest_work):
  """Compile against the exact original installed headers; do not load OpenCV."""
  host=json.loads((work.parent/'native-opencv-headers-initial.json').read_text())
  room(work,host['copied_bytes']+128*1024**2)
  def execute(argv,name):
    evidence['commands'].append(run(guest_command(argv),log,stdout_path=work/name))
    return work/name
  execute(['cargo','build','--locked','--manifest-path',f'{source_mount}/rust/Cargo.toml',
      '-p','openpilot-dev-tools','--bin','build-runtime-opencv','--target-dir',work/'project-target',
      '--message-format=json'],'native-opencv-preparer-build.jsonl')
  command=[work/'project-target/debug/build-runtime-opencv','--prepare-released-headers',
      '--plan',f'{source_mount}/rust/native-opencv-headers.json','--cache',Path(guest_work)/'opencv-source',
      '--store',work/'native-opencv-headers']
  rows=[]
  for phase in ('initial','reused'):
    row=json.loads(execute(command,f'native-opencv-headers-{phase}.json').read_text())
    assert row['tree_sha256']==host['tree_sha256'] and row['identity']==host['identity']
    assert row['public_headers']==170 and not row['provider_libraries_prepared']
    assert row['asset']['transferred_bytes']==row['asset']['stored_bytes']==0
    assert (row['copied_bytes']>0)==row['source_archive_decompressed']==(phase=='initial')
    assert not row['compiler_executed'] and not row['runtime_activated'] and not row['actual_target_execution']
    rows.append(row)
  source=work/'native-opencv-header-api.cc'
  source.write_text('''#include <opencv2/core.hpp>
#include <opencv2/core/version.hpp>
#include <opencv2/cvconfig.h>
#include <opencv2/opencv_modules.hpp>
#include <opencv2/imgproc.hpp>
#include <opencv2/imgcodecs.hpp>
#include <opencv2/videoio.hpp>
#include <opencv2/dnn.hpp>
#if CV_VERSION_MAJOR != 4 || CV_VERSION_MINOR != 13 || CV_VERSION_REVISION != 0
#error Original_OpenCV_version_required
#endif
#if !defined(HAVE_OPENCV_CORE) || !defined(HAVE_OPENCV_DNN) || !defined(HAVE_OPENCV_VIDEOIO) || !defined(HAVE_OPENCV_IMGCODECS) || !defined(HAVE_OPENCV_IMGPROC)
#error Original_five_module_configuration_required
#endif
cv::Mat native_header_api(const cv::Mat& image) {
  cv::Mat gray;
  cv::cvtColor(image,gray,cv::COLOR_BGR2GRAY);
  std::vector<unsigned char> encoded;
  cv::imencode(".png",gray,encoded);
  cv::dnn::Net network;
  cv::VideoCapture camera;
  return gray;
}
''')
  output=work/'native-opencv-header-api.o'
  execute(['g++','-std=c++17','-O0','-fPIC','-I',Path(rows[0]['root'])/'install/include/opencv4',
      '-c',source,'-o',output],'native-opencv-header-api-build.txt')
  with output.open('rb') as stream: header=stream.read(64)
  assert header[:6]==b'\x7fELF\x02\x01' and int.from_bytes(header[16:18],'little')==1 and int.from_bytes(header[18:20],'little')==183
  after=json.loads(execute(command,'native-opencv-headers-after-compilation.json').read_text())
  assert after['tree_sha256']==rows[0]['tree_sha256'] and not after['source_archive_decompressed']
  assert after['copied_bytes']==after['asset']['transferred_bytes']==after['asset']['stored_bytes']==0
  evidence['native_opencv_headers']=dict(tree_sha256=rows[0]['tree_sha256'],public_headers=170,
      source_archive_sha256=host['asset']['sha256'],source_archive_bytes=95450266,
      original_generated_headers=2,original_hardening_preserved=True,actual_arm_preparer=True,
      independent_host_tree_matches=True,repeated_body_bytes=0,repeated_copied_bytes=0,
      repeated_source_decompression=False,header_api_source_sha256=digest(source),
      header_api_object_sha256=digest(output),actual_arm_header_compile=True,
      provider_libraries_prepared=False,provider_link_or_execution_tested=False,
      complete_consumer_regression=False,full_runtime_build=False,physical_device_execution=False)


def native_opencv_prefix_probe(guest_command,work,source_mount,evidence,log,guest_work):
  """Use the real Rust bridge and original shared libraries in ARM userspace."""
  room(work,1024**3)
  def execute(argv,name):
    evidence['commands'].append(run(guest_command(argv),log,stdout_path=work/name))
    return work/name
  owned=work.parent
  host=json.loads((owned/'native-opencv-prefix-initial.json').read_text())
  exported=json.loads((owned/'native-opencv-provider-export.json').read_text())
  command=[work/'project-target/debug/build-runtime-opencv','--prepare-released-prefix',
      '--header-plan',f'{source_mount}/rust/native-opencv-headers.json',
      '--provider-plan',f'{source_mount}/rust/native-opencv-provider.json',
      '--descriptor',Path(guest_work)/'opencv-provider-export/provider.json',
      '--descriptor-sha256',exported['descriptor_sha256'],'--header-cache',Path(guest_work)/'opencv-source',
      '--provider-cache',Path(guest_work)/'opencv-components','--store',work/'native-opencv-prefixes']
  initial=json.loads(execute(command,'native-opencv-prefix-initial.json').read_text())
  assert initial['tree_sha256']==host['tree_sha256'] and initial['identity']==host['identity']
  assert initial['provider']['tree_sha256']==exported['tree_sha256']
  assert initial['linked_bytes']>0 and not initial['compiler_executed'] and not initial['runtime_activated']
  for part in ('headers','provider'):
    assert initial[part]['asset']['transferred_bytes']==initial[part]['asset']['stored_bytes']==0
  prefix=Path(initial['root'])
  libraries=prefix/'install/lib'
  ffmpeg=Path(json.loads((work/'native-encoder-prefix-initial.json').read_text())['root'])
  capnp=Path(json.loads((work/'native-cereal-preparation-initial.json').read_text())['root'])
  rust=json.loads((owned/'native-rust-first.json').read_text())
  rust_root=Path(guest_work)/'pinned-rust'/Path(rust['root']).relative_to(owned/'native-rust-toolchains')
  linker=execute(['readlink','-f','/usr/bin/cc'],'native-opencv-linker.txt').read_text().strip()
  environment=dict(OPENPILOT_OPENCV_ROOT=str(prefix),OPENPILOT_WEBCAM_OPENCV_ROOT=str(prefix),
      FFMPEG_DIR=str(ffmpeg),RUSTC=str(rust_root/'bin/rustc'),
      LD_LIBRARY_PATH=f'{libraries}:{ffmpeg}/lib:{capnp}/usr/lib/aarch64-linux-gnu',
      CARGO_ENCODED_RUSTFLAGS='\x1f'.join(['-C',f'linker={linker}','-C',f'link-arg=-Wl,-rpath-link,{libraries}',
          '-C',f'link-arg=-Wl,-rpath-link,{ffmpeg}/lib']),
      CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=linker)
  for key,value in dict(CC='/usr/bin/gcc',CXX='/usr/bin/g++',AR='/usr/bin/ar',RANLIB='/usr/bin/ranlib').items():
    environment[key]=environment[f'HOST_{key}']=value
  env=['/usr/bin/env',*(f'{key}={value}' for key,value in environment.items())]
  build=[*env,rust_root/'bin/cargo','build','--release','--locked','--offline',
      '--manifest-path',f'{source_mount}/rust/Cargo.toml','--target','aarch64-unknown-linux-gnu',
      '-p','openpilot-opencv-runtime','--features','native-skip-miri','--example','opencv_trace',
      '--target-dir',work/'native-opencv-target','--message-format=json']
  messages=execute(build,'native-opencv-bridge-build.jsonl')
  artifacts=[row for line in messages.read_text().splitlines() if (row:=json.loads(line)).get('reason')=='compiler-artifact'
      and row.get('executable') and row['target']['name']=='opencv_trace']
  assert len(artifacts)==1 and not artifacts[0]['fresh']
  binary=artifacts[0]['executable']
  local_binary=work/Path(binary).relative_to(guest_work)
  initial_binary_sha256=digest(local_binary)
  source=work/'native-opencv-provider-abi.cc'
  source.write_text('''#include <opencv2/core.hpp>
#include <opencv2/imgproc.hpp>
#include <opencv2/imgcodecs.hpp>
#include <opencv2/videoio.hpp>
#include <opencv2/dnn.hpp>
#include <cstdio>
#include <fstream>
int main(int argc,char** argv){
  if(argc!=2) return 10;
  if(cv::getVersionString()!="4.13.0") return 11;
  cv::Mat image(1,3,CV_8UC3);
  image.at<cv::Vec3b>(0,0)=cv::Vec3b(255,0,0);
  image.at<cv::Vec3b>(0,1)=cv::Vec3b(0,255,0);
  image.at<cv::Vec3b>(0,2)=cv::Vec3b(0,0,255);
  cv::Mat gray;
  cv::cvtColor(image,gray,cv::COLOR_BGR2GRAY);
  if(gray.at<unsigned char>(0,0)!=29 || gray.at<unsigned char>(0,1)!=150 || gray.at<unsigned char>(0,2)!=76) return 12;
  std::vector<unsigned char> bmp;
  if(!cv::imencode(".bmp",image,bmp)) return 13;
  auto decoded=cv::imdecode(bmp,cv::IMREAD_UNCHANGED);
  if(decoded.size()!=image.size() || decoded.type()!=image.type() || cv::norm(decoded,image,cv::NORM_INF)!=0) return 14;
  cv::dnn::Net net;
  cv::VideoCapture capture;
  if(!net.empty() || capture.isOpened()) return 15;
  std::ifstream maps("/proc/self/maps");
  std::ofstream output(argv[1]);
  output << maps.rdbuf();
  if(!output) return 16;
  std::puts("native OpenCV 4.13.0 ABI matched");
}
''')
  program=work/'native-opencv-provider-abi'
  execute(['g++','-std=c++17','-I',prefix/'install/include/opencv4',source,'-L',libraries,
      f'-Wl,-rpath-link,{libraries}',f'-Wl,-rpath-link,{ffmpeg}/lib','-lopencv_videoio','-lopencv_imgcodecs',
      '-lopencv_dnn','-lopencv_imgproc','-lopencv_core','-o',program],'native-opencv-provider-abi-build.txt')
  audit=work/'native-opencv-elf-audit.json'
  roots=[program,binary,*(libraries/f'libopencv_{name}.so.4.13.0' for name in ('core','imgproc','imgcodecs','videoio','dnn'))]
  execute([work/'project-target/debug/runtime-elf-audit',*(arg for root in roots for arg in ('--root',root)),
      '--library',libraries,'--library',ffmpeg/'lib','--library','/usr/lib/aarch64-linux-gnu',
      '--library','/lib/aarch64-linux-gnu','--output',audit],'native-opencv-audit-summary.json')
  assert json.loads(audit.read_text())['compatible']
  provider_maps=work/'native-opencv-provider-maps.txt'
  assert execute([*env,program,provider_maps],'native-opencv-provider-abi-run.txt').read_text()=='native OpenCV 4.13.0 ABI matched\n'
  def verify_maps(maps,names):
    assert 'libpython' not in maps and 'cv2.abi3.so' not in maps
    paths={row[5] for line in maps.splitlines() if len(row:=line.split(maxsplit=5))==6}
    for name in names:
      path=libraries/f'libopencv_{name}.so.4.13.0'
      local=work/path.relative_to(guest_work)
      # PRoot's /proc content may expose either the guest path or host bind path.
      assert {str(path),str(local.resolve())}&paths, f'unexpected mapped OpenCV provider: {name}'
  verify_maps(provider_maps.read_text(),('core','imgproc','imgcodecs','videoio','dnn'))
  cases=work/'native-opencv-bridge-cases'
  cases.mkdir()
  (cases/'bgr.bin').write_bytes(bytes([255,0,0,0,255,0,0,0,255]))
  (cases/'gray.bin').write_bytes(bytes([17]))
  def guest_path(path): return str(Path(guest_work)/path.relative_to(work))
  requests=[('bgr-gray',dict(op='bgr_gray',input=guest_path(cases/'bgr.bin'),width=3,height=1),bytes([29,150,76]),(3,1)),
      ('resize',dict(op='resize',input=guest_path(cases/'gray.bin'),width=1,height=1,format='gray',output_width=3,output_height=2),bytes([17])*6,(3,2)),
      ('bounds',dict(op='bounds',points=[[1,2],[3,4],[-1,5]]),None,None)]
  for name,request,expected,extent in requests:
    path=cases/f'{name}-input.json'
    path.write_text(json.dumps(request)+'\n')
    output=cases/f'{name}-output.json'
    execute([*env,binary,path,output],f'native-opencv-bridge-{name}-run.txt')
    actual=json.loads(output.read_text())
    if expected is None:
      assert actual==dict(kind='bounds',x=-1,y=2,width=5,height=4)
    else:
      assert actual['kind']=='image' and (actual['width'],actual['height'])==extent
      assert (work/Path(actual['file']).relative_to(guest_work)).read_bytes()==expected
    maps=output.with_suffix('.maps.txt').read_text()
    verify_maps(maps,('core','imgproc','dnn'))
  repeated=execute(build,'native-opencv-bridge-build-reused.jsonl')
  fresh=[row for line in repeated.read_text().splitlines() if (row:=json.loads(line)).get('reason')=='compiler-artifact'
      and row.get('executable') and row['target']['name']=='opencv_trace']
  assert len(fresh)==1 and fresh[0]['fresh'] and fresh[0]['executable']==binary
  final=json.loads(execute(command,'native-opencv-prefix-reused.json').read_text())
  assert final['tree_sha256']==initial['tree_sha256'] and final['copied_bytes']==final['linked_bytes']==0
  for part in ('headers','provider'):
    assert final[part]['copied_bytes']==final[part]['asset']['transferred_bytes']==final[part]['asset']['stored_bytes']==0
  assert not final['headers']['source_archive_decompressed']
  assert digest(local_binary)==initial_binary_sha256
  evidence['native_opencv_prefix']=dict(tree_sha256=initial['tree_sha256'],provider_tree_sha256=exported['tree_sha256'],
      descriptor_sha256=exported['descriptor_sha256'],component_sha256=initial['provider']['asset']['sha256'],
      component_bytes=initial['provider']['asset']['bytes'],strict_elf_audit_compatible=True,
      original_shared_libraries=5,actual_rust_bridge_compiled_and_executed=True,rust_primitive_cases=3,
      rust_example_sha256=digest(local_binary),provider_abi_source_sha256=digest(source),provider_abi_binary_sha256=digest(program),
      repeated_cargo_artifact_fresh=True,repeated_http_body_bytes=0,repeated_copied_bytes=0,repeated_linked_bytes=0,
      repeated_source_decompression=False,python_shim_loaded=False,origin_archive_downloaded_by_consumer=False,
      project_binaries_reused=False,offline_component_seed=True,published_release=False,
      public_component_download_tested=False,real_model_regression=False,complete_webcam_xiaoge_regression=False,
      full_runtime_build=False,physical_device_execution=False)


def native_runtime_companions_probe(guest_command,work,source_mount,evidence,log,guest_work):
  """Consume independent components, then execute actual foreign APIs on ARM."""
  source=Path(os.environ['GITHUB_WORKSPACE'])
  names=('numerics','geos','brotli','acados')
  payload=sum(sum(row['bytes'] for row in json.loads((source/f'rust/native-{name}-provider.json').read_text())['members'].values()) for name in names)
  room(work,payload+128*1024**2)
  (work/'native-companion-prefixes').mkdir()
  def execute(argv,name):
    evidence['commands'].append(run(guest_command(argv),log,stdout_path=work/name))
    return work/name
  runtime_inputs=dict(files={},trees={})
  roots={}
  reports={}
  libraries=[]
  elf_roots=[]
  commands=[]
  for name in names:
    host=json.loads((work.parent/f'native-companion-{name}-initial.json').read_text())
    exported=json.loads((work.parent/f'native-companion-{name}-export.json').read_text())
    command=[work/'project-target/debug/build-runtime-ffmpeg','--prepare-released-provider',
        '--plan',f'{source_mount}/rust/native-{name}-provider.json',
        '--descriptor',Path(guest_work)/'companion-exports'/name/'provider.json',
        '--descriptor-sha256',exported['descriptor_sha256'],
        '--cache',Path(guest_work)/'companion-components'/name,
        '--store',work/'native-companion-prefixes'/name]
    command_record=[]
    for phase in ('initial','reused'):
      row=json.loads(execute(command,f'native-companion-{name}-{phase}.json').read_text())
      assert row['tree_sha256']==host['tree_sha256']==exported['tree_sha256']
      assert row['offline_seed_bytes']==row['asset']['transferred_bytes']==row['asset']['stored_bytes']==0
      assert (row['copied_bytes']>0)==(phase=='initial')
      assert not row['origin_archive_downloaded'] and not row['compiler_executed'] and not row['runtime_activated']
      command_record.append(row)
    assert command_record[0]['root']==command_record[1]['root']
    row=command_record[0]
    runtime_inputs['files'][row['tree_receipt']]=row['tree_sha256']
    runtime_inputs['trees'][row['root']]=row['tree_receipt']
    leaf={'numerics':'numerics','geos':'carrot-geos','brotli':'brotli','acados':'plannerd-acados'}[name]
    roots[name]=Path(row['root'])/leaf
    libraries.append(roots[name])
    plan=json.loads((source/f'rust/native-{name}-provider.json').read_text())
    for path,member in plan['members'].items():
      if '.so' in Path(path).name and member['link'] is None:
        elf_roots.append(Path(row['root'])/path)
    reports[name]=dict(tree_sha256=row['tree_sha256'],descriptor_sha256=exported['descriptor_sha256'],
        repeated_http_body_bytes=0,repeated_copied_bytes=0,consumer_origin_archive_downloaded=False)
    commands.append((name,command,row))
  configuration=work/'native-companion-runtime-inputs.json'
  configuration.write_text(json.dumps(runtime_inputs,indent=2)+'\n')
  fixture=work/'native-companion-probe.cc'
  shutil.copyfile(source/'tools/ci/fixtures/native-companion-probe.cc',fixture)
  binary=work/'native-companion-probe'
  execute(['g++','-std=c++17','-O2','-Wall','-Wextra','-Werror',fixture,'-ldl','-o',binary],'native-companion-compile.txt')
  header=binary.read_bytes()[:64]
  assert header[:6]==b'\x7fELF\x02\x01' and int.from_bytes(header[18:20],'little')==183
  loader=':'.join(map(str,[*libraries,'/usr/lib/aarch64-linux-gnu','/lib/aarch64-linux-gnu']))
  maps_file=work/'native-companion-provider-maps.txt'
  actual=json.loads(execute(['/usr/bin/env',f'LD_LIBRARY_PATH={loader}',binary,*(roots[name] for name in names),maps_file],'native-companion-api.json').read_text())
  assert actual==dict(numerics_dot_matrix_svd=True,geos_interpolation=True,brotli_roundtrip=True,
      acados_core_symbols=True,mpc_solver_tested=False)
  assert maps_file.stat().st_size<8*1024**2
  maps=maps_file.read_text()
  assert 'libpython' not in maps
  mapped_paths={row[5] for line in maps.splitlines() if len(row:=line.split(maxsplit=5))==6}
  for name,files in {
      'numerics':('libgfortran-e1b7dfc8-d8198c01.so.5.0.0','libscipy_openblas64_-f552eb69.so'),
      'geos':('libgeos-9c5cb28e.so.3.13.1','libgeos_c-42bdf9a9.so.1.19.2'),
      'brotli':('libbrotlicommon.so.1','libbrotlienc.so.1','libbrotlidec.so.1'),
      'acados':('libacados.so',),
  }.items():
    for filename in files:
      guest_path=roots[name]/filename
      host_path=work/guest_path.relative_to(guest_work)
      assert {str(guest_path),str(host_path.resolve())}&mapped_paths, f'wrong mapped component: {name}/{filename}'
  audit=work/'native-companion-elf-audit.json'
  command=[work/'project-target/debug/runtime-elf-audit','--root',binary]
  for path in elf_roots: command += ['--root',path]
  for path in [*libraries,'/usr/lib/aarch64-linux-gnu','/lib/aarch64-linux-gnu']: command += ['--library',path]
  command += ['--output',audit]
  execute(command,'native-companion-elf-summary.json')
  assert json.loads(audit.read_text())['compatible']
  for name,command,before in commands:
    after=json.loads(execute(command,f'native-companion-{name}-after-api.json').read_text())
    assert after['tree_sha256']==before['tree_sha256'] and after['copied_bytes']==after['asset']['transferred_bytes']==after['asset']['stored_bytes']==0
  evidence['native_runtime_companions']=dict(components=reports,numerics_component_count=1,
      intended_numerics_runtime_destinations=4,foreign_api_probe=actual,
      probe_source_sha256=digest(fixture),probe_binary_sha256=digest(binary),strict_elf_audit_compatible=True,
      actual_provider_maps_verified=True,python_library_mapped=False,
      actual_arm_preparer=True,actual_arm_cpp_compile_and_execution=True,
      runtime_inputs_trees=len(runtime_inputs['trees']),project_generated_mpc_solvers_reused=False,
      full_mpc_regression=False,full_module_regression=False,runtime_assets_complete=False,
      full_runtime_build=False,physical_device_execution=False,
      execution_mode=NATIVE_PROOT['MODE'], native_arm_execution=True, emulated=False)


def native_display_inputs_probe(guest_command,work,source_mount,evidence,log,guest_work):
  """Consume the remaining root libraries and exercise their actual Rust APIs."""
  room(work,2*1024**3)
  (work/'native-display-prefixes').mkdir()
  def execute(argv,name):
    evidence['commands'].append(run(guest_command(argv),log,stdout_path=work/name))
    return work/name
  source=Path(os.environ['GITHUB_WORKSPACE'])
  inputs=json.loads((work/'native-companion-runtime-inputs.json').read_text())
  roots={}
  reports={}
  commands={}
  for name in ('sdl','turbojpeg'):
    host=json.loads((work.parent/f'native-companion-{name}-initial.json').read_text())
    exported=json.loads((work.parent/f'native-companion-{name}-export.json').read_text())
    command=[work/'project-target/debug/build-runtime-ffmpeg','--prepare-released-provider',
        '--plan',f'{source_mount}/rust/native-{name}-provider.json',
        '--descriptor',Path(guest_work)/'companion-exports'/name/'provider.json',
        '--descriptor-sha256',exported['descriptor_sha256'],
        '--cache',Path(guest_work)/'companion-components'/name,
        '--store',work/'native-display-prefixes'/name]
    rows=[]
    for phase in ('initial','reused'):
      row=json.loads(execute(command,f'native-display-{name}-{phase}.json').read_text())
      assert row['tree_sha256']==host['tree_sha256']==exported['tree_sha256']
      assert row['offline_seed_bytes']==row['asset']['transferred_bytes']==row['asset']['stored_bytes']==0
      assert (row['copied_bytes']>0)==(phase=='initial')
      assert not row['origin_archive_downloaded'] and not row['compiler_executed'] and not row['runtime_activated']
      rows.append(row)
    assert rows[0]['root']==rows[1]['root']
    roots[name]=Path(rows[0]['root'])/'lib'
    inputs['files'][rows[0]['tree_receipt']]=rows[0]['tree_sha256']
    inputs['trees'][rows[0]['root']]=rows[0]['tree_receipt']
    reports[name]=dict(tree_sha256=rows[0]['tree_sha256'],descriptor_sha256=exported['descriptor_sha256'],
        repeated_http_body_bytes=0,repeated_copied_bytes=0,consumer_origin_archive_downloaded=False)
    commands[name]=(command,rows[0])
  assert len(inputs['files'])==len(inputs['trees'])==6
  (work/'native-display-runtime-inputs.json').write_text(json.dumps(inputs,indent=2)+'\n')
  rust=json.loads((work.parent/'native-rust-first.json').read_text())
  rust_root=Path(guest_work)/'pinned-rust'/Path(rust['root']).relative_to(work.parent/'native-rust-toolchains')
  linker=execute(['readlink','-f','/usr/bin/cc'],'native-display-linker.txt').read_text().strip()
  loader=':'.join(map(str,[*roots.values(),'/usr/lib/aarch64-linux-gnu','/lib/aarch64-linux-gnu']))
  environment=dict(RUSTC=str(rust_root/'bin/rustc'),LD_LIBRARY_PATH=loader,
      CARGO_ENCODED_RUSTFLAGS='\x1f'.join(['-C',f'linker={linker}']),
      CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=linker,
      SDL_VIDEODRIVER='dummy',SDL_AUDIODRIVER='dummy',SDL_JOYSTICK_ALLOW_BACKGROUND_EVENTS='1')
  for key,value in dict(CC='/usr/bin/gcc',CXX='/usr/bin/g++',AR='/usr/bin/ar',RANLIB='/usr/bin/ranlib').items():
    environment[key]=environment[f'HOST_{key}']=value
  env=['/usr/bin/env',*(f'{key}={value}' for key,value in environment.items())]
  build_base=[*env,rust_root/'bin/cargo','build','--release','--locked','--offline',
      '--manifest-path',f'{source_mount}/rust/Cargo.toml','--target','aarch64-unknown-linux-gnu',
      '--target-dir',work/'native-display-target','--message-format=json']
  builds={}
  binaries={}
  binary_hashes={}
  for name,package,example,feature in (
      ('sdl','openpilot-sdl2','native_provider','fixture-inspection'),
      ('turbojpeg','openpilot-jpeg','native_turbojpeg','native-skip-miri')):
    command=[*build_base,'-p',package,'--features',feature,'--example',example]
    def artifact(path):
      rows=[row for line in path.read_text().splitlines() if (row:=json.loads(line)).get('reason')=='compiler-artifact'
          and row.get('executable') and row['target']['name']==example]
      assert len(rows)==1
      return rows[0]
    first=artifact(execute(command,f'native-display-{name}-build.jsonl'))
    assert not first['fresh']
    binary=Path(first['executable'])
    local_binary=work/binary.relative_to(guest_work)
    header=local_binary.read_bytes()[:64]
    assert header[:6]==b'\x7fELF\x02\x01' and int.from_bytes(header[18:20],'little')==183
    binaries[name]=binary
    binary_hashes[name]=digest(local_binary)
    builds[name]=(command,example)
  execute([*env,binaries['sdl'],roots['sdl']/'libSDL2-2.0.so.0',
      work/'native-display-sdl-maps.txt'],'native-display-sdl-api.txt')
  fixture=source/'tools/ci/fixtures/native-display-probe.cc'
  binary=work/'native-display-reference'
  execute(['g++','-std=c++17','-O2','-Wall','-Wextra','-Werror',f'{source_mount}/tools/ci/fixtures/native-display-probe.cc',
      '-ldl','-o',binary],'native-display-reference-build.txt')
  cases=work/'native-display-cases'
  cases.mkdir()
  def verify_maps(path,expected):
    maps=path.read_text()
    assert len(maps)<8*1024**2 and 'libpython' not in maps
    mapped={row[5] for line in maps.splitlines() if len(row:=line.split(maxsplit=5))==6}
    for guest_path in expected:
      host_path=work/guest_path.relative_to(guest_work)
      assert {str(guest_path),str(host_path.resolve())}&mapped, f'wrong display provider mapping: {guest_path}'
  verify_maps(work/'native-display-sdl-maps.txt',[roots['sdl']/'libSDL2-2-6e5c51a6.0.so.0.2800.4'])
  matched=[]
  for width,height,quality in ((1,1,1),(2,1,100),(63,35,82),(128,64,75)):
    stem=cases/f'{width}x{height}-q{quality}'
    pixels=stem.with_suffix('.rgba')
    pixels.write_bytes(bytes(index%251 for index in range(width*height*4)))
    reference=stem.with_suffix('.reference.jpg')
    output=stem.with_suffix('.rust.jpg')
    maps=stem.with_suffix('.reference.maps.txt')
    actual=json.loads(execute([*env,binary,roots['sdl']/'libSDL2-2.0.so.0',roots['sdl']/'libSDL2_ttf-2.0.so.0',
        roots['turbojpeg']/'libturbojpeg.so.0',pixels,reference,width,height,quality,maps],
        f'native-display-reference-{width}x{height}-q{quality}.json').read_text())
    assert actual==dict(sdl_version='2.28.4',ttf_version='2.20.1',ttf_init_cleanup=True,turbojpeg_encoded=True)
    execute([*env,binaries['turbojpeg'],roots['turbojpeg']/'libturbojpeg.so.0',pixels,output,width,height,quality],
        f'native-display-turbojpeg-{width}x{height}-q{quality}.txt')
    assert reference.read_bytes()==output.read_bytes()
    verify_maps(output.with_suffix('.maps.txt'),[roots['turbojpeg']/'libturbojpeg.so.0.4.0'])
    expected=[roots['turbojpeg']/'libturbojpeg.so.0.4.0']
    sdl_plan=json.loads((source/'rust/native-sdl-provider.json').read_text())
    expected.extend(roots['sdl']/Path(path).name for path,row in sdl_plan['members'].items()
        if path.startswith('lib/') and row['link'] is None)
    verify_maps(maps,expected)
    matched.append(dict(width=width,height=height,quality=quality,jpeg_sha256=digest(output)))
  audit=work/'native-display-elf-audit.json'
  audit_command=[work/'project-target/debug/runtime-elf-audit','--root',binary]
  for native in binaries.values(): audit_command+=['--root',native]
  for name in roots:
    plan=json.loads((source/f'rust/native-{name}-provider.json').read_text())
    for path,row in plan['members'].items():
      if path.startswith('lib/') and row['link'] is None:
        audit_command+=['--root',roots[name]/Path(path).name]
  for path in (*roots.values(),'/usr/lib/aarch64-linux-gnu','/lib/aarch64-linux-gnu'):
    audit_command+=['--library',path]
  execute([*audit_command,'--output',audit],'native-display-elf-summary.json')
  assert json.loads(audit.read_text())['compatible']
  for name,(command,example) in builds.items():
    data=execute(command,f'native-display-{name}-build-reused.jsonl')
    rows=[row for line in data.read_text().splitlines() if (row:=json.loads(line)).get('reason')=='compiler-artifact'
        and row.get('executable') and row['target']['name']==example]
    assert len(rows)==1 and rows[0]['fresh'] and rows[0]['executable']==str(binaries[name])
    assert digest(work/binaries[name].relative_to(guest_work))==binary_hashes[name]
  for name,(command,before) in commands.items():
    after=json.loads(execute(command,f'native-display-{name}-after-api.json').read_text())
    assert after['tree_sha256']==before['tree_sha256'] and after['copied_bytes']==after['asset']['transferred_bytes']==after['asset']['stored_bytes']==0
  evidence['native_display_inputs']=dict(components=reports,source_commit=evidence['source_commit'],
      remaining_original_root_library_paths=11,foreign_original_root_library_paths_covered=53,
      actual_rust_virtual_device_cycles=20,actual_rust_turbojpeg_library_cycles=80,
      actual_rust_turbojpeg_compressor_cycles=160,exact_jpeg_cases=matched,
      rust_example_sha256=binary_hashes,reference_source_sha256=digest(fixture),reference_binary_sha256=digest(binary),
      sdl_version='2.28.4',ttf_version='2.20.1',ttf_init_cleanup=True,
      strict_elf_audit_compatible=True,actual_provider_maps_verified=True,python_library_mapped=False,
      repeated_cargo_artifacts_fresh=True,repeated_http_body_bytes=0,repeated_copied_bytes=0,
      consumer_origin_archive_downloaded=False,font_glyph_rendering_tested=False,
      hardware_graphics_tested=False,full_module_regression=False,full_runtime_build=False,
      physical_device_execution=False,execution_mode=NATIVE_PROOT['MODE'], native_arm_execution=True, emulated=False)


def native_media_inputs_probe(guest_command,work,source_mount,evidence,log,guest_work):
  """Complete required foreign tools/font inputs and execute their real APIs."""
  room(work,2*1024**3)
  (work/'native-media-prefixes').mkdir()
  def execute(argv,name):
    evidence['commands'].append(run(guest_command(argv),log,stdout_path=work/name))
    return work/name
  def read(name):
    return json.loads((work/name).read_text())
  def local(path):
    return work/Path(path).relative_to(guest_work)
  inputs=read('native-display-runtime-inputs.json')
  roots={}
  reports={}
  commands={}
  for name in ('ffmpeg-tools','freesans'):
    host=json.loads((work.parent/f'native-companion-{name}-initial.json').read_text())
    exported=json.loads((work.parent/f'native-companion-{name}-export.json').read_text())
    command=[work/'project-target/debug/build-runtime-ffmpeg','--prepare-released-provider',
        '--plan',f'{source_mount}/rust/native-{name}-provider.json',
        '--descriptor',Path(guest_work)/'companion-exports'/name/'provider.json',
        '--descriptor-sha256',exported['descriptor_sha256'],
        '--cache',Path(guest_work)/'companion-components'/name,'--store',work/'native-media-prefixes'/name]
    rows=[]
    for phase in ('initial','reused'):
      row=json.loads(execute(command,f'native-media-{name}-{phase}.json').read_text())
      assert row['tree_sha256']==host['tree_sha256']==exported['tree_sha256']
      assert row['offline_seed_bytes']==row['asset']['transferred_bytes']==row['asset']['stored_bytes']==0
      assert (row['copied_bytes']>0)==(phase=='initial')
      assert not row['origin_archive_downloaded'] and not row['compiler_executed'] and not row['runtime_activated']
      rows.append(row)
    assert rows[0]['root']==rows[1]['root']
    roots[name]=Path(rows[0]['root'])
    inputs['files'][rows[0]['tree_receipt']]=rows[0]['tree_sha256']
    inputs['trees'][rows[0]['root']]=rows[0]['tree_receipt']
    reports[name]=dict(tree_sha256=rows[0]['tree_sha256'],descriptor_sha256=exported['descriptor_sha256'],
        repeated_http_body_bytes=0,repeated_copied_bytes=0,consumer_origin_archive_downloaded=False)
    commands[name]=(command,rows[0])
  assert len(inputs['files'])==len(inputs['trees'])==8
  (work/'native-media-runtime-inputs.json').write_text(json.dumps(inputs,indent=2)+'\n')
  sdl=Path(read('native-display-sdl-initial.json')['root'])/'lib'
  ffmpeg=Path(read('native-encoder-prefix-initial.json')['root'])/'lib'
  loader=':'.join(map(str,(ffmpeg,sdl,'/usr/lib/aarch64-linux-gnu','/lib/aarch64-linux-gnu')))
  env=['/usr/bin/env',f'LD_LIBRARY_PATH={loader}','SDL_VIDEODRIVER=dummy','SDL_AUDIODRIVER=dummy']
  tools=roots['ffmpeg-tools']/'bin'
  for name in ('ffmpeg','ffprobe'):
    with local(tools/name).open('rb') as stream: header=stream.read(64)
    assert header[:6]==b'\x7fELF\x02\x01' and int.from_bytes(header[18:20],'little')==183
  source=Path(os.environ['GITHUB_WORKSPACE'])
  execute([*env,'python3',f'{source_mount}/tools/ci/fixtures/native-media-tools.py',
      '--ffmpeg',tools/'ffmpeg','--ffprobe',tools/'ffprobe','--output',work/'native-media-cli-cases'],
      'native-media-cli.json')
  cli=read('native-media-cli.json')
  assert cli['exact_lossless_cli_roundtrips']==2
  def verify_maps(path,expected):
    maps=path.read_text()
    assert len(maps)<8*1024**2 and 'libpython' not in maps
    mapped={row[5] for line in maps.splitlines() if len(row:=line.split(maxsplit=5))==6}
    for guest_path in expected:
      assert {str(guest_path),str(local(guest_path).resolve())}&mapped, f'wrong media provider mapping: {guest_path}'
  libraries=[ffmpeg/name for name in ('libavcodec.so.60','libavformat.so.60','libavutil.so.58','libavdevice.so.60',
      'libavfilter.so.9','libpostproc.so.57','libswresample.so.4','libswscale.so.7')]
  for case in ('ffv1','libx264'):
    verify_maps(work/f'native-media-cli-cases/{case}.maps.txt',libraries)
  rust=json.loads((work.parent/'native-rust-first.json').read_text())
  rust_root=Path(guest_work)/'pinned-rust'/Path(rust['root']).relative_to(work.parent/'native-rust-toolchains')
  linker=execute(['readlink','-f','/usr/bin/cc'],'native-media-linker.txt').read_text().strip()
  environment=dict(RUSTC=str(rust_root/'bin/rustc'),LD_LIBRARY_PATH=loader,
      CARGO_ENCODED_RUSTFLAGS='\x1f'.join(['-C',f'linker={linker}']),
      CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=linker)
  for key,value in dict(CC='/usr/bin/gcc',CXX='/usr/bin/g++',AR='/usr/bin/ar',RANLIB='/usr/bin/ranlib').items():
    environment[key]=environment[f'HOST_{key}']=value
  compiler_env=['/usr/bin/env',*(f'{key}={value}' for key,value in environment.items())]
  build=[*compiler_env,rust_root/'bin/cargo','build','--release','--locked','--offline',
      '--manifest-path',f'{source_mount}/rust/Cargo.toml','--target','aarch64-unknown-linux-gnu',
      '-p','openpilot-sdl2','--features','gui','--example','native_font',
      '--target-dir',work/'native-display-target','--message-format=json']
  def artifact(path):
    rows=[row for line in path.read_text().splitlines() if (row:=json.loads(line)).get('reason')=='compiler-artifact'
        and row.get('executable') and row['target']['name']=='native_font']
    assert len(rows)==1
    return rows[0]
  first=artifact(execute(build,'native-media-font-build.jsonl'))
  assert not first['fresh']
  binary=Path(first['executable'])
  binary_sha=digest(local(binary))
  font=roots['freesans']/'cluster-replay/freesansbold.ttf'
  assert digest(local(font))=='bf9251269f11e4b35582a99376096deee3d0c4973a459cbd97b0bebc01f440ad'
  reference=work/'native-media-font-reference-bin'
  execute(['g++','-std=c++17','-O2','-Wall','-Wextra','-Werror',
      f'{source_mount}/tools/ci/fixtures/native-font-reference.cc','-ldl','-o',reference],
      'native-media-font-reference-build.txt')
  for executable,name in ((binary,'rust'),(reference,'reference')):
    execute([*env,executable,sdl/'libSDL2-2.0.so.0',sdl/'libSDL2_ttf-2.0.so.0',font,work/f'native-media-font-{name}'],
        f'native-media-font-{name}.txt')
  expected=work/'native-media-font-reference'
  actual=work/'native-media-font-rust'
  assert (expected/'dimensions.txt').read_bytes()==(actual/'dimensions.txt').read_bytes()
  dimensions=[tuple(map(int,line.split())) for line in (actual/'dimensions.txt').read_text().splitlines()]
  assert len(dimensions)==4 and all(0<width<=4096 and 0<height<=4096 for width,height in dimensions)
  glyphs=[]
  for index,(width,height) in enumerate(dimensions):
    hashes={}
    for kind in ('glyph','window'):
      filename=f'{index}.{kind}.rgba'
      assert (actual/filename).stat().st_size==(expected/filename).stat().st_size==width*height*4
      assert (actual/filename).read_bytes()==(expected/filename).read_bytes()
      hashes[kind]=digest(actual/filename)
    glyphs.append(dict(width=width,height=height,sha256=hashes))
  sdl_libraries=[sdl/'libSDL2-2-6e5c51a6.0.so.0.2800.4',sdl/'libSDL2_ttf-2-e035a276.0.so.0.2000.1']
  for folder in (expected,actual): verify_maps(folder/'provider-maps.txt',sdl_libraries)
  audit=work/'native-media-elf-audit.json'
  audit_command=[work/'project-target/debug/runtime-elf-audit','--root',binary,'--root',reference,
      '--root',tools/'ffmpeg','--root',tools/'ffprobe']
  for path in (ffmpeg,sdl,'/usr/lib/aarch64-linux-gnu','/lib/aarch64-linux-gnu'):
    audit_command+=['--library',path]
  execute([*audit_command,'--output',audit],'native-media-elf-summary.json')
  assert read('native-media-elf-audit.json')['compatible']
  repeated=artifact(execute(build,'native-media-font-build-reused.jsonl'))
  assert repeated['fresh'] and repeated['executable']==str(binary) and digest(local(binary))==binary_sha
  for name,(command,before) in commands.items():
    after=json.loads(execute(command,f'native-media-{name}-after-api.json').read_text())
    assert after['tree_sha256']==before['tree_sha256'] and after['copied_bytes']==after['asset']['transferred_bytes']==after['asset']['stored_bytes']==0
  evidence['native_media_inputs']=dict(components=reports,source_commit=evidence['source_commit'],
      original_foreign_ffmpeg_tools_executed=2,cli=cli,original_font_sha256=digest(local(font)),
      actual_current_rust_font_example_sha256=binary_sha,exact_glyph_and_window_cases=glyphs,
      reference_source_sha256=digest(source/'tools/ci/fixtures/native-font-reference.cc'),reference_binary_sha256=digest(reference),
      strict_elf_audit_compatible=True,actual_provider_maps_verified=True,python_library_mapped=False,
      repeated_font_cargo_artifact_fresh=True,repeated_http_body_bytes=0,repeated_copied_bytes=0,
      consumer_origin_archive_downloaded=False,project_rust_binaries_reused=False,
      hardware_graphics_tested=False,korean_glyph_coverage_verified=False,
      complete_cluster_upload_regression=False,full_runtime_build=False,physical_device_execution=False,
      execution_mode=NATIVE_PROOT['MODE'], native_arm_execution=True, emulated=False)


def native_source_assets_probe(guest_command,work,source_mount,evidence,log,guest_work):
  """The ARM preparer checks the exact live source pointers against real bytes."""
  def execute(argv,name,required=True):
    record=run(guest_command(argv),log,stdout_path=work/name,required=required)
    evidence['commands'].append(record)
    return record
  source=Path(os.environ['GITHUB_WORKSPACE'])
  plan=json.loads((source/'rust/native-source-lfs.json').read_text())
  host=json.loads((work.parent/'native-source-lfs-initial.json').read_text())
  command=[work/'project-target/debug/openpilot-runtime-inputs','--prepare-source-lfs',
      '--root',source_mount,'--plan',f'{source_mount}/rust/native-source-lfs.json',
      '--cache',Path(guest_work)/'source-lfs-assets','--store',work/'native-source-lfs-prefixes',
      '--objects',work/'native-source-lfs-objects']
  rows=[]
  for phase in ('initial','reused'):
    execute(command,f'native-source-lfs-{phase}.json')
    row=json.loads((work/f'native-source-lfs-{phase}.json').read_text())
    assert row['source']==host['source'] and row['source']['commit']==evidence['source_commit']
    assert row['origin_commit']==plan['origin_commit']
    assert row['tree_sha256']==host['tree_sha256'] and row['identity']==host['identity']
    assert not row['compiler_executed'] and not row['checkout_modified'] and not row['runtime_activated']
    assert set(row['assets'])==set(plan['assets'])
    assert (row['copied_bytes']>0)==(phase=='initial') and (row['linked_bytes']>0)==(phase=='initial')
    for path,asset in plan['assets'].items():
      assert row['assets'][path]['transferred_bytes']==row['assets'][path]['stored_bytes']==0
      actual=work/Path(row['root']).relative_to(guest_work)/path
      assert actual.stat().st_size==asset['bytes'] and digest(actual)==asset['sha256']
    if phase=='reused': assert not row['asset_cache_consulted'] and row['expected_additional_bytes']==0
    rows.append(row)
  assert rows[0]['root']==rows[1]['root']
  bad=json.loads(json.dumps(plan))
  bad['assets'][next(iter(bad['assets']))]['sha256']='0'*64
  bad_path=work/'native-source-lfs-bad-plan.json'
  bad_path.write_text(json.dumps(bad)+'\n')
  rejected=execute([work/'project-target/debug/openpilot-runtime-inputs','--prepare-source-lfs',
      '--root',source_mount,'--plan',bad_path,'--cache',Path(guest_work)/'source-lfs-assets',
      '--store',work/'native-source-lfs-rejected','--objects',work/'native-source-lfs-rejected-objects'],
      'native-source-lfs-rejected.txt',required=False)
  assert rejected['exit_code']!=0 and not (work/'native-source-lfs-rejected').exists()
  assert not (work/'native-source-lfs-rejected-objects').exists()
  inputs=json.loads((work/'native-media-runtime-inputs.json').read_text())
  inputs['files'][rows[0]['tree_receipt']]=rows[0]['tree_sha256']
  inputs['trees'][rows[0]['root']]=rows[0]['tree_receipt']
  assert len(inputs['files'])==len(inputs['trees'])==9
  (work/'native-source-lfs-runtime-inputs.json').write_text(json.dumps(inputs,indent=2)+'\n')
  evidence['native_source_lfs']=dict(source_commit=evidence['source_commit'],origin_commit=plan['origin_commit'],
      source_lfs_objects=4,declared_bytes=sum(row['bytes'] for row in plan['assets'].values()),
      tree_sha256=rows[0]['tree_sha256'],actual_arm_preparer=True,actual_object_hashes_match_current_source=True,
      host_expected_tree_matches=True,repeated_http_body_bytes=0,repeated_copied_bytes=0,repeated_linked_bytes=0,
      pointer_mismatch_rejected_before_staging=True,source_checkout_unchanged=True,
      generated_firmware_prepared=False,python_model_generation_complete=False,
      full_runtime_build=False,physical_device_execution=False,
      execution_mode=NATIVE_PROOT['MODE'], native_arm_execution=True, emulated=False)


def native_mpc_inputs_probe(guest_command,work,source_mount,evidence,log,guest_work):
  """Verify source-bound MPC inputs and the current Rust/native solver ABI."""
  room(work,2*1024**3)
  def execute(argv,name,required=True):
    record=run(guest_command(argv),log,stdout_path=work/name,required=required)
    evidence['commands'].append(record)
    return record
  def read(name):
    return json.loads((work/name).read_text())
  source=Path(os.environ['GITHUB_WORKSPACE'])
  exported=json.loads((work.parent/'native-mpc-export.json').read_text())
  host=json.loads((work.parent/'native-mpc-initial.json').read_text())
  plan=json.loads((source/'rust/native-mpc-provider.json').read_text())
  command=[work/'project-target/debug/build-runtime-ffmpeg','--prepare-released-provider',
      '--plan',f'{source_mount}/rust/native-mpc-provider.json',
      '--descriptor',Path(guest_work)/'mpc-export/provider.json',
      '--descriptor-sha256',exported['descriptor_sha256'],
      '--cache',Path(guest_work)/'mpc-components','--store',work/'native-mpc-prefixes']
  # The generated solver may never enter through the unbound foreign-library path.
  rejected=execute(command,'native-mpc-unbound.txt',required=False)
  assert rejected['exit_code']!=0 and not (work/'native-mpc-prefixes').exists()
  command+=['--source-root',source_mount]
  rows=[]
  for phase in ('initial','reused'):
    execute(command,f'native-mpc-{phase}.json')
    row=read(f'native-mpc-{phase}.json')
    assert row['tree_sha256']==host['tree_sha256']==exported['tree_sha256']
    assert row['source_binding']==host['source_binding']==exported['source_binding']
    assert row['source_binding']['source']['commit']==evidence['source_commit']
    assert row['source_binding']['original_source_commit']==plan['source_commit']!=evidence['source_commit']
    assert row['source_binding']['inputs_match'] and not row['source_binding']['regenerated']
    assert row['source_binding']['contract_sha256']==digest(source/'rust/native-mpc-source-contract.json')
    assert row['offline_seed_bytes']==row['asset']['transferred_bytes']==row['asset']['stored_bytes']==0
    assert (row['copied_bytes']>0)==(phase=='initial')
    assert not row['origin_archive_downloaded'] and not row['compiler_executed'] and not row['runtime_activated']
    rows.append(row)
  assert rows[0]['root']==rows[1]['root']
  prefix=Path(rows[0]['root'])/'plannerd-acados'
  actual_prefix=work/prefix.relative_to(guest_work)
  rust=json.loads((work.parent/'native-rust-first.json').read_text())
  rust_root=Path(guest_work)/'pinned-rust'/Path(rust['root']).relative_to(work.parent/'native-rust-toolchains')
  capnp=Path(read('native-cereal-preparation-initial.json')['root'])
  execute(['readlink','-f','/usr/bin/cc'],'native-mpc-linker.txt')
  linker=(work/'native-mpc-linker.txt').read_text().strip()
  environment=dict(RUSTC=str(rust_root/'bin/rustc'),OPENPILOT_CAPNP=str(capnp/'usr/bin/capnp'),
      OPENPILOT_NATIVE_INCREMENTAL='1',LD_LIBRARY_PATH=f'{prefix}:{capnp}/usr/lib/aarch64-linux-gnu',
      CARGO_ENCODED_RUSTFLAGS='\x1f'.join(['-C',f'linker={linker}']),
      CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=linker)
  for key,value in dict(CC='/usr/bin/gcc',CXX='/usr/bin/g++',AR='/usr/bin/ar',RANLIB='/usr/bin/ranlib').items():
    environment[key]=environment[f'HOST_{key}']=value
  env=['/usr/bin/env',*(f'{key}={value}' for key,value in environment.items())]
  build=[*env,rust_root/'bin/cargo','build','--release','--locked','--offline',
      '--manifest-path',f'{source_mount}/rust/Cargo.toml','--target','aarch64-unknown-linux-gnu',
      '-p','openpilot-plannerd','--features','native-skip-miri','--example','plannerd_solver_trace',
      '--target-dir',work/'native-mpc-target','--message-format=json']
  def artifact(name):
    artifacts=[row for line in (work/name).read_text().splitlines() if (row:=json.loads(line)).get('reason')=='compiler-artifact'
        and row.get('executable') and row['target']['name']=='plannerd_solver_trace']
    assert len(artifacts)==1
    return artifacts[0]
  execute(build,'native-mpc-build.jsonl')
  first=artifact('native-mpc-build.jsonl')
  assert not first['fresh']
  binary=Path(first['executable'])
  local_binary=work/binary.relative_to(guest_work)
  initial_sha=digest(local_binary)
  header=local_binary.read_bytes()[:64]
  assert header[:6]==b'\x7fELF\x02\x01' and int.from_bytes(header[18:20],'little')==183
  cases=Path(guest_work)/'native-mpc-abi-cases'
  execute([*env,'python3',f'{source_mount}/tools/ci/fixtures/native-mpc-abi.py',
      '--provider',prefix,'--rust-binary',binary,'--output',cases],'native-mpc-abi.json')
  comparison=read('native-mpc-abi.json')
  assert comparison['bit_exact_solves']==12 and comparison['successful_statuses'] and comparison['finite_outputs']
  maps=(work/'native-mpc-abi-cases/provider-maps.txt').read_text()
  assert len(maps)<8*1024**2
  mapped_paths={row[5] for line in maps.splitlines() if len(row:=line.split(maxsplit=5))==6}
  for filename in ('libacados.so','libacados_ocp_solver_lat.so','libacados_ocp_solver_long.so'):
    assert {str(prefix/filename),str((actual_prefix/filename).resolve())}&mapped_paths, f'wrong MPC provider mapping: {filename}'
  audit=work/'native-mpc-elf-audit.json'
  audit_command=[work/'project-target/debug/runtime-elf-audit','--root',binary]
  for path in plan['members']:
    if '.so' in Path(path).name:
      audit_command+=['--root',Path(rows[0]['root'])/path]
  for path in (prefix,'/usr/lib/aarch64-linux-gnu','/lib/aarch64-linux-gnu'):
    audit_command+=['--library',path]
  execute([*audit_command,'--output',audit],'native-mpc-elf-summary.json')
  assert read('native-mpc-elf-audit.json')['compatible']
  execute(build,'native-mpc-build-reused.jsonl')
  repeated=artifact('native-mpc-build-reused.jsonl')
  assert repeated['fresh'] and repeated['executable']==str(binary) and digest(local_binary)==initial_sha
  execute(command,'native-mpc-after-api.json')
  after=read('native-mpc-after-api.json')
  assert after['tree_sha256']==rows[0]['tree_sha256'] and after['source_binding']==rows[0]['source_binding']
  assert after['copied_bytes']==after['asset']['transferred_bytes']==after['asset']['stored_bytes']==0
  # Runtime planning uses the complete bound provider; the earlier partial core
  # component/probe remains as independent evidence and is not mutated/deleted.
  inputs=read('native-source-lfs-runtime-inputs.json')
  partial=read('native-companion-acados-initial.json')
  assert inputs['files'].pop(partial['tree_receipt'])==partial['tree_sha256']
  assert inputs['trees'].pop(partial['root'])==partial['tree_receipt']
  inputs['files'][rows[0]['tree_receipt']]=rows[0]['tree_sha256']
  inputs['trees'][rows[0]['root']]=rows[0]['tree_receipt']
  assert len(inputs['files'])==len(inputs['trees'])==9
  (work/'native-runtime-inputs.json').write_text(json.dumps(inputs,indent=2)+'\n')
  evidence['native_mpc_inputs']=dict(source_binding=rows[0]['source_binding'],
      tree_sha256=rows[0]['tree_sha256'],descriptor_sha256=exported['descriptor_sha256'],
      actual_arm_preparer=True,actual_current_rust_solver_compiled_and_executed=True,
      rust_example_sha256=initial_sha,direct_c_abi_comparison=comparison,
      strict_elf_audit_compatible=True,actual_provider_maps_verified=True,
      repeated_cargo_artifact_fresh=True,repeated_http_body_bytes=0,repeated_copied_bytes=0,
      consumer_origin_archive_downloaded=False,generated_mpc_solvers_reused=True,
      solvers_regenerated=False,original_cython_regression=False,
      complete_python_generation_environment=False,full_planner_regression=False,
      full_runtime_build=False,runtime_activated=False,published_release=False,
      public_component_download_tested=False,physical_device_execution=False,
      execution_mode=NATIVE_PROOT['MODE'], native_arm_execution=True, emulated=False)


def native_panda_inputs_probe(guest_command,work,source_mount,evidence,log,guest_work):
  """Prepare source-bound safety firmware; never execute or flash its bytes."""
  source=Path(os.environ['GITHUB_WORKSPACE'])
  plan=json.loads((source/'rust/native-panda-provider.json').read_text())
  room(work,4*sum(row['bytes'] for row in plan['members'].values())+16*1024**2)
  exported=json.loads((work.parent/'native-panda-export.json').read_text())
  host=json.loads((work.parent/'native-panda-initial.json').read_text())
  def execute(argv,name,required=True):
    record=run(guest_command(argv),log,stdout_path=work/name,required=required)
    evidence['commands'].append(record)
    return record
  command=[work/'project-target/debug/build-runtime-ffmpeg','--prepare-released-provider',
      '--plan',f'{source_mount}/rust/native-panda-provider.json',
      '--descriptor',Path(guest_work)/'panda-export/provider.json',
      '--descriptor-sha256',exported['descriptor_sha256'],
      '--cache',Path(guest_work)/'panda-components','--store',work/'native-panda-prefixes']
  rejected=execute(command,'native-panda-unbound.txt',required=False)
  assert rejected['exit_code']!=0 and not (work/'native-panda-prefixes').exists()
  command+=['--source-root',source_mount]
  rows=[]
  for phase in ('initial','reused'):
    execute(command,f'native-panda-{phase}.json')
    row=json.loads((work/f'native-panda-{phase}.json').read_text())
    assert row['tree_sha256']==host['tree_sha256']==exported['tree_sha256']
    assert row['source_binding']==host['source_binding']==exported['source_binding']
    assert row['source_binding']['source']['commit']==evidence['source_commit']
    assert row['source_binding']['original_source_commit']==plan['source_commit']!=evidence['source_commit']
    assert row['source_binding']['inputs_match'] and not row['source_binding']['regenerated']
    assert row['source_binding']['contract_sha256']==digest(source/'rust/native-panda-source-contract.json')
    assert row['offline_seed_bytes']==row['asset']['transferred_bytes']==row['asset']['stored_bytes']==0
    assert (row['copied_bytes']>0)==(phase=='initial')
    assert not row['origin_archive_downloaded'] and not row['compiler_executed'] and not row['runtime_activated']
    rows.append(row)
  assert rows[0]['root']==rows[1]['root']
  execute(['/usr/bin/python3',f'{source_mount}/tools/ci/fixtures/native-panda-firmware.py',
      '--source',source_mount,'--provider',rows[0]['root'],
      '--output',work/'native-panda-firmware-check.json'],'native-panda-firmware-check.txt')
  report=json.loads((work/'native-panda-firmware-check.json').read_text())
  assert len(report['artifacts'])==4 and report['source_input_hashes_checked']==251
  assert sum(row['debug_signature_verified'] for row in report['artifacts'])==2
  assert not report['firmware_regenerated'] and not report['private_key_read'] and not report['firmware_executed']
  assert not report['device_connected'] and not report['physical_device_execution']
  inputs=json.loads((work/'native-runtime-inputs.json').read_text())
  inputs['files'][rows[0]['tree_receipt']]=rows[0]['tree_sha256']
  inputs['trees'][rows[0]['root']]=rows[0]['tree_receipt']
  assert len(inputs['files'])==len(inputs['trees'])==10
  (work/'native-runtime-panda-inputs.json').write_text(json.dumps(inputs,indent=2)+'\n')
  evidence['native_panda_inputs']=dict(source_binding=rows[0]['source_binding'],tree_sha256=rows[0]['tree_sha256'],
      actual_arm_preparer=True,host_expected_tree_matches=True,firmware_outputs_verified=4,
      signed_images_public_signature_verified=2,firmware_version=report['firmware_version'],
      repeated_http_body_bytes=0,repeated_copied_bytes=0,unbound_provider_rejected_before_staging=True,
      independent_firmware_regenerated=False,firmware_executed=False,device_connected=False,
      full_runtime_build=False,physical_device_execution=False,execution_mode=NATIVE_PROOT['MODE'], native_arm_execution=True, emulated=False)


def native_dbc_inputs_probe(guest_command,work,source_mount,evidence,log,guest_work):
  """Bind all original DBC bytes and regenerate only the CI reference."""
  source=Path(os.environ['GITHUB_WORKSPACE'])
  plan=json.loads((source/'rust/native-dbc-provider.json').read_text())
  room(work,4*sum(row['bytes'] for row in plan['members'].values())+128*1024**2)
  exported=json.loads((work.parent/'native-dbc-export.json').read_text())
  host=json.loads((work.parent/'native-dbc-initial.json').read_text())
  def execute(argv,name,required=True):
    record=run(guest_command(argv),log,stdout_path=work/name,required=required)
    evidence['commands'].append(record)
    return record
  command=[work/'project-target/debug/build-runtime-ffmpeg','--prepare-released-provider',
      '--plan',f'{source_mount}/rust/native-dbc-provider.json',
      '--descriptor',Path(guest_work)/'dbc-export/provider.json',
      '--descriptor-sha256',exported['descriptor_sha256'],
      '--cache',Path(guest_work)/'dbc-components','--store',work/'native-dbc-prefixes']
  rejected=execute(command,'native-dbc-unbound.txt',required=False)
  assert rejected['exit_code']!=0 and not (work/'native-dbc-prefixes').exists()
  rows=[]
  for phase in ('initial','reused'):
    execute([*command,'--source-root',source_mount],f'native-dbc-{phase}.json')
    row=json.loads((work/f'native-dbc-{phase}.json').read_text())
    assert row['tree_sha256']==host['tree_sha256']==exported['tree_sha256']
    assert row['source_binding']==host['source_binding']==exported['source_binding']
    assert row['source_binding']['source']['commit']==evidence['source_commit']
    assert row['source_binding']['original_source_commit']==plan['source_commit']!=evidence['source_commit']
    assert row['source_binding']['inputs_match'] and not row['source_binding']['regenerated']
    assert row['offline_seed_bytes']==row['asset']['transferred_bytes']==row['asset']['stored_bytes']==0
    assert (row['copied_bytes']>0)==(phase=='initial')
    assert not row['origin_archive_downloaded'] and not row['compiler_executed'] and not row['runtime_activated']
    rows.append(row)
  assert rows[0]['root']==rows[1]['root']
  execute([work/'project-target/debug/openpilot-runtime-inputs','--verify-dbcs','--root',source_mount,
      '--store',rows[0]['root']],'native-dbc-core-verification.json')
  execute(['/usr/bin/python3',f'{source_mount}/tools/ci/fixtures/native-dbc-inputs.py',
      '--source',source_mount,'--provider',rows[0]['root'],'--work',work,
      '--output',work/'native-dbc-reference.json'],'native-dbc-reference.txt')
  reference=json.loads((work/'native-dbc-reference.json').read_text())
  assert reference['source_inputs_checked']==140 and reference['original_databases_regenerated_for_oracle']==103
  assert reference['exact_byte_comparison']
  inputs=json.loads((work/'native-runtime-panda-inputs.json').read_text())
  inputs['files'][rows[0]['tree_receipt']]=rows[0]['tree_sha256']
  inputs['trees'][rows[0]['root']]=rows[0]['tree_receipt']
  assert len(inputs['files'])==len(inputs['trees'])==11
  (work/'native-runtime-dbc-inputs.json').write_text(json.dumps(inputs,indent=2)+'\n')
  evidence['native_dbc_inputs']=dict(source_binding=rows[0]['source_binding'],tree_sha256=rows[0]['tree_sha256'],
      actual_arm_preparer=True,original_reference=reference,host_expected_tree_matches=True,
      repeated_http_body_bytes=0,repeated_copied_bytes=0,unbound_provider_rejected_before_staging=True,
      runtime_data_regenerated=False,full_runtime_build=False,physical_device_execution=False,
      execution_mode=NATIVE_PROOT['MODE'], native_arm_execution=True, emulated=False)


def native_usbgpu_inputs_probe(guest_command,work,source_mount,evidence,log,guest_work):
  """Reuse original data-only companions and the independent raw model cache."""
  source=Path(os.environ['GITHUB_WORKSPACE'])
  plan=json.loads((source/'rust/native-usbgpu-provider.json').read_text())
  room(work,4*sum(row['bytes'] for row in plan['members'].values())+16*1024**2)
  exported=json.loads((work.parent/'native-usbgpu-export.json').read_text())
  host=json.loads((work.parent/'native-usbgpu-initial.json').read_text())
  def execute(argv,name):
    evidence['commands'].append(run(guest_command(argv),log,stdout_path=work/name))
    return json.loads((work/name).read_text())
  command=[work/'project-target/debug/build-runtime-ffmpeg','--prepare-released-provider',
      '--source-root',source_mount,'--plan',f'{source_mount}/rust/native-usbgpu-provider.json',
      '--descriptor',Path(guest_work)/'usbgpu-export/provider.json','--descriptor-sha256',exported['descriptor_sha256'],
      '--cache',Path(guest_work)/'usbgpu-components','--store',work/'native-usbgpu-prefixes']
  rows=[execute(command,f'native-usbgpu-{phase}.json') for phase in ('initial','reused')]
  for i,row in enumerate(rows):
    assert row['tree_sha256']==host['tree_sha256']==exported['tree_sha256']
    assert row['source_binding']==host['source_binding']==exported['source_binding']
    assert row['source_binding']['source']['commit']==evidence['source_commit']
    assert row['source_binding']['inputs_match'] and not row['source_binding']['regenerated']
    assert row['asset']['transferred_bytes']==row['asset']['stored_bytes']==row['offline_seed_bytes']==0
    assert (row['copied_bytes']>0)==(i==0)
  model_command=[work/'project-target/debug/openpilot-runtime-inputs','--prepare-assets',
      '--plan',f'{source_mount}/rust/native-usbgpu-model.json','--cache',Path(guest_work)/'usbgpu-model-assets']
  models=[execute(model_command,f'native-usbgpu-model-{phase}.json')['usbgpu-model'] for phase in ('initial','reused')]
  assert all(row['transferred_bytes']==row['stored_bytes']==0 for row in models)
  assert models[0]['path']==models[1]['path'] and models[0]['sha256']==plan_model(source)['sha256']
  evidence['native_usbgpu_inputs']=dict(source_binding=rows[0]['source_binding'],tree_sha256=rows[0]['tree_sha256'],
      model_sha256=models[0]['sha256'],model_cached_independently=True,repeated_http_body_bytes=0,
      repeated_copied_bytes=0,runtime_data_regenerated=False,gpu_executed=False,physical_device_execution=False)


def plan_model(source):
  return json.loads((source/'rust/native-usbgpu-model.json').read_text())['assets']['usbgpu-model']


def native_complete_layout_probe(guest_command,work,source_mount,evidence,log,guest_work):
  """Compose every real input and hand a complete layout to the 99-target build."""
  room(work,64*1024**2)
  def read(name): return json.loads((work/name).read_text())
  def pin(row): return dict(root=row['root'],receipt=row['tree_receipt'],sha256=row['tree_sha256'])
  def execute(argv,name,required=True):
    before=shutil.disk_usage(work).free
    result=run(guest_command(argv),log,stdout_path=work/name,required=required)
    result['observed_disk_growth_bytes']=before-shutil.disk_usage(work).free
    evidence['commands'].append(result)
    return result
  roles={name:pin(read(f'native-companion-{name}-initial.json')) for name in ('numerics','geos','brotli','acados')}
  roles.update({name:pin(read(f'native-display-{name}-initial.json')) for name in ('sdl','turbojpeg')})
  roles.update({name:pin(read(f'native-media-{name}-initial.json')) for name in ('ffmpeg-tools','freesans')})
  roles.update(ffmpeg=pin(read('native-provider-initial.json')),raylib=pin(read('native-raylib-provider-initial.json')),
      opencv=pin(read('native-opencv-prefix-initial.json')['provider']),
      panda=pin(read('native-panda-initial.json')),mpc=pin(read('native-mpc-initial.json')),
      dbc=pin(read('native-dbc-initial.json')),usbgpu=pin(read('native-usbgpu-initial.json')),
      models=pin(read('native-model-generation-initial.json')['export']),
      **{'source-lfs':pin(read('native-source-lfs-initial.json'))})
  raw=read('native-usbgpu-model-initial.json')['usbgpu-model']
  roles['usbgpu-model']={key:raw[key] for key in ('path','sha256','bytes')}
  pins=read('native-model-input-pins.json')
  budget=read('native-project-growth-estimates.json')
  spec=dict(schema=1,roles=roles,toolchain_root=pins['inputs']['rust']['root'],libraries=['/usr/lib/aarch64-linux-gnu','/lib/aarch64-linux-gnu'],
      expected_source_growth_bytes=budget['expected_source_growth_bytes'],remaining_object_growth_bytes=budget['remaining_object_growth_bytes'])
  path=work/'native-complete-input-spec.json'
  path.write_text(json.dumps(spec,indent=2)+'\n')
  python=read('native-python-initial.json')['python']
  command=[python,'-I','-B','-S','-X',f'pycache_prefix={guest_work}/runtime-layout-bytecode',
      f'{source_mount}/rust/tools/native_runtime_layout.py','--root',source_mount,'--spec',path,
      '--store',work/'native-complete-input-prefixes','--verifier',work/'project-target/debug/openpilot-runtime-inputs']
  rows=[]
  for phase in ('initial','reused'):
    report=work/f'native-complete-input-{phase}.json'
    execute([*command,'--report',report],f'native-complete-input-{phase}.txt')
    row=read(report.name)
    assert row['source']['commit']==evidence['source_commit'] and row['required_roles']==18
    assert row['source_data_verified'] and not row['runtime_activated'] and not row['full_runtime_build']
    rows.append(row)
  assert rows[0]['identity']==rows[1]['identity'] and rows[0]['root']==rows[1]['root']
  assert rows[1]['copied_bytes']==rows[1]['linked_bytes']==0
  (work/'native-complete-layout.json').write_text(json.dumps(rows[0]['layout'],indent=2)+'\n')
  pins['runtime_inputs']=rows[0]['runtime_inputs']
  (work/'native-complete-input-pins.json').write_text(json.dumps(pins,indent=2)+'\n')
  planner=work/'project-target/debug/openpilot-runtime-plan'
  cargo=Path(pins['inputs']['rust']['root'])/'bin/cargo'
  execute([planner,'--root',source_mount,'--cargo',cargo,'--native-inputs',work/'native-complete-input-pins.json',
      '--budgets',work/'native-project-growth-estimates.json','--output',work/'native-complete-bound-plan'],
      'native-complete-bound-plan-summary.json')
  complete=read('native-complete-bound-plan/build-plan.json')
  previous=read('native-model-bound-plan/build-plan.json')
  assert complete['groups']==previous['groups'] and complete['runtime_inputs']==rows[0]['runtime_inputs']
  evidence['native_complete_layout']=dict(identity=rows[0]['identity'],tree_sha256=rows[0]['tree_sha256'],
      required_roles=18,compiler_groups=13,binaries=99,initial_copied_bytes=rows[0]['copied_bytes'],
      initial_logical_linked_bytes=rows[0]['linked_bytes'],repeated_copied_bytes=0,repeated_linked_bytes=0,
      source_data_verified=True,full_runtime_build=False,runtime_activated=False,physical_device_execution=False)
  # Execute the complete bound plan, not an inferred build success from planning.
  # Per-group/source/object growth is admitted by the same native build APIs.
  rust=pins['inputs']['rust']['root']
  room(work,2*1024**3)
  execute(['/usr/bin/env',f'RUSTC={Path(rust)/"bin/rustc"}',cargo,'build','--locked','--offline',
      '--manifest-path',f'{source_mount}/rust/Cargo.toml','-p','openpilot-runtime-build',
      '--example','native_full_generation','--target-dir',work/'project-target'],
      'native-complete-generation-tool-build.txt')
  base=work/'native-complete-stores'
  (base/'params/d').mkdir(parents=True)
  (base/'params/d/IsOnroad').write_bytes(b'0')
  fixture=work/'native-complete-fixture-source'
  execute(['git','clone','--shared',work/'native-model-fixture-source',fixture],'native-complete-source-clone.txt')
  execute(['git','-C',fixture,'remote','set-url','origin','https://github.com/bin9208/openpilot-rust.git'],
      'native-complete-source-origin.txt')
  execute(['git','-C',fixture,'checkout','--detach',evidence['source_commit']],'native-complete-source-checkout.txt')
  generation_command=[work/'project-target/debug/examples/native_full_generation','--root',fixture,
      '--plan',work/'native-complete-bound-plan/build-plan.json','--layout',work/'native-complete-layout.json',
      '--cache',work/'native-complete-cargo-cache','--toolchain',Path(rust)/'bin','--stores',base]
  execute(['/usr/bin/time','-v','-o',work/'native-complete-initial-resources.txt',
      *generation_command,'--evidence',work/'native-complete-generation-evidence'],
      'native-complete-generation-result.json')
  generation=read('native-complete-generation-evidence/generation.json')
  assert generation['source']['commit']==evidence['source_commit']
  assert generation['binaries']==99 and generation['compiler_groups']==13
  assert generation['source_built_sealer_and_abi_audit_passed'] and generation['source_generation_verified']
  assert generation['composed_staging_cache_absence_verified'] and not generation['runtime_activated']
  assert not generation['runtime_started'] and not generation['physical_device_execution']
  evidence['native_complete_generation']=generation
  prior=work/'native-complete-generation-evidence/prepared.json'
  cases=[dict(case='repeat',request_source=dict(commit=evidence['source_commit']))]
  cases+=evidence['native_model_generation']['actual_source_reuse_cases']
  reuse=[]
  for case in cases:
    kind=case['case']
    execute(['git','-C',fixture,'checkout','--detach',case['request_source']['commit']],f'native-complete-{kind}-checkout.txt')
    execute(['/usr/bin/time','-v','-o',work/f'native-complete-{kind}-resources.txt',
        *generation_command,'--previous',prior,'--reuse-kind',kind,
        '--evidence',work/f'native-complete-{kind}-evidence'],f'native-complete-{kind}-result.json')
    result=read(f'native-complete-{kind}-evidence/reuse.json')
    assert result['source']['commit']==case['request_source']['commit']
    assert result['compiler_groups']==13 and result['binaries']==99 and not result['runtime_activated']
    assert bool(result['rebuilt_groups'])==(kind=='small-logger')
    reuse.append(result)
  evidence['native_complete_generation_reuse']=reuse
  evidence['native_complete_generation_resource_limit']='per-command wall time, observed disk growth, and single-child maximum RSS; not aggregate process-tree/kernel memory peak or physical ARM/device evidence'



def native_catalog_bindings_probe(guest_command,work,source_mount,evidence,log,guest_work):
  """Assemble the actual 99-target plan using all independently prepared inputs."""
  room(work,4*1024**2)
  def read(name):
    return json.loads((work/name).read_text())
  def pin(row):
    return dict(root=row['root'],receipt=row['tree_receipt'],sha256=row['tree_sha256'])
  rust=json.loads((work.parent/'native-rust-first.json').read_text())
  rust_pin=pin(rust)
  for key in ('root','receipt'):
    rust_pin[key]=str(Path(guest_work)/'pinned-rust'/Path(rust_pin[key]).relative_to(work.parent/'native-rust-toolchains'))
  pins=dict(schema=1,inputs={
      'rust':rust_pin,
      'cargo':pin(read('native-cereal-cargo-inputs-reuse.json')),
      'system-sdk':dict(root='/',receipt=str(Path(guest_work)/'native-system-sdk.json'),sha256=digest(work/'native-system-sdk.json')),
      'capnp':pin(read('native-cereal-preparation-initial.json')),
      'ffmpeg':pin(read('native-encoder-prefix-initial.json')),
      'libyuv':pin(read('native-codec-initial.json')['libyuv']),
      'raylib':pin(read('native-raylib-sdk-initial.json')),
      'opencv':pin(read('native-opencv-prefix-initial.json')),
      'eigen':pin(read('native-eigen-initial.json')['eigen']),
  },runtime_inputs=read('native-runtime-dbc-inputs.json'))
  catalog=read('native-project-catalog.json')
  # Explicit conservative plan-only envelopes. This probe does not reserve or
  # consume these build budgets and is not a measured full-build resource trial.
  source_catalog=json.loads((Path(os.environ['GITHUB_WORKSPACE'])/'rust/native-runtime-catalog.json').read_text())
  budgets=dict(schema=1,
      group_growth_bytes={name:len(group['packages'])*512*1024**2+len(group['binaries'])*64*1024**2
          for name,group in source_catalog['groups'].items()},
      expected_source_growth_bytes=2*1024**3,remaining_object_growth_bytes=catalog['binaries']*64*1024**2,
      estimate_basis='CI plan-only conservative envelope: 512 MiB per selected package + 64 MiB per binary per group; 2 GiB source staging; 64 MiB immutable storage per binary. Not measured full-runtime growth; calibrate for the scheduled full build.')
  pins_path=work/'native-project-input-pins.json'
  budgets_path=work/'native-project-growth-estimates.json'
  for path,value in ((pins_path,pins),(budgets_path,budgets)):
    path.write_text(json.dumps(value,indent=2)+'\n')
  planner=work/'project-target/debug/openpilot-runtime-plan'
  cargo=Path(rust_pin['root'])/'bin/cargo'
  command=[planner,'--root',source_mount,'--cargo',cargo,'--native-inputs',pins_path,'--budgets',budgets_path]
  audits=[]
  outputs=[]
  for phase in ('initial','reused'):
    output=work/f'native-project-bindings-{phase}'
    summary=work/f'native-project-bindings-{phase}.json'
    evidence['commands'].append(run(guest_command([*command,'--output',output]),log,stdout_path=summary))
    audit=json.loads(summary.read_text())
    plan=json.loads((output/'build-plan.json').read_text())
    bindings=json.loads((output/'bindings.json').read_text())
    assert audit['source']['commit']==evidence['source_commit']
    assert audit['groups']==13 and audit['binaries']==99 and audit['compiler_inputs_bound'] is True
    assert not audit['compiled'] and not audit['activated']
    assert audit['native_inputs_sha256']==digest(pins_path) and audit['budgets_sha256']==digest(budgets_path)
    assert audit['bindings_sha256']==digest(output/'bindings.json')
    assert audit['plan_sha256']==digest(output/'build-plan.json')
    assert plan['runtime_inputs']==pins['runtime_inputs'] and len(plan['runtime_inputs']['trees'])==11
    for name,requirements in catalog['compiler_inputs'].items():
      recipe=plan['groups'][name]
      for key,suffix in requirements.items():
        value=Path(recipe['environment'][key])
        root=value if not suffix else value.parents[len(Path(suffix).parts)-1]
        receipt=recipe['input_trees'][str(root)]
        assert receipt in recipe['inputs']
      assert recipe['input_trees']['/']==pins['inputs']['system-sdk']['receipt']
      assert recipe['expected_growth_bytes']==budgets['group_growth_bytes'][name]
    assert bindings['common']==['rust','cargo','system-sdk']
    audits.append(audit)
    outputs.append(output)
  assert audits[0]==audits[1]
  assert all(digest(outputs[0]/name)==digest(outputs[1]/name)
      for name in ('build-plan.json','bindings.json','catalog-audit.json'))
  # Wrong independently supplied pins and missing operation estimates must fail
  # without publishing. No prepared input, previous plan or source is modified.
  failures={}
  for case in ('bad-receipt-pin','missing-provider','zero-growth'):
    candidate=json.loads(json.dumps(pins))
    estimate=json.loads(json.dumps(budgets))
    if case=='bad-receipt-pin': candidate['inputs']['raylib']['sha256']='0'*64
    elif case=='missing-provider': del candidate['inputs']['eigen']
    else: estimate['group_growth_bytes']['12']=0
    input_path=work/f'native-project-bindings-{case}-inputs.json'
    budget_path=work/f'native-project-bindings-{case}-budgets.json'
    input_path.write_text(json.dumps(candidate)+'\n')
    budget_path.write_text(json.dumps(estimate)+'\n')
    output=work/f'native-project-bindings-rejected-{case}'
    result=run(guest_command([planner,'--root',source_mount,'--cargo',cargo,'--native-inputs',input_path,
        '--budgets',budget_path,'--output',output]),log,required=False,
        stdout_path=work/f'native-project-bindings-{case}.txt')
    evidence['commands'].append(result)
    assert result['exit_code']!=0 and not output.exists()
    failures[case]=result['exit_code']
  evidence['native_project_bindings']=dict(source_commit=evidence['source_commit'],
      groups=13,binaries=99,input_roles=9,compiler_inputs_bound=True,
      plan_sha256=audits[0]['plan_sha256'],bindings_sha256=audits[0]['bindings_sha256'],
      repeated_plan_bytes_identical=True,rejected_inputs=failures,
      operation_estimates_measured=False,full_runtime_build=False,runtime_assets_complete=False,
      runtime_activated=False,physical_device_execution=False,
      execution_mode=NATIVE_PROOT['MODE'], native_arm_execution=True, emulated=False)


def build_native_inputs(guest_command, work, source_mount, evidence, log):
  # Shared with the full cereal probe: identical package, feature set and argv.
  evidence['commands'].append(run(guest_command([
      'cargo','build','--locked','--manifest-path',f'{source_mount}/rust/Cargo.toml',
      '-p','openpilot-runtime-build','--bin','openpilot-runtime-inputs',
      '--target-dir',work/'project-target','--message-format=json']), log,
      stdout_path=work/'native-cereal-inputs-build.jsonl'))


def prepare_guest_rust(guest_command, work, rustup, evidence, log):
  evidence['commands'].append(run(guest_command([rustup, '-y', '--default-toolchain', '1.94.0', '--profile', 'minimal', '--no-modify-path']), log))
  for argv in [['rustc', '-vV'], ['cargo', '-vV'], ['rustc', '--print', 'sysroot'], ['gcc', '-print-search-dirs']]:
    evidence['commands'].append(run(guest_command(argv), log))


# Only future job-owned canonical bind roots; never claim prepared providers.
PREFLIGHT_EMPTY_BIND_ROOTS = (
    'native-codec-assets', 'native-eigen-assets', 'native-ffmpeg-source',
    'native-provider-components', 'native-provider-export',
    'native-companion-components', 'native-companion-exports',
    'native-source-lfs-assets', 'native-python-assets', 'native-model-wheel-assets',
    'native-model-assets', 'native-mpc-components', 'native-mpc-export',
    'native-panda-components', 'native-panda-export', 'native-dbc-components',
    'native-dbc-export', 'native-usbgpu-components', 'native-usbgpu-export',
    'native-usbgpu-model-assets', 'native-raylib-assets', 'native-raylib-components',
    'native-raylib-provider-export', 'native-opencv-source',
    'native-opencv-components', 'native-opencv-provider-export')


def prepare_preflight_bind_roots(owned, created):
  for name in PREFLIGHT_EMPTY_BIND_ROOTS:
    path = owned/name
    if not path.exists():
      path.mkdir()
      info = path.lstat()
      created.append(dict(path=str(path), dev=info.st_dev, ino=info.st_ino, uid=info.st_uid))
    assert path.is_dir() and not path.is_symlink() and path.resolve(strict=True) == path


def cleanup_preflight_bind_roots(owned, created):
  assert not owned.is_symlink() and owned.resolve(strict=True) == owned
  def checked(row):
    path = Path(row['path'])
    assert path.parent == owned and path.name in PREFLIGHT_EMPTY_BIND_ROOTS
    info = path.lstat()
    assert stat.S_ISDIR(info.st_mode) and not path.is_symlink()
    assert (info.st_dev, info.st_ino, info.st_uid) == (row['dev'], row['ino'], row['uid'])
    assert info.st_uid == os.geteuid() and path.resolve(strict=True) == path
    assert not any(path.iterdir()), 'preflight bind root is no longer empty; retained'
    return path
  # Reject changed roots before removing any, then recheck each immediately.
  for row in created:
    checked(row)
  for row in created:
    checked(row).rmdir()


def retain_seed_abi_preflight(work, destination):
  assert destination.is_dir() and not destination.is_symlink()
  for name in ('commands.log', 'native-bootstrap-context.json',
               'native-bootstrap-proot-build.json', 'native-cereal-inputs-build.jsonl',
               'native-bootstrap-seed-abi.json', 'native-preflight-source.txt',
               'native-preflight-status.txt'):
    path = work/name
    if path.exists():
      assert path.is_file() and not path.is_symlink()
      shutil.copyfile(path, destination/name)


def native_seed_abi_preflight(guest_command, work, source_mount, evidence, log):
  room(work, 2*1024**3)
  build_native_inputs(guest_command, work, source_mount, evidence, log)
  binary = work/'project-target/debug/openpilot-runtime-inputs'
  code = ("import json,runpy,sys; from pathlib import Path; "
          "seed=runpy.run_path(sys.argv[1]); binary=Path(sys.argv[2]); "
          "seed['native'](); seed['elf'](binary); "
          "print(json.dumps(seed['inspect_abi'](binary)))")
  evidence['commands'].append(run(guest_command([
      '/usr/bin/python3', '-B', '-c', code,
      f'{source_mount}/tools/native/bootstrap_seed.py', binary]), log,
      stdout_path=work/'native-bootstrap-seed-abi.json'))
  evidence['seed_abi'] = json.loads((work/'native-bootstrap-seed-abi.json').read_text())
  evidence['seed_binary_sha256'] = digest(binary)
  for argv, name in [(['git','-C',source_mount,'rev-parse','HEAD'], 'native-preflight-source.txt'),
                     (['git','-C',source_mount,'status','--porcelain','--untracked-files=all'], 'native-preflight-status.txt')]:
    evidence['commands'].append(run(guest_command(argv), log, stdout_path=work/name))
  assert (work/'native-preflight-source.txt').read_text().strip() == evidence['source_commit']
  assert not (work/'native-preflight-status.txt').read_text().strip()


def main():
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument('--bootstrap-seed-abi-preflight', action='store_true')
  preflight = parser.parse_args().bootstrap_seed_abi_preflight
  runpy.run_path(str(Path(__file__).with_name('runner_policy.py')))['require_github_native_arm']()
  assert platform.machine() == 'aarch64' and os.geteuid() != 0
  source = Path(os.environ['GITHUB_WORKSPACE']).resolve()
  sha = subprocess.check_output(['git', '-C', source, 'rev-parse', 'HEAD'], text=True).strip()
  assert sha == os.environ['GITHUB_SHA']
  owned = Path(os.environ['CI_JOB_TEMP'])
  assert owned.parent == Path('/tmp') and owned.name.startswith('openpilot-ci.') and not owned.is_symlink()
  assert (owned / 'owner').read_text() == ':'.join(os.environ[k] for k in ('GITHUB_RUN_ID', 'GITHUB_RUN_ATTEMPT', 'GITHUB_JOB'))
  work = owned / 'native-agnos'
  work.mkdir()
  work_identity = (work.stat().st_dev, work.stat().st_ino)
  preflight_evidence = owned/'native-bootstrap-seed-abi-preflight'
  if preflight:
    preflight_evidence.mkdir()
  log = work / 'commands.log'
  evidence = dict(schema=1, source_commit=sha, runner_name=os.environ['RUNNER_NAME'],
                  runner_environment=os.environ['RUNNER_ENVIRONMENT'], host_architecture=platform.machine(),
                  host_kernel=platform.release(), guest_os='AGNOS 19.8-carrot-bt1', execution_mode=NATIVE_PROOT['MODE'],
                  native_arm_execution=True, emulated=False, proot_used=True, agnos_kernel=False,
                  physical_arm_cpu=False, device_execution=False, full_runtime_build=False,
                  image_compressed_sha256='077d42186def98025ffad9723774c96f45da10c87cc6fedbc58d0cf4fa65ffd4',
                  image_raw_sha256=RAW_SHA, rustup_sha256=RUSTUP_SHA, commands=[])
  if preflight:
    evidence.update(preflight_only=True, published_release=False, payload_created=False,
                    full_probe_success=False, seed_exported=False, created_bind_roots=[])
  initial_disk = shutil.disk_usage(owned).free
  started = time.monotonic()
  try:
    receipt = json.loads((owned / 'native-agnos-assets-first.json').read_text())['agnos-system-19-8-carrot-bt1']
    repeated = json.loads((owned / 'native-agnos-assets-reused.json').read_text())['agnos-system-19-8-carrot-bt1']
    image = Path(receipt['path'])
    assert digest(image) == evidence['image_compressed_sha256'] and image.stat().st_size == 1010655092
    assert repeated['transferred_bytes'] == repeated['stored_bytes'] == 0 and repeated['path'] == receipt['path']
    evidence['image_first_transferred_bytes'] = receipt['transferred_bytes']
    evidence['image_repeated_transferred_bytes'] = repeated['transferred_bytes']
    room(work, RAW_BYTES + 64 * 1024**2)
    sparse = work / 'system.sparse'
    with lzma.open(image, 'rb') as compressed, sparse.open('wb') as output:
      total = 0
      while block := compressed.read(1024 * 1024):
        total += len(block)
        if total > RAW_BYTES + 64 * 1024**2:
          raise RuntimeError('verified image exceeds bounded sparse extraction')
        output.write(block)
    assert digest(sparse) == SPARSE_SHA
    room(work, RAW_BYTES)
    raw = work / 'system.raw'
    run(['simg2img', sparse, raw], log)
    assert raw.stat().st_size == RAW_BYTES and digest(raw) == RAW_SHA
    sparse.unlink()
    room(work, RAW_BYTES + 1024**3)
    rootfs = work / 'rootfs'
    rootfs.mkdir()
    run(['debugfs', '-R', f'rdump / {rootfs}', raw], log)
    assert (rootfs / 'VERSION').read_text().strip() == '19.8-carrot-bt1'
    sdk_profile = source/'rust/native-system-sdk.json'
    assert json.loads(sdk_profile.read_text())['factory_image_sha256'] == RAW_SHA
    host_preparer = Path((owned/'native-capnp-inputs-cli.txt').read_text().strip())
    # Derive expected bytes from the authenticated image before any guest runs.
    # Capturing the mutable live namespace would not establish factory origin.
    evidence['commands'].append(run([host_preparer,'--capture-system-sdk','--root',rootfs,
        '--plan',sdk_profile,'--output',work/'native-system-sdk.json'],log))
    evidence['system_sdk_expected_receipt_sha256'] = digest(work/'native-system-sdk.json')
    raw.unlink()
    room(work, 3 * 1024**3)
    proot = work / 'proot'
    rustup = work / 'rustup-init'
    proot_context = NATIVE_PROOT['build'](work, log, download, run)
    evidence['native_proot'] = proot_context
    download('https://static.rust-lang.org/rustup/archive/1.29.1/aarch64-unknown-linux-gnu/rustup-init', rustup, RUSTUP_SHA, log)
    guest_work = '/openpilot-native-build-probe-599'
    (rootfs / guest_work.lstrip('/')).mkdir()
    source_mount = '/openpilot-native-source-599'
    (rootfs / source_mount.lstrip('/')).mkdir()
    assert (source / '.git').is_dir() and not (source / '.git').is_symlink()
    (work / 'probe.gitconfig').write_text('[safe]\n'
        f'\tdirectory = {source_mount}\n\tdirectory = {guest_work}/driver-fixture\n'
        '[core]\n\thooksPath = /dev/null\n[commit]\n\tgpgSign = false\n')
    (work / 'pinned-rust').mkdir()
    (work / 'capnp-sdk').mkdir()
    (work / 'cargo-assets').mkdir()
    (work / 'cargo-sources').mkdir()
    (work / 'rust-assets').mkdir()
    (work / 'codec-assets').mkdir()
    (work / 'ffmpeg-source').mkdir()
    (work / 'provider-assets').mkdir()
    (work / 'provider-export').mkdir()
    for name in ('raylib-assets','raylib-components','raylib-provider-export','opencv-source','opencv-components','opencv-provider-export','source-lfs-assets','panda-components','panda-export','python-assets','model-wheel-assets','model-assets','dbc-components','dbc-export','usbgpu-components','usbgpu-export','usbgpu-model-assets'):
      (work/name).mkdir()
    cargo_inputs = json.loads((owned/'native-cargo-first.json').read_text())
    cargo_vendor = Path(guest_work)/'cargo-sources'/Path(cargo_inputs['vendor']).relative_to(owned/'native-cargo-sources')
    (work/'cargo').mkdir()
    (work/'cargo/config.toml').write_text('[source.crates-io]\nreplace-with="openpilot-native"\n'
        '[source.openpilot-native]\ndirectory='+json.dumps(str(cargo_vendor))+'\n[net]\noffline=true\n')
    evidence['cargo_source_mode'] = dict(directory=str(cargo_vendor),tree_sha256=cargo_inputs['tree_sha256'],
        cargo_config_sha256=digest(work/'cargo/config.toml'),net_offline=True,
        source='locked authenticated directory source; original commands and Rustup/toolchain pins retained')
    # The authenticated factory namespace already contains its native tools.
    # Build as the real unprivileged user: PRoot's fake-root DAC emulation
    # temporarily chmods immutable paths and can retain that mode after rename.
    if preflight:
      prepare_preflight_bind_roots(owned, evidence['created_bind_roots'])
    guest = NATIVE_PROOT['expected_guest_command'](work)
    def guest_command(args):
      return [*guest, *(str(arg).replace(str(work), guest_work) for arg in args)]
    context = dict(schema=1,
        source_commit=sha,work=str(work),guest_work=guest_work,source_mount=source_mount,
        guest=list(map(str,guest)), **proot_context)
    NATIVE_PROOT['verify_context'](context, work)
    (work/'native-bootstrap-context.json').write_text(json.dumps(context)+'\n')
    evidence['guest_architecture'] = subprocess.check_output(list(map(str, guest_command(['/bin/uname', '-m']))), text=True).strip()
    assert evidence['guest_architecture'] == 'aarch64', 'AGNOS guest is not executing ARM userspace'
    guest_uid = int(subprocess.check_output(list(map(str, guest_command(['/usr/bin/id', '-u']))), text=True))
    assert guest_uid == os.geteuid() != 0, 'native guest build must retain the real non-root identity'
    evidence['guest_identity'] = dict(mode='unprivileged', uid=guest_uid, host_uid=os.geteuid(), fake_root=False)
    if preflight:
      prepare_guest_rust(guest_command, work, rustup, evidence, log)
      native_seed_abi_preflight(guest_command, work, source_mount, evidence, log)
      NATIVE_PROOT['verify_context'](context, work)
      retain_seed_abi_preflight(work, preflight_evidence)
      assert not owned.is_symlink() and owned.resolve(strict=True) == owned
      assert not (owned/'owner').is_symlink()
      assert (owned/'owner').read_text() == ':'.join(os.environ[k] for k in ('GITHUB_RUN_ID', 'GITHUB_RUN_ATTEMPT', 'GITHUB_JOB'))
      assert not work.is_symlink() and work.resolve(strict=True) == owned/'native-agnos'
      assert (work.stat().st_dev, work.stat().st_ino) == work_identity and work.stat().st_uid == os.geteuid()
      cleanup_preflight_bind_roots(owned, evidence['created_bind_roots'])
      assert shutil.rmtree.avoids_symlink_attacks
      shutil.rmtree(work)
      evidence.update(success=True, work_removed_after_success=True)
      return

    evidence['commands'].append(run(guest_command(['/usr/bin/python3', '-B', '-m', 'unittest', 'discover',
        '-s', f'{source_mount}/tools/ci/tests', '-p', 'test_readonly_publication.py', '-v']), log,
        stdout_path=work/'native-readonly-publication.txt'))
    for argv in [['/bin/uname', '-m'], ['getconf', 'GNU_LIBC_VERSION'], ['cat', '/etc/os-release'],
                    ['gcc', '--version'], ['g++', '--version'], ['clang', '--version'], ['cmake', '--version'],
                    ['pkg-config', '--version'], ['capnp', '--version'], ['python3', '--version']]:
      evidence['commands'].append(run(guest_command(argv), log, required=False))
    prepare_guest_rust(guest_command, work, rustup, evidence, log)
    fixture = work / 'fixture'
    (fixture / 'src/bin').mkdir(parents=True)
    (fixture / 'Cargo.toml').write_text("[package]\nname='agnos-native-compiler-probe'\nversion='0.1.0'\nedition='2021'\n[workspace]\n")
    (fixture / 'Cargo.lock').write_text('version = 4\n\n[[package]]\nname = "agnos-native-compiler-probe"\nversion = "0.1.0"\n')
    (fixture / 'src/bin/first.rs').write_text('fn main(){println!("native AGNOS Rust std first 42");}\n')
    (fixture / 'src/bin/second.rs').write_text('fn main(){println!("native AGNOS Rust std second 42");}\n')
    (fixture / 'provider.cc').write_text('#include <iostream>\nint main(){std::cout << "native AGNOS C++ std 42" << std::endl;}\n')
    evidence['commands'].append(run(guest_command(['g++', '-std=c++17', fixture / 'provider.cc', '-o', fixture / 'provider']), log))
    evidence['commands'].append(run(guest_command([fixture / 'provider']), log))
    build = guest_command(['cargo', 'build', '--locked', '--offline', '--message-format=json', '--manifest-path', fixture / 'Cargo.toml', '--target-dir', work / 'target'])
    evidence['commands'].append(run(build, log, stdout_path=work / 'cargo-first.jsonl'))
    before = {name: digest(work / 'target/debug' / name) for name in ('first', 'second')}
    evidence['commands'].append(run(build, log, stdout_path=work / 'cargo-reused.jsonl'))
    assert before == {name: digest(work / 'target/debug' / name) for name in before}
    evidence['cargo_artifacts'] = {}
    for phase in ('first', 'reused'):
      rows = [json.loads(line) for line in (work / f'cargo-{phase}.jsonl').read_text().splitlines()]
      artifacts = [dict(name=row['target']['name'], fresh=row['fresh']) for row in rows if row['reason'] == 'compiler-artifact']
      assert len(artifacts) == 2 and all(row['fresh'] == (phase == 'reused') for row in artifacts)
      evidence['cargo_artifacts'][phase] = artifacts
    for name in before:
      evidence['commands'].append(run(guest_command([work / 'target/debug' / name]), log))
    evidence['binaries'] = before
    evidence['toolchain_files'] = {str(path.relative_to(work)): digest(path) for path in (work / 'rustup/toolchains').glob('*/bin/*') if path.is_file()}
    native_driver_probe(guest_command, work, source_mount, evidence, log, guest_work)
    native_cereal_probe(guest_command, work, source_mount, evidence, log, guest_work)
    native_bootstrap_seed_probe(guest_command, work, source_mount, evidence, log, guest_work)
    native_system_sdk_probe(guest_command, work, rootfs, evidence, log, guest_work)
    native_codec_probe(guest_command, work, source_mount, evidence, log, guest_work)
    native_ffmpeg_headers_probe(guest_command, work, source_mount, evidence, log, guest_work)
    native_provider_probe(guest_command, work, source_mount, evidence, log, guest_work)
    native_python_probe(guest_command, work, source_mount, evidence, log, guest_work)
    native_model_wheels_probe(guest_command, work, source_mount, evidence, log, guest_work)
    native_encoder_probe(guest_command, work, source_mount, evidence, log, guest_work)
    native_eigen_probe(guest_command, work, source_mount, evidence, log, guest_work)
    native_raylib_probe(guest_command, work, source_mount, evidence, log, guest_work)
    native_opencv_headers_probe(guest_command, work, source_mount, evidence, log, guest_work)
    native_opencv_prefix_probe(guest_command, work, source_mount, evidence, log, guest_work)
    native_runtime_companions_probe(guest_command, work, source_mount, evidence, log, guest_work)
    native_display_inputs_probe(guest_command, work, source_mount, evidence, log, guest_work)
    native_media_inputs_probe(guest_command, work, source_mount, evidence, log, guest_work)
    native_source_assets_probe(guest_command, work, source_mount, evidence, log, guest_work)
    native_mpc_inputs_probe(guest_command, work, source_mount, evidence, log, guest_work)
    native_panda_inputs_probe(guest_command, work, source_mount, evidence, log, guest_work)
    native_dbc_inputs_probe(guest_command, work, source_mount, evidence, log, guest_work)
    native_usbgpu_inputs_probe(guest_command, work, source_mount, evidence, log, guest_work)
    native_catalog_bindings_probe(guest_command, work, source_mount, evidence, log, guest_work)
    native_model_generation_probe(guest_command, work, source_mount, evidence, log, guest_work)
    native_complete_layout_probe(guest_command, work, source_mount, evidence, log, guest_work)
    # Add the production orchestration as a separate guarded command after all
    # original ARM commands. Keep their user space, pins and evidence intact.
    preserve = 'GITHUB_ACTIONS,RUNNER_ENVIRONMENT,RUNNER_NAME,RUNNER_OS,RUNNER_ARCH,CI_JOB_TEMP,GITHUB_RUN_ID,GITHUB_RUN_ATTEMPT,GITHUB_JOB,GITHUB_SHA,GITHUB_WORKSPACE,PATH'
    scope = Path(os.environ['GITHUB_WORKSPACE'])/'tools/ci/native_resource_scope.py'
    driver = Path(os.environ['GITHUB_WORKSPACE'])/'tools/ci/verify_native_runtime_inputs.py'
    evidence['commands'].append(run(['sudo', '-n', f'--preserve-env={preserve}', '/usr/bin/python3', scope,
        'enter', '--', '/usr/bin/python3', driver], log, stdout_path=work/'native-runtime-inputs-driver.txt'))
    evidence['native_production_runtime_inputs'] = json.loads((work/'runtime-inputs-evidence/summary.json').read_text())
    source_after = work/'native-project-final-source.txt'
    status_after = work/'native-project-final-status.txt'
    evidence['commands'].append(run(guest_command(['git','-C',source_mount,'rev-parse','HEAD']),log,stdout_path=source_after))
    evidence['commands'].append(run(guest_command(['git','-C',source_mount,'status','--porcelain','--untracked-files=all']),log,stdout_path=status_after))
    assert source_after.read_text().strip() == sha and not status_after.read_text().strip()
    NATIVE_PROOT['verify_context'](context, work)
    evidence['success'] = True
  except BaseException as error:
    evidence.update(success=False, failure=str(error))
    raise
  finally:
    evidence['elapsed_seconds'] = time.monotonic() - started
    evidence['observed_final_disk_growth_bytes'] = initial_disk - shutil.disk_usage(owned).free
    evidence['maximum_single_child_rss_kib'] = resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss
    evidence['measurement_limit'] = 'single-child maximum RSS; not aggregate process-tree peak, device time or complete runtime resource evidence'
    if preflight:
      if work.exists():
        retain_seed_abi_preflight(work, preflight_evidence)
      report = owned/'native-bootstrap-seed-abi-preflight.json'
    else:
      report = owned/'native-agnos-environment-599.json'
    report.write_text(json.dumps(evidence, indent=2) + '\n')


if __name__ == '__main__':
  main()
