#!/usr/bin/env python3
"""Actions-only real APT execution; preserve shared sources and dpkg arches."""
import hashlib
import json
import os
from pathlib import Path
import platform
import runpy
import shlex
import shutil
import subprocess


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def snapshot():
    files = [Path('/etc/apt/sources.list')]
    files += sorted(Path('/etc/apt/sources.list.d').glob('*'))
    return dict(
        native=subprocess.check_output(['dpkg', '--print-architecture'], text=True).strip(),
        foreign=subprocess.check_output(['dpkg', '--print-foreign-architectures'], text=True).split(),
        sources={str(path): dict(sha256=digest(path), mode=path.stat().st_mode,
                                uid=path.stat().st_uid, gid=path.stat().st_gid)
                 for path in files if path.is_file()})


def main():
    runpy.run_path(str(Path(__file__).with_name('runner_policy.py')))['require_environment'](self_hosted_only=True)
    assert os.environ['LOCAL_CI_ARCH'] == 'x86_64' and platform.machine() == 'x86_64'
    job = Path(os.environ['CI_JOB_TEMP'])
    assert job.parent == Path('/tmp') and job.name.startswith('openpilot-ci.') and job.resolve() == job
    assert (job/'owner').read_text() == ':'.join(os.environ[k] for k in ('GITHUB_RUN_ID', 'GITHUB_RUN_ATTEMPT', 'GITHUB_JOB'))
    source = Path(os.environ['GITHUB_WORKSPACE'])
    sha = subprocess.check_output(['git', '-C', source, 'rev-parse', 'HEAD'], text=True).strip()
    assert sha == os.environ['GITHUB_SHA']
    expected_growth = 2 * 1024**3
    assert shutil.disk_usage(job).free >= expected_growth, 'insufficient capacity for private cross APT indices'
    before = snapshot()
    assert before['native'] == 'amd64'
    work = job/'native-apt'
    work.mkdir()
    evidence = dict(schema=1, source_commit=sha, runner=os.environ['RUNNER_NAME'], runner_environment=os.environ['RUNNER_ENVIRONMENT'],
                    host_architecture=platform.machine(),
                    before=before, expected_private_index_growth_bytes=expected_growth,
                    scope='X64 APT transport and original explicit cross repository metadata; no ARM program or device execution')

    def run(script, name, *, environment=None):
        with (work/f'{name}.log').open('wb') as output:
            subprocess.run(['bash', '--noprofile', '--norc', '-e', '-o', 'pipefail', '-c', script],
                           stdout=output, stderr=subprocess.STDOUT, check=True, env=environment)

    try:
        # This is the original failing command, without source-step changes.
        run('sudo apt-get update', 'original-native-update')
        cross = work/'explicit-cross'
        (cross/'lists/partial').mkdir(parents=True)
        (cross/'archives/partial').mkdir(parents=True)
        config = cross/'private.conf'
        # A process-only list tests both arches even on an otherwise clean host.
        # It does not register a foreign architecture with dpkg.
        config.write_text('#clear APT::Architectures;\nAPT::Architecture "amd64";\n'
                          'APT::Architectures { "amd64"; "arm64"; };\n'
                          f'Dir::State::lists "{cross}/lists";\n'
                          f'Dir::Cache::Archives "{cross}/archives";\n')
        repositories = source/'.github/apt/startup-ui-cross.sources'
        evidence['original_cross_sources_sha256'] = digest(repositories)
        argv = ['sudo', 'apt-get', '-o', f'Dir::Etc::sourcelist={repositories}',
                '-o', 'Dir::Etc::sourceparts=-', '-c', str(config), 'update']
        run(shlex.join(argv), 'original-explicit-cross-sources')
        indices = [path for path in (cross/'lists').iterdir() if path.is_file()]
        for arch in ('amd64', 'arm64'):
            assert any(f'binary-{arch}_Packages' in path.name and path.stat().st_size > 0 for path in indices), arch
        evidence['private_index_bytes'] = sum(path.stat().st_size for path in indices)
        run('test "$(sudo -n /usr/bin/id -u)" = 0', 'unrelated-sudo-command')
        # A shell guard check, not an ARM execution claim. Guest sudo must keep
        # its original meaning when local-arm loads the same job BASH_ENV.
        run('if declare -F sudo; then exit 1; fi', 'arm-shell-no-native-wrapper',
            environment=dict(os.environ, LOCAL_CI_ARCH='aarch64'))
        rows = [json.loads(line) for line in (job/'native-apt-evidence.jsonl').read_text().splitlines()]
        assert [row['mode'] for row in rows] == ['native-default', 'explicit-configuration']
        assert all(row['source_commit'] == sha and row['native_architecture'] == 'amd64' for row in rows)
        evidence.update(success=True, invocations=rows, explicit_arm64_indices_present=True)
    except BaseException as error:
        evidence.update(success=False, failure=str(error))
        raise
    finally:
        after = snapshot()
        evidence.update(after=after, shared_configuration_preserved=before == after)
        (work/'result.json').write_text(json.dumps(evidence, indent=2)+'\n')
        assert before == after, 'shared dpkg architecture or APT sources changed'


if __name__ == '__main__':
    main()
