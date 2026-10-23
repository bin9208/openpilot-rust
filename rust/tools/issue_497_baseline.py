"""Run narrow behavioral RED controls in a disposable registered Actions checkout."""
import argparse
import contextlib
import hashlib
import json
import os
from pathlib import Path
import subprocess

BASE = 'b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a'


def sdk_baseline_with_current_space_policy(source):
  # Keep the original directory walker; only apply the owner's current capacity policy.
  old = b'if shutil.disk_usage(args.output).free < 27 * 1024**3:'
  message = b'SDK extraction needs 25 GiB reserve plus 2 GiB estimated growth'
  if source.count(old) != 1 or source.count(message) != 1:
    raise AssertionError('SDK baseline capacity boundary changed')
  return source.replace(old, b'if shutil.disk_usage(args.output).free < 2 * 1024**3:').replace(
    message, b'SDK extraction needs 2 GiB estimated growth')


def sdk_alias_failure(output):
  return all(marker in output for marker in (
    'FAILED (failures=1)', 'missing image file /usr/include/real:', 'Type: directory',
  )) and not any(marker in output for marker in (
    'SDK extraction needs', 'ModuleNotFoundError', 'ImportError', 'SyntaxError',
    'No space left on device',
  ))

@contextlib.contextmanager
def original(paths, hooks=None, ref=BASE):
  saved = {path: Path(path).read_bytes() for path in paths}
  try:
    for path in paths:
      source = subprocess.check_output(['git', 'show', f'{ref}:{path}'])
      Path(path).write_bytes(source + (hooks or {}).get(path, '').encode())
    yield
  finally:
    for path, data in saved.items():
      Path(path).write_bytes(data)


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('scope', choices=['core', 'web', 'media', 'tools', 'vehicles', 'athena', 'card-attribution'])
  parser.add_argument('--output', type=Path, required=True)
  options = parser.parse_args()
  options.output.mkdir(parents=True, exist_ok=False)
  results = []

  def red(name, command, marker='test result: FAILED'):
    result = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, timeout=900)
    (options.output / f'{name}.log').write_text(result.stdout)
    invalid = ('could not compile', 'fixture PTY readiness deadline', 'shell exited during setup')
    if (result.returncode == 0 or marker not in result.stdout
        or any(failure in result.stdout for failure in invalid)
        or (name == 'issue_292_directory_aliases' and not sdk_alias_failure(result.stdout))
        or (name == 'issue_185_kia_key' and 'MissingTorque' not in result.stdout)
        or (name == 'issue_214_upload_attribution' and ('exactly one receipt per failed upload call' not in result.stdout or 'left: 0' not in result.stdout))
        or (name == 'issue_501_public_registry_admits_exact_psa_dashcam_parameters' and 'MissingTorque' not in result.stdout)
        or (name == 'issue_501_public_registry_constructs_first_update_and_apply_from_known_angle_params'
            and 'behavioral RED: existing PSA RuntimeUnported' not in result.stdout)
        or (name == 'issue_270_cleanup' and (
            'owned jobs remain:' not in result.stdout
            or result.stdout.count('"event":"issue270_fixture_setup_complete"') != 6))):
      raise AssertionError(f'{name}: expected a behavioral baseline assertion failure; exit={result.returncode}; inspect retained log')
    results.append({'case': name, 'exit': result.returncode, 'expected_behavioral_failure': True})
    print(json.dumps(results[-1]), flush=True)

  cargo = ['cargo', 'test', '--manifest-path', 'rust/Cargo.toml', '--locked']
  if options.scope == 'core':
    paths = ['rust/crates/modeld/src/action.rs'] + [f'rust/crates/drivingd/src/{name}.rs' for name in ['publication', 'state', 'jetlink', 'lib', 'daemon']]
    with original(paths):
      for case in ['issue_17_selected_uncertainty_controls_curvature_and_next_delay', 'issue_49_raw_request_keeps_external_publications_and_native_raw_identity']:
        red(case, cargo + ['-p', 'openpilot-driving-modeld', '--test', 'issue_17_49', case, '--', '--exact'])
    path = 'rust/crates/qcomgpsd/src/framing.rs'
    paths = [f'rust/crates/qcomgpsd/src/{name}.rs' for name in ['framing', 'serial', 'setup', 'daemon']]
    hook = '\n#[cfg(test)]\n#[path = "../tests/data/framing_retention.rs"]\nmod retention_tests;\n'
    with original(paths, {path: hook}):
      red('issue_320_retention', cargo + ['-p', 'openpilot-qcomgpsd', '--lib', 'framing::retention_tests::delimiter_free_retention_is_bounded', '--', '--exact'])
  elif options.scope == 'web':
    path = 'rust/crates/carrot-server/src/live/snapshot.rs'
    with original([path], {path: '\n#[cfg(test)]\n#[path = "snapshot_tests.rs"]\nmod snapshot_tests;\n'}):
      red('issue_254_navigation', cargo + ['-p', 'openpilot-carrot-server', '--lib', 'populated_cereal_navigation_maps_to_browser_fields_in_meters'])
    path = 'rust/crates/carrot-server/src/dashcam/report/aggregate.rs'
    with original([path], {path: '\n#[cfg(test)]\n#[path = "warning_tests.rs"]\nmod warning_tests;\n'}):
      red('issue_252_warnings', cargo + ['-p', 'openpilot-carrot-server', '--lib', 'numbered_driver_warnings_each_start_a_report_episode'])
    path = 'rust/crates/carrot-server/src/terminal/pty.rs'
    with original([path], {path: '\n#[cfg(test)]\n#[path = "pty_cleanup_tests.rs"]\nmod cleanup_tests;\n'}):
      red('issue_270_cleanup', cargo + ['-p', 'openpilot-carrot-server', '--lib', 'terminal::pty::cleanup_tests::issue270_', '--', '--nocapture', '--test-threads=1'])
  elif options.scope == 'vehicles':
    psa_paths = ['rust/crates/card/data/vehicle.json', 'rust/crates/card/src/brands/psa.rs',
                 'rust/crates/card/src/registry.rs', 'rust/crates/card/src/registry/diagnostics.rs',
                 'rust/crates/card/src/registry/vehicle.rs']
    with original(psa_paths, ref='b3705789764410d182e7bc9ac384634fa633e6cd'):
      for case in ['public_registry_admits_exact_psa_dashcam_parameters',
                   'public_registry_constructs_first_update_and_apply_from_known_angle_params']:
        red('issue_501_' + case, cargo + ['-p', 'openpilot-card', '--features', 'native',
            '--test', 'psa_admission_501', case, '--', '--exact'])
    with original(['rust/crates/card/data/vehicle.json']):
      red('issue_185_kia_key', cargo + ['-p', 'openpilot-card', '--features', 'native', '--test', 'hyundai_torque_key_497'])
  elif options.scope == 'athena':
    path = 'rust/crates/athena/src/upload_http.rs'
    hook = '\n#[cfg(test)]\n#[path = "upload_http_attribution_tests.rs"]\nmod attribution_tests;\n'
    with original([path], {path: hook}):
      red('issue_214_upload_attribution', cargo + ['-p', 'openpilot-athena', '--lib',
          'upload_http::attribution_tests::failure_receipts_are_sanitized_bounded_and_locate_real_upload_failures',
          '--', '--exact', '--nocapture', '--test-threads=1'])
  elif options.scope == 'card-attribution':
    with original(['rust/tools/card_qa/runtime_pumped_shutdown.py']):
      red('issue_201_shutdown_attribution', ['python', '-m', 'pytest', '-c', '/dev/null', '--rootdir=.',
          '--confcutdir=rust/tools/tests', '-q',
          'rust/tools/tests/test_card_attribution.py::test_real_shutdown_timeout_keeps_original_exception_and_captures_owned_process'],
          'AssertionError: missing shutdown timeout receipt')
  elif options.scope == 'media':
    with original(['rust/crates/loggerd/src/raw_file.rs', 'rust/crates/loggerd/src/media.rs']):
      red('issue_54_failed_video_completion', cargo + ['-p', 'openpilot-loggerd', '--test', 'media', 'marks_raw_video_failed_after_enospc'])
    with original(['rust/crates/uploader/src/scan.rs']):
      red('issue_54_failed_file_scanning', cargo + ['-p', 'openpilot-uploader', '--test', 'video_failures'])
  else:
    path = 'rust/crates/dev-tools/src/torque_stage.rs'
    with original([path]):
      red('issue_277_directories', cargo + ['-p', 'openpilot-dev-tools', '--test', 'torque_stage', 'directories_are_excluded_and_native_bytes_hashes_licenses_and_provenance_are_preserved', '--', '--exact'])
    path = 'rust/tools/extract_runtime_sdk.py'
    with original([path]):
      baseline = Path(path).read_bytes()
      exercised = sdk_baseline_with_current_space_policy(baseline)
      Path(path).write_bytes(exercised)
      (options.output / 'issue_292-source.json').write_text(json.dumps({
        'base': BASE, 'path': path,
        'original_sha256': hashlib.sha256(baseline).hexdigest(),
        'exercised_sha256': hashlib.sha256(exercised).hexdigest(),
        'capacity_policy_adjustment': 'retired fixed reserve removed; original 2 GiB growth remains',
        'growth_bytes': 2 * 1024**3, 'reserve_bytes': 0,
      }, indent=2) + '\n')
      red('issue_292_directory_aliases', ['python3', '-m', 'unittest', 'discover', '-s', 'rust/tools/tests', '-p', 'test_extract_runtime_sdk.py', '-k', 'directory_aliases_preserve_headers_without_duplicate_tree_traversal', '-v'], 'FAILED')
    path = 'rust/crates/jpeg/build.rs'
    target = Path(os.environ['RUNNER_TEMP']) / 'jpeg-345-baseline'
    with original([path]):
      subprocess.run(['cargo', 'build', '--manifest-path', 'rust/Cargo.toml', '--locked', '-p', 'openpilot-jpeg', '--target-dir', str(target)], check=True, timeout=900)
      scripts = list((target / 'debug/build').glob('openpilot-jpeg-*/build-script-build'))
      if len(scripts) != 1:
        raise AssertionError('expected one actual JPEG baseline build script')
      red('issue_345_source_identity', ['python3', 'rust/crates/jpeg/tests/build_cache.py', '--build-script', str(scripts[0])], 'does not match')
  (options.output / 'result.json').write_text(json.dumps({'base': BASE, 'scope': options.scope, 'results': results}, indent=2) + '\n')

if __name__ == '__main__':
  main()
