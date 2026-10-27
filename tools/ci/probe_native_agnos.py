#!/usr/bin/env python3
"""#599: execute a compiler feasibility probe in the genuine pinned AGNOS image.

Actions only. This does not boot an AGNOS kernel, flash a device or build the full
runtime. It checks native toolchain/ABI facts and the actual Rust encoder against
the unchanged source codec methods in an emulated ARM user space.
"""
import hashlib
import json
import lzma
import os
import platform
import re
import resource
import shutil
import subprocess
import time
from pathlib import Path

RAW_BYTES = 4718592000
SPARSE_SHA = 'a67410a888105ecce7bcc05e17cda77fd10572aa4380e9de242a7c92f3497114'
RAW_SHA = '375c5d22335770ac08750660bb5b29a3331c550d6a9875b35f36b88af792ea44'
PROOT_SHA = 'db185300a9daabbce86aca7632d8c7fdc4249f6f153171954013b70c5f491ec0'
RUSTUP_SHA = '15f6e4ce9f583b929c996c91562bad6d4454f3281de858b02cdfdef615fac433'


def digest(path):
  with Path(path).open('rb') as stream:
    return hashlib.file_digest(stream, 'sha256').hexdigest()


def room(path, growth):
  free = shutil.disk_usage(path).free
  print(json.dumps(dict(path=str(path), available_bytes=free, expected_additional_bytes=growth)), flush=True)
  if free < growth:
    raise RuntimeError('insufficient space for this operation; other data retained')


def run(argv, log, *, required=True, stdout_path=None):
  started = time.monotonic()
  with log.open('ab') as output:
    output.write((json.dumps(list(map(str, argv))) + '\n').encode())
    output.flush()
    if stdout_path is None:
      result = subprocess.run(list(map(str, argv)), stdout=output, stderr=subprocess.STDOUT, check=False)
    else:
      with stdout_path.open('wb') as stdout:
        result = subprocess.run(list(map(str, argv)), stdout=stdout, stderr=output, check=False)
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
  execute(['cargo','build','--locked','--manifest-path',f'{source_mount}/rust/Cargo.toml',
           '-p','openpilot-runtime-build','--bin','openpilot-runtime-inputs',
           '--target-dir',work/'project-target','--message-format=json'],'native-cereal-inputs-build.jsonl')
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
      '--','--nocapture','--test-threads=1']),log,stdout_path=output))
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
      emulation='QEMU/PRoot ARM user space on x64 bin-actions',full_runtime_build=False,
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
  },runtime_inputs=dict(files={},trees={}))
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
    assert plan['runtime_inputs']==dict(files={},trees={})
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
      emulation='QEMU/PRoot ARM user space on x64 bin-actions')


def main():
  assert os.environ.get('RUNNER_NAME') == 'bin-actions'
  assert os.environ.get('RUNNER_ENVIRONMENT') == 'self-hosted'
  source = Path(os.environ['GITHUB_WORKSPACE']).resolve()
  sha = subprocess.check_output(['git', '-C', source, 'rev-parse', 'HEAD'], text=True).strip()
  assert sha == os.environ['GITHUB_SHA']
  owned = Path(os.environ['CI_JOB_TEMP'])
  assert owned.parent == Path('/tmp') and owned.name.startswith('openpilot-ci.') and not owned.is_symlink()
  assert (owned / 'owner').read_text() == ':'.join(os.environ[k] for k in ('GITHUB_RUN_ID', 'GITHUB_RUN_ATTEMPT', 'GITHUB_JOB'))
  work = owned / 'native-agnos'
  work.mkdir()
  log = work / 'commands.log'
  evidence = dict(schema=1, source_commit=sha, runner_name=os.environ['RUNNER_NAME'],
                  runner_environment=os.environ['RUNNER_ENVIRONMENT'], host_architecture=platform.machine(),
                  host_kernel=platform.release(), guest_os='AGNOS 19.8-carrot-bt1', execution_mode='QEMU user / PRoot',
                  physical_arm_cpu=False, device_execution=False, full_runtime_build=False,
                  image_compressed_sha256='077d42186def98025ffad9723774c96f45da10c87cc6fedbc58d0cf4fa65ffd4',
                  image_raw_sha256=RAW_SHA, proot_sha256=PROOT_SHA, rustup_sha256=RUSTUP_SHA, commands=[])
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
    download('https://github.com/proot-me/proot/releases/download/v5.5.0/proot', proot, PROOT_SHA, log)
    download('https://static.rust-lang.org/rustup/archive/1.29.1/aarch64-unknown-linux-gnu/rustup-init', rustup, RUSTUP_SHA, log)
    qemu = Path('/usr/bin/qemu-aarch64-static')
    evidence['qemu_sha256'] = digest(qemu)
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
    for name in ('raylib-assets','raylib-components','raylib-provider-export','opencv-source','opencv-components','opencv-provider-export'):
      (work/name).mkdir()
    cargo_inputs = json.loads((owned/'native-cargo-first.json').read_text())
    cargo_vendor = Path(guest_work)/'cargo-sources'/Path(cargo_inputs['vendor']).relative_to(owned/'native-cargo-sources')
    (work/'cargo').mkdir()
    (work/'cargo/config.toml').write_text('[source.crates-io]\nreplace-with="openpilot-native"\n'
        '[source.openpilot-native]\ndirectory='+json.dumps(str(cargo_vendor))+'\n[net]\noffline=true\n')
    evidence['cargo_source_mode'] = dict(directory=str(cargo_vendor),tree_sha256=cargo_inputs['tree_sha256'],
        cargo_config_sha256=digest(work/'cargo/config.toml'),net_offline=True,
        source='locked authenticated directory source; original commands and Rustup/toolchain pins retained')
    guest = [proot, '-0', '-r', rootfs, '-q', qemu, '-b', '/proc', '-b', '/dev', '-b', '/etc/resolv.conf',
             '-b', '/etc/hosts', '-b', f'{work}:{guest_work}', '-b', f'{owned / "native-rust-toolchains"}:{guest_work}/pinned-rust',
             '-b', f'{owned / "native-capnp"}:{guest_work}/capnp-sdk',
             '-b', f'{owned / "native-capnp-assets"}:{guest_work}/capnp-assets',
             '-b', f'{owned / "native-cargo-assets"}:{guest_work}/cargo-assets',
             '-b', f'{owned / "native-cargo-sources"}:{guest_work}/cargo-sources',
             '-b', f'{owned / "native-rust-assets"}:{guest_work}/rust-assets',
             '-b', f'{owned / "native-codec-assets"}:{guest_work}/codec-assets',
             '-b', f'{owned / "native-eigen-assets"}:{guest_work}/eigen-assets',
             '-b', f'{owned / "native-ffmpeg-source"}:{guest_work}/ffmpeg-source',
             '-b', f'{owned / "native-provider-components"}:{guest_work}/provider-assets',
             '-b', f'{owned / "native-provider-export"}:{guest_work}/provider-export',
             '-b', f'{owned / "native-raylib-assets"}:{guest_work}/raylib-assets',
             '-b', f'{owned / "native-raylib-components"}:{guest_work}/raylib-components',
             '-b', f'{owned / "native-raylib-provider-export"}:{guest_work}/raylib-provider-export',
             '-b', f'{owned / "native-opencv-source"}:{guest_work}/opencv-source',
             '-b', f'{owned / "native-opencv-components"}:{guest_work}/opencv-components',
             '-b', f'{owned / "native-opencv-provider-export"}:{guest_work}/opencv-provider-export',
             '-b', f'{source}:{source_mount}', '-w', guest_work, '/usr/bin/env', '-i', 'HOME=/root',
             f'CARGO_HOME={guest_work}/cargo', f'RUSTUP_HOME={guest_work}/rustup', 'CARGO_BUILD_JOBS=1', 'CARGO_INCREMENTAL=0',
             'CARGO_PROFILE_DEV_DEBUG=0', 'CARGO_PROFILE_TEST_DEBUG=0', 'GIT_CONFIG_NOSYSTEM=1',
             f'GIT_CONFIG_GLOBAL={guest_work}/probe.gitconfig', 'GIT_OPTIONAL_LOCKS=0',
             f'PATH={guest_work}/cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin']
    def guest_command(args):
      return [*guest, *(str(arg).replace(str(work), guest_work) for arg in args)]
    evidence['guest_architecture'] = subprocess.check_output(list(map(str, guest_command(['/bin/uname', '-m']))), text=True).strip()
    assert evidence['guest_architecture'] == 'aarch64', 'AGNOS guest is not executing ARM userspace'
    for argv in [['/bin/uname', '-m'], ['getconf', 'GNU_LIBC_VERSION'], ['cat', '/etc/os-release'],
                    ['gcc', '--version'], ['g++', '--version'], ['clang', '--version'], ['cmake', '--version'],
                    ['pkg-config', '--version'], ['capnp', '--version'], ['python3', '--version']]:
      evidence['commands'].append(run(guest_command(argv), log, required=False))
    evidence['commands'].append(run(guest_command([rustup, '-y', '--default-toolchain', '1.94.0', '--profile', 'minimal', '--no-modify-path']), log))
    for argv in [['rustc', '-vV'], ['cargo', '-vV'], ['rustc', '--print', 'sysroot'], ['gcc', '-print-search-dirs']]:
      evidence['commands'].append(run(guest_command(argv), log))
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
    native_encoder_probe(guest_command, work, source_mount, evidence, log, guest_work)
    native_eigen_probe(guest_command, work, source_mount, evidence, log, guest_work)
    native_raylib_probe(guest_command, work, source_mount, evidence, log, guest_work)
    native_opencv_headers_probe(guest_command, work, source_mount, evidence, log, guest_work)
    native_opencv_prefix_probe(guest_command, work, source_mount, evidence, log, guest_work)
    native_catalog_bindings_probe(guest_command, work, source_mount, evidence, log, guest_work)
    source_after = work/'native-project-final-source.txt'
    status_after = work/'native-project-final-status.txt'
    evidence['commands'].append(run(guest_command(['git','-C',source_mount,'rev-parse','HEAD']),log,stdout_path=source_after))
    evidence['commands'].append(run(guest_command(['git','-C',source_mount,'status','--porcelain','--untracked-files=all']),log,stdout_path=status_after))
    assert source_after.read_text().strip() == sha and not status_after.read_text().strip()
    evidence['success'] = True
  except BaseException as error:
    evidence.update(success=False, failure=str(error))
    raise
  finally:
    evidence['elapsed_seconds'] = time.monotonic() - started
    evidence['observed_final_disk_growth_bytes'] = initial_disk - shutil.disk_usage(owned).free
    evidence['maximum_single_child_rss_kib'] = resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss
    evidence['measurement_limit'] = 'single-child maximum RSS; not aggregate process-tree peak, device time or complete runtime resource evidence'
    (owned / 'native-agnos-environment-599.json').write_text(json.dumps(evidence, indent=2) + '\n')


if __name__ == '__main__':
  main()
