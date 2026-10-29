#!/usr/bin/env python3
"""Exercise the pinned ARM seed's guarded current-tools build on a real host cgroup.

Called once through native_resource_scope enter, after credentials are dropped.
The AGNOS user space and emulation pins come from the original genuine probe.
"""
import hashlib
import json
import os
import runpy
from pathlib import Path
import shutil
import subprocess


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def main():
    assert os.geteuid() != 0 and not os.getgroups()
    runpy.run_path(str(Path(__file__).resolve().parent/'runner_policy.py'))['require_environment'](self_hosted_only=True)
    job = Path(os.environ['CI_JOB_TEMP'])
    assert job.parent == Path('/tmp') and job.name.startswith('openpilot-ci.') and job.resolve() == job
    owner = ':'.join(os.environ[k] for k in ('GITHUB_RUN_ID', 'GITHUB_RUN_ATTEMPT', 'GITHUB_JOB'))
    assert (job/'owner').read_text() == owner
    work = job/'native-agnos'
    context = json.loads((work/'native-bootstrap-context.json').read_text())
    assert context['schema'] == 1
    assert context['source_commit'] == os.environ['GITHUB_SHA']
    assert context['work'] == str(work)
    source = Path(os.environ['GITHUB_WORKSPACE'])
    guest_work = Path(context['guest_work'])
    guest = context['guest']
    assert guest[0] == str(work/'proot') and '-0' not in guest
    assert digest(work/'proot') == context['proot_sha256']
    cache = work/'bootstrap-current-cache'
    cache.mkdir()
    for name in ('rust-assets', 'cargo-assets'):
        (cache/name).mkdir()
    owner_fields = Path('/proc/self/stat').read_text().rsplit(') ', 1)[1].split()
    request_owner = f'{os.getpid()}:{int(owner_fields[19])}'
    # Reuse the original authenticated raw assets through owned PRoot binds.
    # The test does not fetch a runtime archive or copy existing raw assets.
    guest = [guest[0], '-b', '/sys/fs/cgroup',
        '-b', f'{job/"native-rust-assets"}:{guest_work}/bootstrap-current-cache/rust-assets',
        '-b', f'{job/"native-cargo-assets"}:{guest_work}/bootstrap-current-cache/cargo-assets',
        *guest[1:], f'OPENPILOT_NATIVE_REQUEST_OWNER={request_owner}']
    fixture = work/'bootstrap-current-source'
    evidence = work/'bootstrap-current-evidence'
    evidence.mkdir()

    def execute(arguments, name, required=True):
        command = [*guest, *(str(arg).replace(str(work), str(guest_work)) for arg in arguments)]
        output = evidence/name
        with output.open('xb') as stdout, (evidence/(name+'.stderr')).open('xb') as stderr:
            result = subprocess.run(command, stdout=stdout, stderr=stderr, check=False)
        if required and result.returncode:
            raise RuntimeError(f'guarded bootstrap command failed ({result.returncode}): {name}; retained output {output}')
        return output, result.returncode

    git = ['/usr/bin/git', '-c', 'user.name=Owned bootstrap fixture', '-c', 'user.email=ci@example.invalid',
        '-c', 'commit.gpgsign=false', '-c', 'core.hooksPath=/dev/null']
    seed = json.loads((work/'native-bootstrap-reused.json').read_text())
    manifest = json.loads((work/'bootstrap-seed-export/manifest.json').read_text())
    helper = seed['helper']
    host_helper = work/Path(helper).relative_to(guest_work)
    assert digest(host_helper) == manifest['assets']['helper']['sha256']
    assert seed['descriptor_sha256'] == digest(work/'bootstrap-seed-export/manifest.json')
    rust_plan = json.loads((source/'rust/native-rust-toolchain.json').read_text())
    cargo_plan = json.loads((source/'rust/native-cargo-inputs.json').read_text())
    rust_growth = sum(row['maximum_unpacked_bytes'] for row in rust_plan['components'].values())
    # Cargo unpacked package objects and the shared assembled directory source.
    cargo_growth = 2*sum(row['unpacked_bytes'] for row in cargo_plan['packages'])
    tree = subprocess.check_output(['git', '-C', str(source), 'ls-tree', '-r', '-l', '-z', 'HEAD'])
    source_bytes = sum(int(row.split(b'\t', 1)[0].split()[3]) for row in tree.split(b'\0') if row and row.split(b'\t', 1)[0].split()[3] != b'-')
    source_growth = 2*source_bytes+64*1024**2
    compile_growth = 2*1024**3  # Serial release compilation of these 13 tools, not all runtime targets.
    object_growth = 512*1024**2
    growth = rust_growth+cargo_growth+source_growth+compile_growth+object_growth
    assert shutil.disk_usage(work).free >= growth
    execute(['/usr/bin/env', 'GIT_LFS_SKIP_SMUDGE=1', *git, 'clone', '--shared', '--no-checkout',
        context['source_mount'], fixture], 'source-clone.txt')
    execute([*git, '-C', fixture, 'remote', 'set-url', 'origin', 'https://github.com/bin9208/openpilot-rust.git'], 'source-origin.txt')
    execute(['/usr/bin/env', 'GIT_LFS_SKIP_SMUDGE=1', *git, '-C', fixture, 'checkout', '--detach', context['source_commit']], 'source-checkout.txt')
    stores = {name: str(guest_work/f'bootstrap-current-{name}') for name in ('sources', 'objects', 'generations', 'params')}
    for name in ('sources', 'objects', 'generations', 'params'):
        (work/f'bootstrap-current-{name}').mkdir()
    params = work/'bootstrap-current-params/d'
    params.mkdir()
    (params/'IsOnroad').write_bytes(b'0')
    active = work/'bootstrap-current-generations/active.json'
    log = work/'bootstrap-current-generations/previous-log'
    active.write_bytes(b'owned previous selection sentinel\n')
    log.write_bytes(b'owned previous log sentinel\n')
    config = dict(schema=1, stores=stores, cache=str(guest_work/'bootstrap-current-cache'),
        system_sdk_receipt=str(guest_work/'native-system-sdk.json'), system_sdk_sha256=digest(work/'native-system-sdk.json'),
        expected_source_growth_bytes=source_growth, expected_compile_growth_bytes=compile_growth,
        remaining_object_growth_bytes=object_growth,
        resources=dict(schema=1, memory_cgroup_parent=os.environ['NATIVE_TEST_CGROUP_PARENT'],
            maximum_memory_bytes=3*1024**3, minimum_available_memory_bytes=128*1024**2,
            filesystems={str(guest_work):dict(maximum_growth_bytes=growth, minimum_free_bytes=64*1024**2)}))
    configuration = work/'bootstrap-current-config.json'
    configuration.write_text(json.dumps(config)+'\n')
    configuration_sha256 = digest(configuration)
    command = [helper, '--bootstrap-tools', '--config', configuration,
        '--configuration-sha256', configuration_sha256, '--root', fixture]
    reports = {}
    metrics = {}
    initial_hashes = None
    for phase in ('initial', 'reused', 'docs', 'small'):
        if phase == 'docs':
            path = fixture/'README.md'
            path.write_bytes(path.read_bytes()+b'\nOwned bootstrap docs reuse fixture.\n')
            execute([*git, '-C', fixture, 'add', 'README.md'], 'docs-add.txt')
            execute([*git, '-C', fixture, 'commit', '-m', 'Owned bootstrap docs reuse fixture'], 'docs-commit.txt')
        elif phase == 'small':
            path = fixture/'rust/crates/runtime-build/src/main.rs'
            original = path.read_text()
            changed = original.replace('Usage: openpilot-runtime-build', 'Owned bootstrap reuse marker. Usage: openpilot-runtime-build', 1)
            assert original != changed
            path.write_text(changed)
            execute([*git, '-C', fixture, 'add', 'rust/crates/runtime-build/src/main.rs'], 'small-add.txt')
            execute([*git, '-C', fixture, 'commit', '-m', 'Owned bootstrap executable behavior fixture'], 'small-commit.txt')
        output, _ = execute(command, phase+'.json')
        row = json.loads(output.read_text())
        assert row['configuration_sha256'] == configuration_sha256 == digest(configuration)
        assert not row['runtime_activated'] and not row['runtime_started'] and not row['runtime_release_downloaded']
        assert row['seed_helper_sha256'] == manifest['assets']['helper']['sha256']
        assert len(row['build']['binaries']) == 13 and len(row['build']['reports']) == 1
        artifacts = row['build']['reports']['bootstrap-tools']['artifacts']
        hashes = {Path(a['executable']).name:a['sha256'] for a in artifacts}
        assert len(hashes) == 13
        assert all(a['transferred_bytes'] == 0 for kind in ('rust', 'cargo') for a in row['preparations'][kind]['assets'].values())
        if phase != 'initial':
            assert row['preparations']['rust']['copied_bytes'] == 0
            assert row['preparations']['cargo']['copied_bytes'] == row['preparations']['cargo']['linked_bytes'] == 0
        if phase == 'initial':
            assert not any(a['fresh'] for a in artifacts)
            initial_hashes = hashes
        elif phase in ('reused', 'docs'):
            assert all(a['fresh'] for a in artifacts) and hashes == initial_hashes
            assert row['build']['stored_output_bytes'] == 0
            assert row['tools'] == reports['initial']['tools']
            assert row['bundle_original_verified_source'] == reports['initial']['bundle_original_verified_source']
        else:
            assert hashes['openpilot-runtime-build'] != initial_hashes['openpilot-runtime-build']
            assert all(a['fresh'] and a['sha256'] == initial_hashes[Path(a['executable']).name]
                for a in artifacts if Path(a['executable']).name != 'openpilot-runtime-build')
            help_output, _ = execute([Path(row['tools'])/'openpilot-runtime-build', '--help'], 'small-actual-help.txt')
            assert 'Owned bootstrap reuse marker.' in help_output.read_text()
        attempt = work/Path(row['resource_evidence']).relative_to(guest_work)
        samples = [json.loads(line) for line in (attempt/'resources.jsonl').read_text().splitlines()]
        assert samples[-1]['state'] == 'success'
        assert samples[-1]['memory']['kernel_peak_bytes'] > 0
        metrics[phase] = dict(elapsed_ms=samples[-1]['elapsed_ms'],
            kernel_worker_peak_bytes=samples[-1]['memory']['kernel_peak_bytes'],
            frontend_kernel_peak_rss_bytes=samples[-1]['frontend_kernel_peak_rss_bytes'],
            observed_filesystem_growth_bytes=[d['observed_peak_growth_bytes'] for d in samples[-1]['filesystems']],
            binary_stored_bytes=row['build']['stored_output_bytes'])
        assert active.read_bytes() == b'owned previous selection sentinel\n'
        assert log.read_bytes() == b'owned previous log sentinel\n'
        reports[phase] = row
    (evidence/'summary.json').write_text(json.dumps(dict(schema=1,
        actual_current_tools=13, cases=list(reports), source_commit=context['source_commit'],
        seed_source=manifest['source'], expected_seed_descriptor_sha256=seed['descriptor_sha256'],
        repeated_and_docs_all_fresh=True, repeated_and_docs_binary_storage_bytes=0,
        small_changed_tool='openpilot-runtime-build', unmodified_tools_fresh=12,
        original_bundle_provenance_retained=True, public_seed_download_tested=False,
        request_owner_pid_and_start_time_bound=True,
        actual_host_kernel_cgroup=True, process_architecture='aarch64', physical_arm_cpu=False,
        agnos_kernel=False, runtime_activated=False, runtime_started=False,
        resource_policy_is_ci_fixture_not_device_budget=True, metrics=metrics), indent=2)+'\n')


if __name__ == '__main__':
    main()
