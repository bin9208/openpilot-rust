#!/usr/bin/env python3
"""Actions-only full production entry check, using real factory ARM credentials.

No fixture index, provider archive, sudo replacement, or cross-job Cargo target
is accepted. Hardware startup is deliberately outside --verify-package.
"""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import select
import signal
import shutil
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[2]
CLEANUP_RESERVE_SECONDS = 60
FIELDS = {'schema', 'maximum_memory_bytes', 'minimum_available_memory_bytes',
          'maximum_growth_bytes', 'minimum_free_bytes', 'expected_source_growth_bytes',
          'expected_compile_growth_bytes', 'remaining_object_growth_bytes',
          'compiler_growth_bytes', 'build_timeout_seconds', 'budgets'}
LOGGER_HELP = 'Continuous native route logger.'
REUSE_LOGGER_HELP = 'Continuous native route logger (public source reuse proof).'
# loggerd/build.rs watches src, embedding the derived Git stamp in its library;
# bootlog consumes Environment::init_data, and manager depends on bootlog.
# Normal/target path-dependency closure is checked against Cargo.toml in tests.
# Cargo freshness and linked ELF changes are distinct, especially for manager.
LOGGER_CHANGE_CONSUMERS = {'07': 'openpilot-loggerd', '08': 'openpilot-bootlog', '09': 'openpilot-manager'}
LOGGER_REQUIRED_CHANGES = {'openpilot-loggerd', 'openpilot-bootlog'}


def module(path, name):
    spec = importlib.util.spec_from_file_location(name, path)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


def checksum(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def save(path, value):
    with path.open('x') as stream:
        json.dump(value, stream, indent=2)
        stream.write('\n')


def remaining(deadline_ns, cap=None):
    seconds = (deadline_ns-time.monotonic_ns())/1_000_000_000
    if seconds <= 0:
        raise TimeoutError('overall entry budget exhausted; startup policy is unchanged')
    return seconds if cap is None else min(seconds, cap)


def transfer_counts(value):
    """Read existing counters, stopping at aggregates to avoid double counting."""
    counts = {}
    def visit(row, path):
        if isinstance(row, dict):
            if 'transferred_bytes' in row:
                count = row['transferred_bytes']
                counters = count if isinstance(count, dict) else {'body': count}
                assert counters, 'empty asset transfer receipt: '+path
                for name, number in counters.items():
                    assert type(number) is int and number >= 0, 'invalid transfer counter: '+path
                    counts[path+'/'+name] = number
                return
            for name, child in row.items():
                visit(child, path+'/'+name)
        elif isinstance(row, list):
            for number, child in enumerate(row):
                visit(child, path+'/'+str(number))
    visit(value, '')
    assert counts, 'missing asset transferred_bytes receipt'
    return counts


def resource_receipt(entry, attempt, parent, source, configuration_pin, policy):
    attempt = Path(attempt)
    assert attempt.parent == parent and attempt.resolve(strict=True) == attempt
    path = attempt/'resources.jsonl'
    last = None
    # A long build journal can exceed the JSON receipt size bound; stream it.
    with entry.seed.regular(path) as stream:
        while line := stream.readline(4*1024**2+1):
            assert len(line) <= 4*1024**2
            last = json.loads(line, object_pairs_hook=entry.runtime.pairs)
    assert last is not None and last['state'] == 'success'
    assert last['context']['requested_source'] == source
    assert last['context']['configuration_sha256'] == configuration_pin
    assert last['policy'] == policy and last['elapsed_ms'] >= 0
    assert 0 < last['memory']['kernel_peak_bytes'] <= policy['maximum_memory_bytes']
    assert last['memory']['limit_events'] == 0 and last['filesystems']
    for disk in last['filesystems']:
        assert disk['budget'] in policy['filesystems'].values()
        assert disk['observed_peak_growth_bytes'] <= disk['budget']['maximum_growth_bytes']
        assert disk['minimum_available_bytes'] >= disk['budget']['minimum_free_bytes']
    return dict(path=str(path), sha256=checksum(path), final=last)


def sealed_build(entry, state, row, source, catalog):
    path = state/'generations'/(row['generation_id']+'.json')
    # Rust hashes its pretty JSON before generation::write appends one newline.
    sealed = entry.runtime.read(path)
    with entry.seed.regular(path) as stream:
        data = stream.read(4*1024**2+1)
    assert data.endswith(b'\n') and hashlib.sha256(data[:-1]).hexdigest() == row['generation_id']
    assert sealed['origin'] == 'source-build' and sealed['architecture'] == 'aarch64'
    assert sealed['agnos_version'] == '19.8-carrot-bt1' and sealed['startup_update_protocol'] == 1
    prepared = sealed['prepared']
    assert prepared['source']['source'] == sealed['runtime']['source'] == source
    groups, binaries = prepared['reports'], prepared['binaries']
    assert set(groups) == set(catalog['groups']) == {f'{n:02d}' for n in range(13)}
    assert len(binaries) == 99 and set(prepared['plan']['groups']) == set(groups)
    for name, report in groups.items():
        assert report['source'] == source
        expected = set(catalog['groups'][name]['binaries'])
        assert set(prepared['plan']['groups'][name]['binaries']) == expected
        artifacts = {Path(a['executable']).name: a for a in report['artifacts']}
        assert set(artifacts) == expected and len(artifacts) == len(report['artifacts'])
        for binary, artifact in artifacts.items():
            obj = binaries[binary]
            assert type(artifact['fresh']) is bool and obj['mode'] == 0o555
            assert (obj['sha256'], obj['bytes']) == (artifact['sha256'], artifact['bytes'])
            assert sealed['files']['rust/bin/'+binary] == obj
    assert set(binaries) == {b for g in catalog['groups'].values() for b in g['binaries']}
    # Retained object bytes, not mutable Cargo output paths, are authoritative.
    def object_path(obj):
        target = state/'objects'/(obj['sha256']+f'-{obj["mode"]:o}')/'blob'
        with entry.seed.regular(target) as stream:
            info = os.fstat(stream.fileno())
            assert info.st_size == obj['bytes'] and info.st_mode & 0o7777 == obj['mode']
            assert hashlib.file_digest(stream, 'sha256').hexdigest() == obj['sha256']
        return target
    for obj in binaries.values():
        object_path(obj)
    abi_path = object_path(sealed['abi_report'])
    abi = entry.runtime.read(abi_path, sealed['abi_report']['sha256'])
    assert abi['compatible'] is True
    logger = Path(sealed['runtime']['root'])/'rust/bin/openpilot-loggerd'
    assert logger.is_relative_to(state/'sources') and logger.resolve(strict=True) == logger
    assert checksum(logger) == binaries['openpilot-loggerd']['sha256']
    return dict(source=source, generation_id=row['generation_id'], descriptor_sha256=checksum(path),
        abi_report=sealed['abi_report'], binaries=binaries, groups=groups,
        stored_output_bytes=prepared['stored_output_bytes'],
        logger_executable=str(logger))


def verify_reuse(previous, current, phase):
    assert phase in ('reused', 'launch-package', 'docs-only', 'small-logger')
    assert set(current['groups']) == set(previous['groups']) == {f'{n:02d}' for n in range(13)}
    assert set(current['binaries']) == set(previous['binaries']) and len(current['binaries']) == 99
    for name, group in current['groups'].items():
        for field in ('cache_key', 'target_directory', 'recipe_sha256', 'rustc_sha256', 'cargo_sha256',
                      'toolchain_directory', 'native_tools', 'verified_input_trees'):
            assert group[field] == previous['groups'][name][field], 'build cache/input identity changed: '+name+'/'+field
    rebuilt = [name for name, group in current['groups'].items()
               if any(not a['fresh'] for a in group['artifacts'])]
    assert set(rebuilt) == (set(LOGGER_CHANGE_CONSUMERS) if phase == 'small-logger' else set()), 'unexpected rebuilt groups'
    for group, binary in LOGGER_CHANGE_CONSUMERS.items():
        artifacts = [a for a in current['groups'][group]['artifacts']
                     if Path(a['executable']).name == binary]
        assert len(artifacts) == 1 and artifacts[0]['fresh'] is (phase != 'small-logger')
    for group in current['groups'].values():
        for artifact in group['artifacts']:
            if phase != 'small-logger' or Path(artifact['executable']).name not in LOGGER_CHANGE_CONSUMERS.values():
                assert artifact['fresh'] is True, 'unaffected Cargo artifact rebuilt'
    changed = {name for name, obj in current['binaries'].items() if obj != previous['binaries'][name]}
    if phase == 'small-logger':
        assert LOGGER_REQUIRED_CHANGES <= changed <= set(LOGGER_CHANGE_CONSUMERS.values()), 'unexpected binary change'
    else:
        assert not changed, 'unexpected binary change'
    if phase != 'small-logger':
        assert current['stored_output_bytes'] == 0
    return rebuilt


def verify_warm_reuse(previous, current, phase):
    assert current['input_index_sha256'] == previous['input_index_sha256']
    assert current['machine_configuration_sha256'] == previous['machine_configuration_sha256']
    assert current['large_asset_transferred_bytes'] == 0, 'warm entry downloaded a large input'
    # Descriptor bodies are separate from seed/toolchain/provider/model/LFS bodies.
    assert all(n == 0 for n in current['metadata_transferred_bytes'].values()), 'warm metadata download'
    current['rebuilt_groups'] = verify_reuse(previous['sealed_build'], current['sealed_build'], phase)
    current['changed_binaries'] = sorted(name for name, obj in current['sealed_build']['binaries'].items()
        if obj != previous['sealed_build']['binaries'][name])


def verify_logger_help(entry, logger, state, deadline_ns):
    # Match generation::tool: prefer only the verified sealed library directory.
    with (state/'small-logger-help.stdout').open('xb') as out, (state/'small-logger-help.stderr').open('xb') as err:
        subprocess.run([str(logger), '--help'], env=dict(entry.seed.ENV,
            LD_LIBRARY_PATH=str(logger.parent/'lib'), LOG_ROOT=str(state/'logger-help-logs'),
            PARAMS_ROOT=str(state/'params'), OPENPILOT_PREFIX='native-public-reuse'),
            stdout=out, stderr=err, check=True, timeout=remaining(deadline_ns, 30))
    remaining(deadline_ns)
    help_path = state/'small-logger-help.stdout'
    assert help_path.stat().st_size < 65536 and REUSE_LOGGER_HELP in help_path.read_text()
    return checksum(help_path)


def reuse_git(entry, deadline_ns, *arguments):
    result = subprocess.run(['/usr/bin/git', '-c', 'user.name=Owned public source reuse',
        '-c', 'user.email=ci@example.invalid', '-c', 'commit.gpgsign=false',
        '-c', 'core.hooksPath=/dev/null', '-c', 'filter.lfs.required=false',
        '-c', 'filter.lfs.smudge=', '-c', 'filter.lfs.process=', *map(str, arguments)],
        env=dict(entry.seed.ENV, GIT_CONFIG_GLOBAL='/dev/null', GIT_CONFIG_SYSTEM='/dev/null',
                 GIT_LFS_SKIP_SMUDGE='1'), capture_output=True, check=True,
        timeout=remaining(deadline_ns))
    remaining(deadline_ns)
    return result.stdout.decode().strip()


def derive_source(entry, original, derived, phase, request, deadline_ns):
    assert phase in ('docs-only', 'small-logger')
    assert not derived.is_relative_to(original) and not original.is_relative_to(derived)
    assert entry.runtime.source_identity(original) == request['source']
    if phase == 'docs-only':
        assert not derived.exists() and derived.parent.resolve(strict=True) == derived.parent
        estimates = request['estimates']
        assert shutil.disk_usage(derived.parent).free >= (estimates['expected_source_growth_bytes']
                                                        + estimates['minimum_free_bytes'])
        reuse_git(entry, deadline_ns, 'clone', '--shared', '--no-checkout', '--', original, derived)
        reuse_git(entry, deadline_ns, '-C', derived, 'remote', 'set-url', 'origin',
                  'https://github.com/bin9208/openpilot-rust.git')
        reuse_git(entry, deadline_ns, '-C', derived, 'checkout', '--detach', request['source']['commit'])
    before = entry.runtime.source_identity(derived)
    name = 'README.md' if phase == 'docs-only' else 'rust/crates/loggerd/src/main.rs'
    path = derived/name
    assert path.resolve(strict=True) == path and not path.is_symlink()
    text = path.read_text()
    if phase == 'docs-only':
        assert before == request['source']
        text += '\nOwned public input production CLI documentation reuse proof.\n'
    else:
        assert text.count(LOGGER_HELP) == 1
        text = text.replace(LOGGER_HELP, REUSE_LOGGER_HELP, 1)
    path.write_text(text)
    reuse_git(entry, deadline_ns, '-C', derived, 'add', '--', name)
    reuse_git(entry, deadline_ns, '-C', derived, 'commit', '-m', '공개 입력 production CLI 재사용 검증: '+phase)
    identity = entry.runtime.source_identity(derived)
    assert identity['commit'] != before['commit'] and identity['tree'] != before['tree']
    assert reuse_git(entry, deadline_ns, '-C', derived, 'rev-parse', 'HEAD^') == before['commit']
    assert reuse_git(entry, deadline_ns, '-C', derived, 'diff', '--name-only', '--no-renames',
                     'HEAD^', 'HEAD', '--') == name
    assert entry.input_index(derived) == entry.input_index(original)
    assert entry.input_index(derived)[1] == request['index_sha256']
    assert entry.runtime.source_identity(original) == request['source']
    return dict(source=identity, parent_source=before, changed_path=name,
                changed_file_sha256=checksum(path), original_source=request['source'])


def verify_receipts(entry, row, state, request, index, machine_pin, source):
    # Only existing production receipts are read. In particular, the native
    # build receipt has no helper-SHA field: bind its unchanged CLI invocation
    # to the independently verified current tool bundle instead.
    runtime = entry.runtime
    assert runtime.source_identity(source) == request['source']
    assert row['source'] == request['source'] and row['input_index_sha256'] == request['index_sha256']
    assert row['machine_configuration_sha256'] == machine_pin and row['full_runtime_build'] is True
    assert all(row[k] is False for k in ('runtime_started', 'runtime_activated',
                                        'runtime_release_downloaded', 'staged_for_boot'))
    parent = state/'generations/source-requests'
    attempt, resolver = Path(row['evidence']), Path(row['resolver_evidence'])
    assert all(p.parent == parent and p.resolve(strict=True) == p for p in (attempt, resolver))
    resolved = runtime.read(resolver/'resolved.json')
    assert resolved['index_sha256'] == request['index_sha256']
    assert resolved['configuration_sha256'] == row['bootstrap_configuration_sha256']
    expected_metadata = dict(seed=index['seed'], system_sdk=index['system_sdk'], **index['providers'])
    assert set(row['metadata']) == set(resolved['receipts']) == set(expected_metadata)
    for role, asset in expected_metadata.items():
        for receipt in (row['metadata'][role], resolved['receipts'][role]):
            assert receipt['sha256'] == asset['sha256'] and receipt['bytes'] == asset['bytes']
    config = runtime.read(Path(resolved['configuration']), row['bootstrap_configuration_sha256'])
    assert config['stores'] == {name: str(state/name) for name in ('sources', 'objects', 'generations', 'params')}
    first = runtime.read(attempt/'seed-current-tools.json')
    assert first['seed_executed_for_current_tools'] is True and first['seed_native_abi_checked'] is True
    assert first['seed']['descriptor_sha256'] == index['seed']['sha256']
    seed_plan, _ = entry.seed.read_plan(Path(first['seed']['root'])/'manifest.json', index['seed']['sha256'])
    assert row['seed_source'] == first['seed']['seed_source'] == seed_plan['source']
    assert runtime.read(attempt/'tools-config.json') == {k: config[k] for k in runtime.TOOLS_FIELDS}
    assert first['current_tools']['configuration_sha256'] == checksum(attempt/'tools-config.json')
    assert first['current_tools']['seed_helper_sha256'] == seed_plan['assets']['helper']['sha256']
    assert checksum(Path(first['seed']['helper'])) == seed_plan['assets']['helper']['sha256']
    first_tools, first_pins = runtime.tools_from_report(first['current_tools'], request['source'], config)
    prepared = runtime.read(attempt/'prepared-inputs.json')
    assert prepared['configuration_sha256'] == row['bootstrap_configuration_sha256']
    assert prepared['seed_helper_sha256'] == first_pins[first_tools/'openpilot-runtime-inputs']
    tools, pins = runtime.tools_from_report(prepared, request['source'], config)
    inputs = prepared['runtime_inputs']
    assert inputs['source'] == request['source'] and inputs['runtime_inputs_verified'] is True
    assert inputs['runtime_roles'] == 18 and inputs['compiler_input_roles'] == 9
    assert inputs['configuration_sha256'] == config['runtime_inputs']['sha256']
    assert inputs['bootstrap_configuration_sha256'] == row['bootstrap_configuration_sha256']
    assert inputs['native_configuration'] == row['prepared_configuration']
    assert inputs['native_configuration_sha256'] == row['build_configuration_sha256']
    native = runtime.read(Path(inputs['native_configuration']), row['build_configuration_sha256'])
    assert native['stores'] == config['stores'] and native['cache'] == str(state/'cache/cargo-build')
    built = runtime.read(attempt/'built-generation.json')
    assert built['requested_source'] == request['source'] and built['generation_id'] == row['generation_id']
    assert built['configuration_sha256'] == row['build_configuration_sha256']
    assert built['activated'] is False and built['staged_for_boot'] is False
    output = Path(inputs['native_configuration']).parent
    assert output == Path(prepared['resource_evidence'])/'runtime-inputs'
    for name, pin in inputs['files'].items():
        path = Path(name)
        assert path.is_relative_to(output) and path.resolve(strict=True) == path and checksum(path) == pin
    operations_path = output/'operations.json'
    operations = runtime.read(operations_path, inputs['files'][str(operations_path)])
    asset_operations = {'capnp', 'codecs', 'raylib-sdk', 'ffmpeg-sdk', 'opencv-sdk', 'python',
                        'python-initial-preparation', 'models', 'source-lfs', 'usbgpu-model'}
    asset_operations.update('provider-'+role for role in entry.ROLES)
    assert set(operations) == asset_operations | {'catalog', 'model-compiler-build'}
    assert operations['catalog']['source'] == request['source']
    assert operations['catalog']['groups'] == 13 and operations['catalog']['binaries'] == 99
    assert operations['catalog']['compiler_inputs_bound'] is True
    assert operations['catalog']['compiled'] is False and operations['catalog']['activated'] is False
    transfers = {'seed': transfer_counts(row['seed_preparation'])}
    for scope, report in (('current-tools', first['current_tools']), ('runtime-inputs', prepared)):
        for role in ('rust', 'cargo'):
            transfers[scope+'/'+role] = transfer_counts(report['preparations'][role])
    for name in sorted(asset_operations):
        transfers[name] = transfer_counts(operations[name])
    for role in entry.ROLES:
        assert operations['provider-'+role]['origin_archive_downloaded'] is False
    metadata_transfers = {name: receipt['transferred_bytes'] for name, receipt in row['metadata'].items()}
    assert all(type(n) is int and n >= 0 for n in metadata_transfers.values())
    resources = {}
    for name, report, parent, configuration_pin in (
            ('current-tools', first['current_tools'], 'bootstrap-attempts', first['current_tools']['configuration_sha256']),
            ('runtime-inputs', prepared, 'bootstrap-attempts', row['bootstrap_configuration_sha256']),
            ('build-seal', built, 'build-attempts', row['build_configuration_sha256'])):
        resources[name] = resource_receipt(entry, report['resource_evidence'], state/'generations'/parent,
            request['source'], configuration_pin, config['resources'])
    catalog, _ = entry.git_json(source, 'rust/native-runtime-catalog.json')
    build = sealed_build(entry, state, row, request['source'], catalog)
    return dict(evidence=str(attempt), generation_id=built['generation_id'],
        source=request['source'], input_index_sha256=request['index_sha256'],
        machine_configuration_sha256=machine_pin, elapsed_ms=row['elapsed_ms'],
        bootstrap_configuration_sha256=row['bootstrap_configuration_sha256'],
        build_configuration_sha256=row['build_configuration_sha256'],
        metadata_transferred_bytes=metadata_transfers, asset_transferred_bytes=transfers,
        large_asset_transferred_bytes=sum(sum(counts.values()) for counts in transfers.values()),
        resource_receipts=resources, sealed_build=build,
        current_tools=dict(requested_source=prepared['requested_source'],
            bundle_original_verified_source=prepared['bundle_original_verified_source'],
            executable_sha256={p.name: sha for p, sha in pins.items()}),
        production_source_sha256={name: checksum(source/'tools/native'/name)
                                  for name in ('source_entry.py', 'source_runtime.py', 'bootstrap_seed.py')},
        seed_helper_sha256=seed_plan['assets']['helper']['sha256'],
        inputs_helper_sha256=prepared['seed_helper_sha256'],
        native_build_scope_helper_sha256=pins[tools/'openpilot-native-build-scope'],
        receipt_sha256={name: checksum(attempt/(name+'.json')) for name in
                       ('seed-current-tools', 'prepared-inputs', 'built-generation')})


def policy(raw, source):
    entry = module(source/'tools/native/source_entry.py', 'entry_policy')
    value = json.loads(raw, object_pairs_hook=entry.runtime.pairs)
    if (set(value) != FIELDS or type(value['schema']) is not int or value['schema'] != 1
            or any(not entry.positive(value[k]) for k in FIELDS-{'schema', 'budgets'})
            or not 120 <= value['build_timeout_seconds'] <= 18000):
        raise ValueError('explicit, bounded operation estimates are required')
    catalog, _ = entry.git_json(source, 'rust/native-runtime-catalog.json')
    budgets = value['budgets']
    if (set(budgets) != {'schema', 'group_growth_bytes', 'expected_source_growth_bytes',
                         'remaining_object_growth_bytes', 'estimate_basis'}
            or type(budgets['schema']) is not int or budgets['schema'] != 1
            or set(budgets['group_growth_bytes']) != set(catalog['groups'])
            or any(not entry.positive(v) for v in budgets['group_growth_bytes'].values())
            or any(not entry.positive(budgets[k]) for k in ('expected_source_growth_bytes', 'remaining_object_growth_bytes'))
            or not isinstance(budgets['estimate_basis'], str)
            or not 1 <= len(budgets['estimate_basis'].strip()) <= 4096):
        raise ValueError('explicit operation estimates must cover every current runtime group')
    return value


def preflight():
    transport = module(ROOT/'tools/ci/probe_native_source_transport.py', 'entry_transport')
    transport.source_module('runner_policy')['require_github_native_arm']()
    assert os.uname().machine == 'aarch64'
    context = transport.source_module('native_cache')['Context']()
    source = context.workspace
    entry = module(source/'tools/native/source_entry.py', 'entry_preflight')
    identity = entry.runtime.source_identity(source)
    if identity['commit'] != os.environ['GITHUB_SHA'] or not (source/'.git').is_dir():
        raise ValueError('an independent exact Actions checkout is required')
    # Read the exact committed Git blob; no candidate artifact or local fixture.
    index, pin = entry.input_index(source)
    if pin != os.environ['NATIVE_ENTRY_INDEX']:
        raise ValueError('committed public input index differs from dispatch pin')
    estimates = policy(os.environ['NATIVE_ENTRY_BUDGETS'], source)
    metadata = context.job/'source-entry-public-metadata'
    metadata.mkdir()
    transferred = {}
    assets = [('seed', index['seed']), ('system_sdk', index['system_sdk']), *index['providers'].items()]
    assert shutil.disk_usage(context.job).free >= sum(a['bytes']+65536 for _, a in assets)+estimates['minimum_free_bytes']
    for role, asset in assets:
        target = metadata/(role+'.json')
        count = entry.seed.fetch(asset, target)
        if target.stat().st_size != asset['bytes'] or checksum(target) != asset['sha256']:
            raise ValueError('public metadata body differs from committed pin: '+role)
        transferred[role] = count
        if role == 'seed':
            seed, _ = entry.seed.read_plan(target, asset['sha256'])
            if seed.get('protocols') != dict(current_tools=1, request_owner=2, generation_boot=1):
                raise ValueError('published boot seed lacks the required production protocols')
        else:
            descriptor = json.loads(target.read_bytes(), object_pairs_hook=entry.runtime.pairs)
            plan, _ = entry.git_json(source, 'rust/native-system-sdk.json' if role == 'system_sdk'
                                     else f'rust/native-{role}-provider.json')
            if role == 'system_sdk':
                if descriptor['plan'] != plan:
                    raise ValueError('public SDK descriptor differs from committed SDK plan')
            elif (descriptor['profile'] != plan['profile']
                  or descriptor['provider_source_commit'] != plan['source_commit']):
                raise ValueError('public provider descriptor differs from committed profile: '+role)
    if json.loads((source/'rust/native-system-sdk.json').read_text())['factory_image_sha256'] != transport.RAW_SHA:
        raise ValueError('factory image differs from the source SDK pin')
    save(context.job/'source-entry-request.json', dict(schema=1, source=identity,
        index_sha256=pin, seed=index['seed'], estimates=estimates,
        public_metadata_transferred_bytes=transferred))
    print(json.dumps(dict(source=identity, index_sha256=pin, public_metadata_verified=True,
        source_entry_full_build_executed=False, cross_job_cargo_cache_reused=False)))


def arm():
    import platform
    import pwd
    import grp
    assert platform.machine() == 'aarch64' and os.getuid() == os.geteuid() == 1000
    assert not os.getgroups() and Path('/VERSION').read_text().strip() == '19.8-carrot-bt1'
    assert pwd.getpwuid(1000).pw_name == 'comma' and pwd.getpwuid(1000).pw_gid == os.getgid()
    assert grp.getgrgid(os.getgid()).gr_name == 'comma'
    source, state = Path('/native/source'), Path('/native/state')
    request = json.loads((state/'request.json').read_text())
    estimates = request['estimates']
    deadline_ns = request['payload_deadline_monotonic_ns']
    remaining(deadline_ns)
    transport = module(source/'tools/ci/probe_native_source_transport.py', 'factory_transport')
    # Require the unchanged factory sudoers to authorize the actual original uid.
    with (state/'sudoers.stdout').open('xb') as out, (state/'sudoers.stderr').open('xb') as err:
        subprocess.run(['/usr/bin/sudo', '-n', '-l'], stdout=out, stderr=err, check=True, timeout=remaining(deadline_ns, 30))
    completed = subprocess.run(['/usr/bin/python3', '-I', '-B', '-S', '-c',
        transport.USER_PROBE, '1000', transport.ROOT_PROBE], capture_output=True, text=True,
        check=True, timeout=remaining(deadline_ns, 120))
    remaining(deadline_ns)
    admission = json.loads(completed.stdout)
    assert admission['factory_sudo_sha256'] == request['factory_sudo']['sha256']
    save(state/'factory-admission.json', admission)
    entry = module(source/'tools/native/source_entry.py', 'factory_entry')
    assert entry.runtime.source_identity(source) == request['source']
    index, pin = entry.input_index(source)
    assert pin == request['index_sha256'] and index['seed'] == request['seed']
    stores = {name: str(state/name) for name in ('sources', 'objects', 'generations', 'params')}
    for name in (*stores, 'cache', 'seed', 'updater'):
        (state/name).mkdir()
    (state/'params/d').mkdir()
    (state/'params/d/IsOnroad').write_bytes(b'0')
    machine = dict(schema=1, stores=stores, cache=str(state/'cache'), seed_cache=str(state/'seed'),
        updater_staging=str(state/'updater'), budgets=estimates['budgets'], boot_seed=index['seed'],
        **{k: estimates[k] for k in ('expected_source_growth_bytes', 'expected_compile_growth_bytes',
                                   'remaining_object_growth_bytes', 'compiler_growth_bytes')},
        resources=dict(schema=1, memory_cgroup_parent='/sys/fs/cgroup',
            maximum_memory_bytes=estimates['maximum_memory_bytes'],
            minimum_available_memory_bytes=estimates['minimum_available_memory_bytes'],
            filesystems={str(state): {k: estimates[k] for k in ('maximum_growth_bytes', 'minimum_free_bytes')}}))
    machine_path = state/'machine.json'
    save(machine_path, machine)
    entry.validate_machine(machine, source)
    machine_pin = checksum(machine_path)
    paths = state/'launch-paths.json'
    save(paths, dict(data=str(state), temporary='/tmp', system='/'))
    base = ['/usr/bin/python3', '-I', '-B', '-S', str(source/'tools/native/source_entry.py'),
        '--source', str(source), '--machine-config', str(machine_path),
        '--machine-configuration-sha256', machine_pin]
    rows, receipts = {}, {}
    requests = state/'generations/source-requests'
    for phase in ('initial', 'reused', 'launch-package'):
        before_launch = set(requests.glob('request-*/result.json')) if phase == 'launch-package' else set()
        arguments = base + (['--launch', '--verify-package', '--paths', str(paths)]
                            if phase == 'launch-package' else [])
        phase_started = time.monotonic_ns()
        with (state/(phase+'.stdout')).open('xb') as out, (state/(phase+'.stderr')).open('xb') as err:
            subprocess.run(arguments, env=entry.seed.ENV, stdout=out, stderr=err, check=True,
                           timeout=remaining(deadline_ns))
        cli_elapsed_ms = (time.monotonic_ns()-phase_started)//1_000_000
        remaining(deadline_ns)
        if phase == 'launch-package':
            # The launch CLI execs its helper; use its newly retained full build
            # result, rather than parsing mixed JSON/launcher diagnostics stderr.
            candidates = []
            for path in set(requests.glob('request-*/result.json'))-before_launch:
                candidate = entry.runtime.read(path)
                if 'resolver_evidence' in candidate:
                    assert Path(candidate['resolver_evidence']) == path.parent
                    candidates.append(candidate)
            assert len(candidates) == 1, 'fresh launch must retain exactly one production source-entry build result'
            row = candidates[0]
        else:
            row = entry.runtime.read(state/(phase+'.stdout'))
        receipts[phase] = verify_receipts(entry, row, state, request, index, machine_pin, source)
        receipts[phase]['cli_elapsed_ms'] = cli_elapsed_ms
        if phase != 'initial':
            verify_warm_reuse(receipts['initial'], receipts[phase], phase)
        save(state/(phase+'-proof.json'), receipts[phase])
        rows[phase] = row
        remaining(deadline_ns)
    # --verify-package returns before normal manager/hardware startup. Retained
    # startup config is checked by the production launcher, not manufactured here.
    startup = entry.startup_record(machine)
    sealed = entry.runtime.read(state/'generations'/(startup['initial_generation']+'.json'))
    assert sealed['runtime']['source'] == request['source']
    launch = rows['launch-package']
    assert startup['initial_generation'] == launch['generation_id']
    assert startup['configuration'] == launch['launch_configuration']
    assert startup['configuration_sha256'] == launch['launch_configuration_sha256'] == launch['build_configuration_sha256']
    assert entry.runtime.source_identity(source) == request['source']
    derived = state/'reuse-source'
    derived_sources = {}
    startup_pin = checksum(state/'generations/startup.json')
    for phase in ('docs-only', 'small-logger'):
        derived_sources[phase] = derive_source(entry, source, derived, phase, request, deadline_ns)
        assert derived_sources[phase]['parent_source'] == (request['source'] if phase == 'docs-only'
                                                         else derived_sources['docs-only']['source'])
        save(state/(phase+'-source.json'), derived_sources[phase])
        phase_request = dict(request, source=derived_sources[phase]['source'])
        phase_entry = module(derived/'tools/native/source_entry.py', 'entry_'+phase.replace('-', '_'))
        # The actual derived checkout's unchanged CLI uses exactly the same machine bytes/stores.
        arguments = ['/usr/bin/python3', '-I', '-B', '-S', str(derived/'tools/native/source_entry.py'),
            '--source', str(derived), '--machine-config', str(machine_path),
            '--machine-configuration-sha256', machine_pin]
        phase_started = time.monotonic_ns()
        with (state/(phase+'.stdout')).open('xb') as out, (state/(phase+'.stderr')).open('xb') as err:
            subprocess.run(arguments, env=entry.seed.ENV, stdout=out, stderr=err, check=True,
                           timeout=remaining(deadline_ns))
        cli_elapsed_ms = (time.monotonic_ns()-phase_started)//1_000_000
        remaining(deadline_ns)
        row = phase_entry.runtime.read(state/(phase+'.stdout'))
        proof = verify_receipts(phase_entry, row, state, phase_request, index, machine_pin, derived)
        proof['cli_elapsed_ms'] = cli_elapsed_ms
        assert proof['production_source_sha256'] == receipts['initial']['production_source_sha256']
        previous = receipts['docs-only'] if phase == 'small-logger' else receipts['initial']
        verify_warm_reuse(previous, proof, phase)
        if phase == 'small-logger':
            # Execute the verified sealed logger, with its original colocated libraries.
            logger = Path(proof['sealed_build']['logger_executable'])
            proof['logger_help_sha256'] = verify_logger_help(entry, logger, state, deadline_ns)
        assert checksum(machine_path) == machine_pin
        assert checksum(state/'generations/startup.json') == startup_pin
        assert entry.runtime.source_identity(source) == request['source']
        receipts[phase], rows[phase] = proof, row
        save(state/(phase+'-proof.json'), proof)
        remaining(deadline_ns)
    remaining(deadline_ns)
    save(state/'execution-summary.json', dict(schema=1, source=request['source'], index_sha256=pin,
        generation_id=startup['initial_generation'], factory_sudo_sha256=admission['factory_sudo_sha256'],
        actual_arm_sudo_executed=True, source_entry_full_build_executed=True,
        production_three_sudo_build_gates_executed=True, production_launch_verify_package_executed=True,
        generation_ids={k: row['generation_id'] for k, row in rows.items()},
        production_sudo_receipts=receipts, cleanup_verified=False,
        derived_sources=derived_sources,
        overall_budget_seconds=estimates['build_timeout_seconds'],
        factory_admission_in_overall_budget=True, startup_20_second_policy_changed=False,
        payload_finished_monotonic_ns=time.monotonic_ns(),
        cargo_cache_identity_path=str(state/'cache/cargo-build'),
        public_metadata_verified=True, warm_same_path_entry_completed=True,
        public_metadata_preflight_transferred_bytes=request['public_metadata_transferred_bytes'],
        cross_job_cargo_cache_reused=False, warm_zero_download_claimed=True,
        docs_and_small_change_reuse_tested=True, update_and_recovery_tested=False,
        physical_arm_cpu=False, agnos_kernel=False, proot_used=False,
        native_arm_execution=True, emulated=False, factory_identity=request['factory_identity'],
        manager_started=False, hardware_model_record_upload_tested=False,
        resource_policy_is_ci_fixture_not_device_budget=True))


def namespace_child(transport, uid, gid, work, factory, source, ready, go, group, inode, write):
    # This copy of namespace wiring is intentionally isolated from the admission
    # helper. See the scoped helper-extension proposal in the accompanying note.
    assert group.stat().st_ino == inode
    write(group/'cgroup.procs', os.getpid())
    transport.unshare(transport.NEWUSER)
    os.write(ready, b'U')
    assert os.read(go, 1) == b'M'
    assert os.getuid() == os.geteuid() == 0
    transport.unshare(transport.NEWNS | transport.NEWCGROUP)
    mount = lambda *args: transport.command('/usr/bin/mount', *args)
    mount('--make-rprivate', '/')
    memory = work/'memory'
    memory.mkdir()
    mount('-t', 'cgroup2', 'cgroup2', memory)
    assert memory.stat().st_ino == inode
    (memory/'control').mkdir()
    write(memory/'control/cgroup.procs', os.getpid())
    write(memory/'cgroup.subtree_control', '+memory')
    for name in ('dev', 'proc', 'sys'):
        mount('-t', 'tmpfs', 'tmpfs', factory/name)
    transport.readonly_proc_tree(factory/'proc')
    for name in ('null', 'zero', 'random', 'urandom'):
        target = factory/'dev'/name
        target.touch()
        mount('--bind', Path('/dev')/name, target)
    (factory/'dev/pts').mkdir()
    mount('-t', 'devpts', '-o', 'newinstance,ptmxmode=0666,mode=0620,gid=5', 'devpts', factory/'dev/pts')
    (factory/'dev/ptmx').symlink_to('pts/ptmx')
    (factory/'sys/fs/cgroup').mkdir(parents=True)
    mount('--bind', memory, factory/'sys/fs/cgroup')
    (factory/'native').mkdir()
    for name, path in (('source', source), ('state', work/'state')):
        target = factory/'native'/name
        target.mkdir()
        mount('--bind', path, target)
        if name == 'source':
            mount('-o', 'remount,bind,ro', target)
    os.chroot(factory)
    os.chdir('/')
    os.setgroups([])
    os.setgid(1000)
    os.setuid(1000)
    os.execve('/usr/bin/python3', ['/usr/bin/python3', '-I', '-B', '-S',
        '/native/source/tools/ci/verify_native_source_entry.py', '--inside-factory'],
        {'PATH': '/usr/sbin:/usr/bin:/sbin:/bin', 'LC_ALL': 'C', 'HOME': '/home/comma'})


def run():
    transport = module(ROOT/'tools/ci/probe_native_source_transport.py', 'entry_transport')
    transport.source_module('runner_policy')['require_github_native_arm']()
    assert os.uname().machine == 'aarch64'
    scope_module = transport.source_module('native_resource_scope')
    uid, gid, job, scope, version = scope_module['owned']()
    assert version == 2
    transport.factory_id_map(uid)
    transport.factory_id_map(gid)
    request = json.loads((job/'source-entry-request.json').read_text())
    source = Path(os.environ['GITHUB_WORKSPACE'])
    assert source == ROOT and source.resolve() == source
    # Git is checked again by the mapped uid1000 factory caller before sudo,
    # avoiding root safe.directory overrides for the runner-owned checkout.
    assert request['source']['commit'] == os.environ['GITHUB_SHA']
    raw = job/'native-agnos/system.raw'
    assert raw.resolve(strict=True) == raw and raw.stat().st_uid == uid
    assert raw.stat().st_size == transport.RAW_BYTES and checksum(raw) == transport.RAW_SHA
    expected = request['estimates']['maximum_growth_bytes']+request['estimates']['minimum_free_bytes']
    assert shutil.disk_usage(job).free >= transport.RAW_BYTES+expected
    work = job/'source-entry-full'
    work.mkdir()
    factory, state = work/'factory', work/'state'
    factory.mkdir()
    state.mkdir()
    os.chown(state, uid, gid)
    transport.command('/usr/sbin/debugfs', '-R', f'rdump / {factory}', raw, timeout=600)
    assert (factory/'VERSION').read_text().strip() == '19.8-carrot-bt1'
    request['factory_identity'] = transport.remap_factory_owners(factory, uid, gid)
    request['factory_sudo'] = transport.restore_factory_sudo(raw, factory/'usr/bin/sudo')
    # Only the private extracted image's resolver is updated. The host is read.
    resolver = factory/'etc/resolv.conf'
    if resolver.is_symlink():
        resolver.unlink()
    resolver.write_bytes(Path('/etc/resolv.conf').read_bytes())
    request['payload_started_monotonic_ns'] = time.monotonic_ns()
    request['payload_deadline_monotonic_ns'] = request['payload_started_monotonic_ns']+request['estimates']['build_timeout_seconds']*1_000_000_000
    save(state/'request.json', request)
    group = scope/'source-entry-full'
    group.mkdir()
    inode = group.stat().st_ino
    write = scope_module['write']
    original = transport.handlers()
    global_control = Path('/sys/fs/cgroup/cgroup.subtree_control').read_text()
    ready, send_ready = os.pipe()
    go, send_go = os.pipe()
    pid = os.fork()
    if pid == 0:
        os.close(ready)
        os.close(send_go)
        try:
            with (work/'namespace.stdout').open('xb') as out, (work/'namespace.stderr').open('xb') as err:
                os.dup2(out.fileno(), 1)
                os.dup2(err.fileno(), 2)
                namespace_child(transport, uid, gid, work, factory, source, send_ready, go, group, inode, write)
        except BaseException as error:
            os.write(2, f'full entry namespace: {type(error).__name__}: {error}\n'.encode())
        finally:
            os._exit(1)
    os.close(send_ready)
    os.close(go)
    pidfd = os.pidfd_open(pid)
    procfd = os.open(f'/proc/{pid}', os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    reaped = False
    result = None
    deadline_ns = request['payload_deadline_monotonic_ns']
    # This watchdog remains outside the payload cgroup. Its cleanup reserve is
    # separate from the one shared ARM admission/build/package-check budget.
    try:
        assert select.select([ready], [], [], remaining(deadline_ns, 30))[0]
        assert os.read(ready, 1) == b'U', 'new user namespace did not become ready'
        transport.write_factory_id_maps(procfd, uid, gid)
        os.write(send_go, b'M')
        while True:
            remaining(deadline_ns)
            waited, status = os.waitpid(pid, os.WNOHANG)
            if waited:
                reaped = True
                break
            time.sleep(remaining(deadline_ns, 0.2))
        remaining(deadline_ns)
        if os.waitstatus_to_exitcode(status):
            raise RuntimeError('factory entry failed; see source-entry-full namespace and state stderr')
        result = json.loads((state/'execution-summary.json').read_text())
        assert result['source'] == request['source'] and result['index_sha256'] == request['index_sha256']
        assert result['payload_finished_monotonic_ns'] < deadline_ns
        remaining(deadline_ns)
    finally:
        cleanup_deadline_ns = time.monotonic_ns()+CLEANUP_RESERVE_SECONDS*1_000_000_000
        cleanup = dict(schema=1, source=request['source'], index_sha256=request['index_sha256'],
            cleanup_reserve_seconds=CLEANUP_RESERVE_SECONDS, scope_path=str(group), scope_inode=inode,
            outer_job_scope_cleanup_is_separate=True, owned_cgroup_removed=False,
            shared_binfmt_unchanged=False, shared_controller_unchanged=False)
        try:
            assert group.stat().st_ino == inode and group.parent == scope
            write(group/'cgroup.kill', 1)
            if not reaped:
                try:
                    signal.pidfd_send_signal(pidfd, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                if not select.select([pidfd], [], [], remaining(cleanup_deadline_ns))[0]:
                    raise TimeoutError('owned entry child did not exit within cleanup reserve')
                os.waitpid(pid, 0)
            while True:
                remaining(cleanup_deadline_ns)
                groups = [group, *(p for p in group.rglob('*') if p.is_dir() and not p.is_symlink())]
                assert len(groups) <= 1024
                if all(not (p/'cgroup.procs').read_text().strip() for p in groups):
                    for path in sorted(groups, key=lambda p: len(p.parts), reverse=True):
                        path.rmdir()
                    cleanup['owned_cgroup_removed'] = True
                    break
                time.sleep(remaining(cleanup_deadline_ns, 0.05))
            cleanup['shared_binfmt_unchanged'] = transport.handlers() == original
            cleanup['shared_controller_unchanged'] = Path('/sys/fs/cgroup/cgroup.subtree_control').read_text() == global_control
            assert cleanup['shared_binfmt_unchanged'] and cleanup['shared_controller_unchanged']
            remaining(cleanup_deadline_ns)
        finally:
            for fd in (pidfd, procfd, ready, send_go):
                os.close(fd)
            # Uploaded by the existing state/*.json artifact path even on failure.
            save(state/'cleanup.json', cleanup)
    # Any payload/cleanup exception skips this final success result entirely.
    assert result is not None and cleanup['owned_cgroup_removed']
    result.update(cleanup_verified=True, cleanup_evidence_sha256=checksum(state/'cleanup.json'),
        host_kernel=os.uname().release, host_architecture=os.uname().machine,
        factory_image_sha256=transport.RAW_SHA, native_arm_execution=True, emulated=False)
    save(work/'result.json', result)


if __name__ == '__main__':
    if not (sys.flags.isolated and sys.flags.no_site and sys.flags.dont_write_bytecode):
        raise SystemExit('requires /usr/bin/python3 -I -B -S')
    if sys.argv[1:] == ['--preflight']:
        preflight()
    elif sys.argv[1:] == ['--run']:
        run()
    elif sys.argv[1:] == ['--inside-factory']:
        arm()
    else:
        raise SystemExit('--preflight | --run | --inside-factory')
