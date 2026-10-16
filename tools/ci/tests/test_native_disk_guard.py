"""Native CI guard bootstrap must reserve capacity before any toolchain/build work."""
import os
import hashlib
import re
from pathlib import Path
import shutil
import subprocess

import yaml

ROOT = Path(__file__).resolve().parents[3]
BOOTSTRAP = ROOT / 'rust/tools/check-disk-space'


def fixture(tmp_path, available='999999999999'):
    commands = tmp_path / 'commands'
    commands.mkdir()
    log = tmp_path / 'commands.log'
    for name, body in {
        'df': f"case \"$*\" in *'operation space'*) printf 'Avail\\n%s\\n' \"${{GUARD_OPERATION_FREE:-{available}}}\";;"
              + f" *) printf 'Avail\\n{available}\\n';; esac\n",
        'rustup': 'printf "rustup %s\\n" "$*" >> "$GUARD_COMMAND_LOG"\n',
        'cargo': '''test -z "${RUSTFLAGS+x}${CARGO_BUILD_TARGET+x}"
printf 'cargo %s\\n' "$*" >> "$GUARD_COMMAND_LOG"
test -z "${GUARD_BUILD_FAIL:-}" || exit 17
sleep 0.1
target=''
host=''
while [ "$#" -gt 0 ]; do
  case "$1" in
    --target-dir) target=$2; shift ;;
    --target) host=$2; shift ;;
  esac
  shift
done
mkdir -p "$target/$host/debug"
printf '#!/bin/bash\\nprintf "native %%s\\\\n" "$*" >> "$GUARD_COMMAND_LOG"\\n' > "$target/$host/debug/disk-space-guard"
chmod +x "$target/$host/debug/disk-space-guard"
''',
    }.items():
        path = commands / name
        path.write_text('#!/bin/bash\nset -eu\n' + body)
        path.chmod(0o755)
    environment = dict(os.environ, PATH=str(commands) + os.pathsep + os.environ['PATH'],
                       RUNNER_TEMP=str(tmp_path / 'runner'), GUARD_COMMAND_LOG=str(log),
                       CARGO_TARGET_DIR=str(tmp_path / 'unrelated-target'),
                       RUSTFLAGS='-Zsanitizer=address', CARGO_BUILD_TARGET='aarch64-unknown-linux-gnu')
    Path(environment['RUNNER_TEMP']).mkdir(mode=0o700)
    return environment, log


def invoke(environment):
    return subprocess.run(['bash', environment.get('GUARD_BOOTSTRAP', str(BOOTSTRAP)),
                           '--path', environment.get('GUARD_OPERATION_PATH', '/path with spaces/new'), '--growth-mib', '128'],
                          env=environment, text=True, capture_output=True)


def test_capacity_failure_precedes_rustup_and_cargo(tmp_path):
    environment, log = fixture(tmp_path, str(25 * 1024**3 + 1024**3 - 1))
    result = invoke(environment)
    assert result.returncode != 0
    assert 'recover at least 35 GiB' in result.stderr
    assert not log.exists(), 'toolchain/build ran before capacity admission'


def test_cold_bootstrap_builds_only_host_guard_and_survives_target_cleanup(tmp_path):
    environment, log = fixture(tmp_path)
    result = invoke(environment)
    assert result.returncode == 0, result.stdout + result.stderr
    first = log.read_text().splitlines()
    assert first[0].startswith('rustup toolchain install 1.94.0')
    assert 'cargo +1.94.0 build' in first[1]
    assert '-p openpilot-disk-space' in first[1]
    assert '--locked' in first[1]
    assert first[2] == 'native --path /path with spaces/new --growth-mib 128'
    shutil.rmtree(environment['CARGO_TARGET_DIR'], ignore_errors=True)
    environment['CARGO_TARGET_DIR'] += '-changed'
    result = invoke(environment)
    assert result.returncode == 0, result.stdout + result.stderr
    assert log.read_text().splitlines() == first + [first[2]]


def test_workflow_guards_use_absolute_native_bootstrap_and_keep_all_growths():
    workflow = ROOT / '.github/workflows/rust.yml'
    text = workflow.read_text()
    assert 'card_ci_space.py' not in text
    assert text.count('"$GITHUB_WORKSPACE/rust/tools/check-disk-space"') == 187
    jobs = yaml.safe_load(text)['jobs']
    guarded = [job for job in jobs.values() if any('check-disk-space' in step.get('run', '') for step in job.get('steps', []))]
    assert len(guarded) == 26
    def has_baseline_guard(script):
        lines = script.splitlines()
        return any('check-disk-space' in line and
                   (index + 1 == len(lines) or '--bin stage-startup-ui-raylib' not in lines[index + 1])
                   for index, line in enumerate(lines))
    assert sum(any(has_baseline_guard(step.get('run', '')) for step in job.get('steps', []))
               for job in jobs.values()) == 24
    # Preserve the original179 migration baseline independently of the five
    # #326 guards added specifically before compiling its native stager.
    lines = text.splitlines()
    arguments, raylib_guards, replay_guards, encoder_guards = [], [], [], []
    for index, line in enumerate(lines):
        match = re.search(r'"\$GITHUB_WORKSPACE/rust/tools/check-disk-space"([^\n]*)', line)
        if match:
            if index + 1 < len(lines) and '--bin stage-startup-ui-raylib' in lines[index + 1]:
                raylib_guards.append(match.group(1))
            elif index + 1 < len(lines) and '--bin openpilot-radar-replay -- bundle' in lines[index + 1]:
                replay_guards.append(match.group(1))
            elif index + 1 < len(lines) and '-p openpilot-encoderd --lib --features' in lines[index + 1]:
                encoder_guards.append(match.group(1))
            else:
                arguments.append(match.group(1))
    assert replay_guards == [' --path "$GITHUB_WORKSPACE" --growth-mib 1024']
    assert len(arguments) == 179
    assert raylib_guards == [' --path "$GITHUB_WORKSPACE" --growth-mib 2048'] * 5
    assert encoder_guards == [' --path "$GITHUB_WORKSPACE" --growth-mib 1024'] * 2
    native_calls = [index for index, line in enumerate(lines) if '--bin stage-startup-ui-raylib' in line]
    assert len(native_calls) == 5
    native_arguments = re.findall(r'--bin stage-startup-ui-raylib -- ([^\n]*)', text)
    assert hashlib.sha256('\n'.join(native_arguments).encode()).hexdigest() == '2efc90425a3b4e1cecaabbc410c6d13649b89ad4fbc3a9d722b33b510694b396'
    for index in native_calls:
        assert 'cargo +1.94.0 run' in lines[index]
        assert '"$GITHUB_WORKSPACE/rust/tools/check-disk-space"' in lines[index - 1]
    assert 'stage_startup_ui_raylib.py' not in text
    assert hashlib.sha256('\n'.join(arguments).encode()).hexdigest() == '952f3b9fc8820e701aae55f88368236acddd3da2af55465448aa00eddaf9fe55'
    template = (ROOT / 'rust/crates/carrot-man/ci-jobs.yml').read_text()
    assert 'card_ci_space.py' not in template
    assert template.count('"$GITHUB_WORKSPACE/rust/tools/check-disk-space"') == 9
    arguments = re.findall(r'"\$GITHUB_WORKSPACE/rust/tools/check-disk-space"([^\n]*)', template)
    assert hashlib.sha256('\n'.join(arguments).encode()).hexdigest() == '471b43e7b994adca394eb5a342a5cdbaca211fec1b306dc219cc9c0f767a03b7'


def test_failed_build_never_publishes_guard(tmp_path):
    environment, log = fixture(tmp_path)
    environment['GUARD_BUILD_FAIL'] = 'yes'
    result = invoke(environment)
    assert result.returncode != 0
    assert not any(path.name == 'disk-space-guard' for path in Path(environment['RUNNER_TEMP']).rglob('*'))
    del environment['GUARD_BUILD_FAIL']
    result = invoke(environment)
    assert result.returncode == 0, result.stdout + result.stderr
    assert sum(line.startswith('cargo ') for line in log.read_text().splitlines()) == 2


def test_parallel_cold_calls_publish_once(tmp_path):
    from concurrent.futures import ThreadPoolExecutor
    environment, log = fixture(tmp_path)
    with ThreadPoolExecutor(max_workers=2) as pool:
        results = list(pool.map(invoke, [environment, environment]))
    assert all(result.returncode == 0 for result in results), results
    lines = log.read_text().splitlines()
    assert sum(line.startswith('cargo ') for line in lines) == 1
    assert sum(line.startswith('native ') for line in lines) == 2


def test_requested_operation_filesystem_is_checked_before_toolchain_work(tmp_path):
    environment, log = fixture(tmp_path)
    operation = tmp_path / 'operation space'
    operation.mkdir()
    environment['GUARD_OPERATION_PATH'] = str(operation / 'new output')
    environment['GUARD_OPERATION_FREE'] = str(25 * 1024**3 + 128 * 1024**2 - 1)
    result = invoke(environment)
    assert result.returncode != 0
    assert 'recover at least 35 GiB' in result.stderr
    assert not log.exists()


def test_arm_runner_bootstraps_native_host_even_with_cross_environment(tmp_path):
    environment, log = fixture(tmp_path)
    uname = tmp_path / 'commands/uname'
    uname.write_text('#!/bin/bash\nprintf "aarch64\\n"\n')
    uname.chmod(0o755)
    environment['CARGO_BUILD_TARGET'] = 'x86_64-unknown-linux-gnu'
    result = invoke(environment)
    assert result.returncode == 0, result.stdout + result.stderr
    assert '--target aarch64-unknown-linux-gnu' in log.read_text()


def test_bad_df_output_fails_closed_before_install(tmp_path):
    environment, log = fixture(tmp_path, 'not-a-number')
    result = invoke(environment)
    assert result.returncode != 0
    assert not log.exists()


def test_invalid_cli_never_installs_or_compiles(tmp_path):
    environment, log = fixture(tmp_path)
    for arguments in [
        ['--path', '.', '--path', '.', '--growth-mib', '0'],
        ['--path', '.', '--growth-mib', '0', '--growth-mib', '0'],
        ['--growth-mib', '0'],
        ['--path', '.', '--growth-mib', '0', '--unknown'],
    ]:
        result = subprocess.run(['bash', str(BOOTSTRAP), *arguments], env=environment, text=True, capture_output=True)
        assert result.returncode != 0, arguments
        assert result.stdout == ''
        assert not log.exists(), arguments


def test_changed_source_invalidates_cached_executable(tmp_path):
    environment, log = fixture(tmp_path)
    root = tmp_path / 'repository'
    (root / 'rust/tools').mkdir(parents=True)
    shutil.copy(BOOTSTRAP, root / 'rust/tools/check-disk-space')
    for filename in ('Cargo.toml', 'Cargo.lock'):
        shutil.copy(ROOT / 'rust' / filename, root / 'rust' / filename)
    shutil.copytree(ROOT / 'rust/crates/disk-space', root / 'rust/crates/disk-space')
    environment['GUARD_BOOTSTRAP'] = str(root / 'rust/tools/check-disk-space')
    assert invoke(environment).returncode == 0
    source = root / 'rust/crates/disk-space/src/lib.rs'
    source.write_text(source.read_text() + '\n// changed source identity\n')
    assert invoke(environment).returncode == 0
    for manifest in ('Cargo.lock', 'Cargo.toml'):
        path = root / 'rust' / manifest
        path.write_text(path.read_text() + '\n')
        assert invoke(environment).returncode == 0
    lines = log.read_text().splitlines()
    assert sum(line.startswith('cargo ') for line in lines) == 4


def test_interrupted_publication_exposes_no_partial_executable(tmp_path):
    environment, log = fixture(tmp_path)
    copier = tmp_path / 'commands/cp'
    copier.write_text('''#!/bin/bash
for argument do destination=$argument; done
printf 'partial executable' > "$destination"
exit 17
''')
    copier.chmod(0o755)
    result = invoke(environment)
    assert result.returncode != 0
    cache = Path(environment['RUNNER_TEMP']) / 'openpilot-disk-guard'
    assert not list(cache.glob('*/disk-space-guard'))
    assert not list(cache.glob('*/.publish.*'))
    assert not any(line.startswith('native ') for line in log.read_text().splitlines())
    copier.unlink()
    assert invoke(environment).returncode == 0


def test_cache_rejects_unsafe_entries_without_executing_or_building(tmp_path):
    for kind in ('base-mode', 'base-symlink', 'lock-fifo', 'lock-symlink', 'binary-mode', 'binary-symlink', 'key-mode', 'lock-mode', 'base-owner'):
        case = tmp_path / kind
        case.mkdir()
        environment, log = fixture(case)
        base = Path(environment['RUNNER_TEMP']) / 'openpilot-disk-guard'
        if kind.startswith('binary') or kind == 'key-mode':
            assert invoke(environment).returncode == 0
            binary = next(base.glob('*/disk-space-guard'))
            if kind == 'key-mode':
                binary.parent.chmod(0o777)
            elif kind == 'binary-mode':
                binary.chmod(0o777)
            else:
                target = case / 'sentinel'
                binary.rename(target)
                binary.symlink_to(target)
            log.unlink()
        elif kind == 'base-symlink':
            target = case / 'sentinel'
            target.mkdir()
            base.symlink_to(target)
        else:
            base.mkdir(mode=0o700)
            if kind == 'base-mode':
                base.chmod(0o777)
            elif kind == 'lock-mode':
                lock = base / 'bootstrap.lock'
                lock.write_text('untouched')
                lock.chmod(0o666)
            elif kind == 'base-owner':
                stat = case / 'commands/stat'
                stat.write_text('#!/bin/bash\nprintf \"%s 700\\n\" \"$(( $(id -u) + 1 ))\"\n')
                stat.chmod(0o755)
            elif kind == 'lock-fifo':
                os.mkfifo(base / 'bootstrap.lock')
            else:
                target = case / 'sentinel'
                target.write_text('untouched')
                (base / 'bootstrap.lock').symlink_to(target)
        result = subprocess.run(['bash', str(BOOTSTRAP), '--path', '.', '--growth-mib', '0'],
                                env=environment, text=True, capture_output=True, timeout=3)
        assert result.returncode != 0, kind
        assert not log.exists(), kind
        if kind == 'lock-symlink':
            assert target.read_text() == 'untouched'


def test_local_cache_uses_private_user_cache_and_secure_modes(tmp_path):
    environment, log = fixture(tmp_path)
    environment.pop('RUNNER_TEMP')
    environment['XDG_CACHE_HOME'] = str(tmp_path / 'user-cache')
    result = invoke(environment)
    assert result.returncode == 0, result.stderr
    base = tmp_path / 'user-cache/openpilot-disk-guard'
    binary = next(base.glob('*/disk-space-guard'))
    assert base.stat().st_mode & 0o777 == 0o700
    assert binary.parent.stat().st_mode & 0o777 == 0o700
    assert binary.stat().st_mode & 0o777 == 0o700
    assert invoke(environment).returncode == 0
    assert sum(line.startswith('cargo ') for line in log.read_text().splitlines()) == 1
