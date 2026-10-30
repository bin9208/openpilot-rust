#!/usr/bin/env python3
"""Hosted RED evidence against immutable pre-fix Jetlink cleanup sources."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time
import urllib.request

assert os.environ.get('GITHUB_ACTIONS') == 'true'
assert os.environ.get('RUNNER_ENVIRONMENT') == 'github-hosted'
assert os.environ.get('GITHUB_REPOSITORY') == 'bin9208/openpilot-rust'
root = Path(__file__).resolve().parents[2]
source = root / 'rust/crates/jetlink/src/ffs.rs'
adb = root / 'rust/crates/jetlink/src/gadget/adb.rs'
baseline = '6133c589c71d765e9a27a5961fc26c8dce099b67'
expected = 'c8e32605ed535bed7523bf55ecbb840c7ac680e981364a7a15051d6b7e4ad930'
adb_baseline = '5ec52243473fbba9a4833b371fe891b0fde37d87'
adb_expected = 'd1718ba555e62c8208191aef65dc58fa6143212829d95086ad60165155ff99f2'


def original(path, commit, sha256):
    relative = path.relative_to(root).as_posix()
    url = f'https://raw.githubusercontent.com/bin9208/openpilot-rust/{commit}/{relative}'
    for attempt in range(3):
        try:
            with urllib.request.urlopen(url, timeout=30) as response:
                data = response.read(128 * 1024)
            assert hashlib.sha256(data).hexdigest() == sha256, relative
            return data
        except OSError:
            if attempt == 2:
                raise
            time.sleep(2)


def restore_block(data):
    start = data.index(b'\npub(super) fn restore(')
    end = data.index(b'\n#[cfg(test)]', start)
    return start, end


old = original(source, baseline, expected)
old_adb = original(adb, adb_baseline, adb_expected)
current = source.read_bytes()
current_adb = adb.read_bytes()
assert current != old
old_start, old_end = restore_block(old_adb)
start, end = restore_block(current_adb)
assert current_adb[start:end] != old_adb[old_start:old_end]
paths = [str(source), str(adb)]
subprocess.run(['git', 'diff', '--exit-code', '--', *paths], cwd=root, check=True)
output = Path(os.environ['RUNNER_TEMP']) / 'jetlink-teardown-red'
output.mkdir()
rows = []


def fails(target, name):
    command = ['cargo', '+1.94.0', 'test', '--manifest-path', 'rust/Cargo.toml', '--locked',
               '-p', 'openpilot-jetlink', *target, name, '--', '--exact', '--test-threads=1']
    result = subprocess.run(command, cwd=root, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                            text=True, env=dict(os.environ, CARGO_TERM_COLOR='never'))
    (output / f'{name.replace(":", "_")}.log').write_text(result.stdout)
    assert result.returncode == 101 and f'test {name} ... FAILED' in result.stdout, result.stdout[-12000:]
    rows.append(dict(test=name, exit_code=result.returncode, expected_failure=True))


try:
    source.write_bytes(old)
    for name in ['onroad_setup_teardown_and_reconnect_release_the_controller',
                 'already_unbound_controller_is_not_written_again']:
        fails(['--test', 'ffs'], name)
    source.write_bytes(current)
    # Preserve the newly added private unit fixtures while executing the exact
    # original production restore function, not a rewritten imitation of it.
    adb.write_bytes(current_adb[:start] + old_adb[old_start:old_end] + current_adb[end:])
    fails(['--lib'], 'gadget::adb::tests::off_setting_accepts_stock_unbind_error_only_after_verification')
finally:
    source.write_bytes(current)
    adb.write_bytes(current_adb)
    assert source.read_bytes() == current and adb.read_bytes() == current_adb
    subprocess.run(['git', 'diff', '--exit-code', '--', *paths], cwd=root, check=True)
receipt = dict(baseline_commit=baseline, baseline_source_sha256=expected,
               adb_baseline_commit=adb_baseline, adb_baseline_source_sha256=adb_expected,
               adb_restore_function_sha256=hashlib.sha256(old_adb[old_start:old_end]).hexdigest(),
               candidate_commit=subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip(),
               candidate_source_sha256=hashlib.sha256(current).hexdigest(),
               candidate_adb_source_sha256=hashlib.sha256(current_adb).hexdigest(),
               candidate_source_restored=True, device_execution=False, cases=rows)
(output / 'summary.json').write_text(json.dumps(receipt, indent=2) + '\n')
print(json.dumps(receipt))
