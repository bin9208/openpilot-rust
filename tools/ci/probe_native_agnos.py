#!/usr/bin/env python3
"""#599: execute a compiler feasibility probe in the genuine pinned AGNOS image.

Actions only. This does not boot an AGNOS kernel, flash a device or build the full
runtime. Its isolated toy binaries establish native toolchain/standard ABI facts.
"""
import hashlib
import json
import lzma
import os
import platform
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
  evidence['native_project_catalog'] = dict(source_commit=actual_source, planner_sha256=digest(planner),
      catalog_sha256=catalog['catalog_sha256'], groups=catalog['groups'], binaries=catalog['binaries'],
      full_runtime_build=False)
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
  sdk = owned / 'native-capnp'
  guest_sdk = Path(guest_work) / 'capnp-sdk'
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
      original_python_full_schema_cases=2,exact_wire_bytes_match=True,repeated_artifact_fresh=True,
      sdk_unchanged=True,actual_guest_execution=True,full_runtime_build=False,device_execution=False)


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
    guest = [proot, '-0', '-r', rootfs, '-q', qemu, '-b', '/proc', '-b', '/dev', '-b', '/etc/resolv.conf',
             '-b', '/etc/hosts', '-b', f'{work}:{guest_work}', '-b', f'{owned / "native-rust-toolchains"}:{guest_work}/pinned-rust',
             '-b', f'{owned / "native-capnp"}:{guest_work}/capnp-sdk',
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
