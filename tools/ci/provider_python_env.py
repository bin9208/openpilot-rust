"""Small stdlib-only #506 probe for the existing provider/source Actions jobs."""
import argparse
import hashlib
import importlib
import json
import os
from pathlib import Path
import platform
import subprocess
import sys


BASELINE_COMMIT = '2765b4ab1c15a05c72d23c4ca32c033011c14543'
BASELINE_BLOB = '55170bb2f75746d668d7e3c2ded1f4b197f4a222'
# Exact stripped lines from that Git blob; the fast regression authenticates the
# entire blob and both lines. Keep the old environment confined to a child.
LEGACY = {
    'webcam': (
        r'''printf 'OPENPILOT_WEBCAM_OPENCV_ROOT=%s/webcam/provider\nLD_LIBRARY_PATH=%s/webcam/provider/install/lib\nPYTHONPATH=%s/webcam/binding:%s\n' "$RUNNER_TEMP" "$RUNNER_TEMP" "$RUNNER_TEMP" "$PYTHONPATH" >> "$GITHUB_ENV"''',
        'af82621374538fa9bd551b5b8d3a9bc98d47717964f465ff55eef068bef00bd8'),
    'xiaoge': (
        r'''printf 'OPENPILOT_OPENCV_ROOT=%s/xiaoge-opencv\nLD_LIBRARY_PATH=%s/xiaoge-opencv/install/lib\nPYTHONPATH=%s/xiaoge-original/ipc:%s\nOPENCV_TEST_LANE_MODEL=%s/openpilot/selfdrive/carrot/xiaoge/assets/lane.onnx\n' "$RUNNER_TEMP" "$RUNNER_TEMP" "$RUNNER_TEMP" "$PYTHONPATH" "$GITHUB_WORKSPACE" >> "$GITHUB_ENV"''',
        'ef4da3c6390f36795f9ef8edd7500c0e407cacef68abf540c8771266ca9d0b13'),
}
MODULES = {
    'webcam': ('av', 'cv2', 'numpy', 'capnp'),
    'xiaoge': ('cv2', 'numpy', 'capnp', 'httpx2', 'zmq'),
}


def provider_library_path(provider):
    tail = os.environ.get('LD_LIBRARY_PATH')
    return provider + (':' + tail if tail else '')


def legacy_fragment(job):
    command, digest = LEGACY[job]
    if hashlib.sha256(command.encode()).hexdigest() != digest:
        raise ValueError('Changed immutable #506 baseline fragment')
    return command


def snapshot(modules):
    result = {
        'identity': {'executable': sys.executable, 'version': sys.version,
                     'prefix': sys.prefix, 'base_prefix': sys.base_prefix},
        'machine': platform.machine(), 'version_info': list(sys.version_info),
        'executable_realpath': str(Path(sys.executable).resolve()),
        'sys_path': sys.path, 'pythonLocation': os.environ.get('pythonLocation'),
        'LD_LIBRARY_PATH': os.environ.get('LD_LIBRARY_PATH'),
        'PYTHONPATH': os.environ.get('PYTHONPATH'),
        'source_commit': os.environ.get('GITHUB_SHA'),
        'modules': {}, 'import_errors': {},
    }
    for name in modules:
        try:
            module = importlib.import_module(name)
            result['modules'][name] = {'file': module.__file__, 'version': getattr(module, '__version__', None)}
        except Exception as error:
            result['import_errors'][name] = f'{type(error).__name__}: {error}'
    mappings = [line for line in Path('/proc/self/maps').read_text().splitlines() if 'libpython' in line]
    result['libpython_maps'] = mappings
    # Addresses change across processes. The loaded files must remain identical.
    result['identity']['libpython'] = sorted({line.split(maxsplit=5)[5] for line in mappings})
    return result


def identity_failures(before, after):
    failures = [field for field, value in before['identity'].items() if after['identity'].get(field) != value]
    if after['import_errors']:
        failures.append('imports')
    if before['modules'] != after['modules']:
        failures.append('modules')
    return failures


def write_receipt(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2) + '\n')


def before_provider(job, provider, output):
    before = snapshot(MODULES[job])
    write_receipt(output / 'before/result.json', before)
    if before['import_errors']:
        raise RuntimeError(f'Already-pinned closure failed before provider ENV: {before["import_errors"]}')
    command = legacy_fragment(job)
    legacy = output / 'legacy'
    legacy.mkdir()
    env_file = legacy / 'github-env.txt'
    subprocess.run(['bash', '--noprofile', '--norc', '-euc', command], check=True,
                   env=dict(os.environ, GITHUB_ENV=str(env_file)))
    exported = dict(line.split('=', 1) for line in env_file.read_text().splitlines())
    if exported['LD_LIBRARY_PATH'] != provider:
        raise RuntimeError('Baseline expansion does not match the actual provider')
    arguments = [sys.executable, '-P', str(Path(__file__).resolve()), 'probe', '--job', job,
                 '--output', str(legacy)]
    write_receipt(legacy / 'invocation.json', {'argv': arguments, 'exported': exported})
    with (legacy / 'stdout.log').open('w') as stdout, (legacy / 'stderr.log').open('w') as stderr:
        try:
            process = subprocess.run(arguments, env=dict(os.environ, **exported), stdout=stdout, stderr=stderr, timeout=120)
            returncode = process.returncode
        except subprocess.TimeoutExpired:
            returncode = 'timeout'
    path = legacy / 'result.json'
    actual = json.loads(path.read_text()) if path.exists() else {}
    failures = identity_failures(before, actual) if 'identity' in actual else ['probe-did-not-complete']
    actual.update({'baseline_commit': BASELINE_COMMIT, 'baseline_blob': BASELINE_BLOB,
                   'fragment': command, 'fragment_sha256': LEGACY[job][1],
                   'returncode': returncode, 'differences_from_before': failures,
                   'exported_LD_LIBRARY_PATH': exported['LD_LIBRARY_PATH'],
                   'erased_nonempty_tail': bool(before['LD_LIBRARY_PATH']) and
                       exported['LD_LIBRARY_PATH'] != provider + ':' + before['LD_LIBRARY_PATH'],
                   'counterexample': 'observed-failure-or-identity-change' if failures or returncode != 0 else 'not-reproduced',
                   'limit': 'This real interpreter may retain its prefix through RPATH; no old failure is forced.'})
    write_receipt(path, actual)
    print(json.dumps({'legacy': actual['counterexample'], 'differences': failures, 'machine': before['machine']}))


def after_provider(job, provider, output):
    before = json.loads((output / 'before/result.json').read_text())
    after = snapshot(MODULES[job])
    failures = identity_failures(before, after)
    expected = provider + (':' + before['LD_LIBRARY_PATH'] if before['LD_LIBRARY_PATH'] else '')
    if after['LD_LIBRARY_PATH'] != expected:
        failures.append('LD_LIBRARY_PATH')
    after.update({'failures': failures, 'expected_LD_LIBRARY_PATH': expected,
                  'status': 'FAIL' if failures else 'PASS'})
    write_receipt(output / 'candidate/result.json', after)
    if failures:
        raise RuntimeError(f'Provider ENV changed Python identity/imports: {failures}')
    print(json.dumps({'candidate': 'PASS', 'modules': list(after['modules']), 'machine': after['machine']}))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('phase', choices=('before', 'after', 'probe'))
    parser.add_argument('--job', choices=tuple(MODULES), required=True)
    parser.add_argument('--provider')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    if args.phase == 'probe':
        result = snapshot(MODULES[args.job])
        write_receipt(args.output / 'result.json', result)
        return int(bool(result['import_errors']))
    if not args.provider:
        parser.error('--provider is required before/after the provider ENV')
    if args.phase == 'before':
        before_provider(args.job, args.provider, args.output)
    else:
        after_provider(args.job, args.provider, args.output)
    return 0


if __name__ == '__main__':
    sys.exit(main())
