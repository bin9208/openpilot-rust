#!/usr/bin/env python3
"""Hosted RED evidence against the immutable pre-fix FunctionFS source."""
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
baseline = '6133c589c71d765e9a27a5961fc26c8dce099b67'
expected = 'c8e32605ed535bed7523bf55ecbb840c7ac680e981364a7a15051d6b7e4ad930'
url = f'https://raw.githubusercontent.com/bin9208/openpilot-rust/{baseline}/rust/crates/jetlink/src/ffs.rs'
for attempt in range(3):
    try:
        with urllib.request.urlopen(url, timeout=30) as response:
            old = response.read(128 * 1024)
        break
    except OSError:
        if attempt == 2:
            raise
        time.sleep(2)
assert hashlib.sha256(old).hexdigest() == expected
current = source.read_bytes()
assert current != old, 'RED source must differ from the proposed fix'
subprocess.run(['git', 'diff', '--exit-code', '--', str(source)], cwd=root, check=True)
output = Path(os.environ['RUNNER_TEMP']) / 'jetlink-teardown-red'
output.mkdir()
cases = ['onroad_setup_teardown_and_reconnect_release_the_controller',
         'already_unbound_controller_is_not_written_again']
rows = []
try:
    source.write_bytes(old)
    for name in cases:
        command = ['cargo', '+1.94.0', 'test', '--manifest-path', 'rust/Cargo.toml', '--locked',
                   '-p', 'openpilot-jetlink', '--test', 'ffs', name, '--', '--exact', '--test-threads=1']
        result = subprocess.run(command, cwd=root, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                text=True, env=dict(os.environ, CARGO_TERM_COLOR='never'))
        (output / f'{name}.log').write_text(result.stdout)
        assert result.returncode == 101 and f'test {name} ... FAILED' in result.stdout, result.stdout[-12000:]
        rows.append(dict(test=name, exit_code=result.returncode, expected_failure=True))
finally:
    source.write_bytes(current)
    assert source.read_bytes() == current
    subprocess.run(['git', 'diff', '--exit-code', '--', str(source)], cwd=root, check=True)
receipt = dict(baseline_commit=baseline, baseline_source_sha256=expected,
               candidate_commit=subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip(),
               candidate_source_sha256=hashlib.sha256(current).hexdigest(),
               candidate_source_restored=True, device_execution=False, cases=rows)
(output / 'summary.json').write_text(json.dumps(receipt, indent=2) + '\n')
print(json.dumps(receipt))
