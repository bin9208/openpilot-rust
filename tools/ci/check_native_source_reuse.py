#!/usr/bin/env python3
"""#608: real loggerd/logmessaged inputs, not a dependency-free stand-in."""
import hashlib
import json
import os
import subprocess
import time
from pathlib import Path


def main():
  assert os.environ['RUNNER_NAME'] == 'bin-actions' and os.environ['RUNNER_ENVIRONMENT'] == 'self-hosted'
  source = Path(os.environ['GITHUB_WORKSPACE'])
  work = Path(os.environ['RUNNER_TEMP']) / 'native-source-reuse-608'
  work.mkdir()
  checkout, cache = work / 'source', work / 'cache'
  evidence = work / 'evidence'
  evidence.mkdir()
  env = dict(os.environ, GIT_LFS_SKIP_SMUDGE='1', CARGO_INCREMENTAL='0')
  def run(args, **kwargs):
    return subprocess.run(list(map(str, args)), check=True, env=env, **kwargs)
  def git(*args):
    return subprocess.check_output(['git', '-c', 'core.hooksPath=/dev/null', '-c', 'commit.gpgsign=false', '-C', str(checkout), *args], env=env, text=True).strip()
  # The source object database remains in this job's original Actions checkout.
  # Only this clone is changed. No external source/cache is cleaned or reset.
  run(['git', '-c', 'core.hooksPath=/dev/null', 'clone', '--shared', '--no-checkout', source, checkout])
  git('checkout', '--detach', os.environ['GITHUB_SHA'])
  git('config', 'user.name', 'Owned source-reuse fixture')
  git('config', 'user.email', 'source-reuse@example.invalid')
  initial = git('rev-parse', 'HEAD')
  assert initial == os.environ['GITHUB_SHA'] and not git('status', '--porcelain')
  cargo = Path(subprocess.check_output(['rustup', 'which', '--toolchain', '1.94.0', 'cargo'], text=True).strip())
  rustc = Path(subprocess.check_output(['rustup', 'which', '--toolchain', '1.94.0', 'rustc'], text=True).strip())
  assert cargo.parent == rustc.parent
  host = next(line.removeprefix('host: ') for line in subprocess.check_output([rustc, '-vV'], text=True).splitlines() if line.startswith('host: '))
  driver = Path(os.environ['NATIVE_REUSE_BUILD_DRIVER'])
  (cache / 'cargo-home').mkdir(parents=True)
  fetch_env = dict(env, CARGO_HOME=str(cache / 'cargo-home'))
  subprocess.run([cargo, 'fetch', '--locked', '--manifest-path', checkout / 'rust/Cargo.toml', '--target', host], env=fetch_env, check=True)
  names = {'openpilot-loggerd', 'openpilot-logmessaged'}
  recipe = dict(schema=1, target=host, packages=sorted(names), binaries=sorted(names), features=[],
                inputs={}, input_trees={}, environment={'LIBCLANG_PATH': '/usr/lib/llvm-18/lib'},
                expected_growth_bytes=4 * 1024**3)
  source_file = checkout / 'rust/crates/loggerd/src/main.rs'
  def build(label, native):
    settings = dict(recipe, environment=dict(recipe['environment']))
    if native:
      settings['environment']['OPENPILOT_NATIVE_INCREMENTAL'] = '1'
    path = work / 'recipe.json'
    path.write_text(json.dumps(settings) + '\n')
    began = time.monotonic()
    result = subprocess.check_output([driver, '--root', checkout, '--recipe', path, '--cache', cache, '--toolchain', cargo.parent], env=env, text=True)
    report = json.loads(result)
    (evidence / f'{label}.json').write_text(json.dumps(report, indent=2) + '\n')
    assert report['source']['commit'] == git('rev-parse', 'HEAD')
    artifacts = {Path(row['executable']).name: row for row in report['artifacts']}
    assert artifacts.keys() == names
    for name, row in artifacts.items():
      binary = Path(row['executable'])
      assert hashlib.sha256(binary.read_bytes()).hexdigest() == row['sha256']
      if name == 'openpilot-loggerd':
        help_text = subprocess.check_output([binary, '--help'], env=env, text=True)
        expected = 'Owned incremental route logger probe.' if 'Owned incremental route logger probe.' in source_file.read_text() else 'Continuous native route logger.'
        assert expected in help_text
    print(json.dumps(dict(case=label, elapsed_seconds=time.monotonic()-began,
                          source_commit=report['source']['commit'], artifacts={name: dict(fresh=row['fresh'], sha256=row['sha256']) for name,row in artifacts.items()})), flush=True)
    return artifacts
  def commit(path, contents, message):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(contents)
    git('add', str(path.relative_to(checkout)))
    git('commit', '-m', message)
    assert not git('status', '--porcelain')
    return git('rev-parse', 'HEAD')
  first = build('native-initial', True)
  assert all(not row['fresh'] for row in first.values())
  repeat = build('native-repeat', True)
  assert all(row['fresh'] and row['sha256'] == first[name]['sha256'] for name,row in repeat.items())
  docs = commit(checkout / 'docs/naver/owned-source-reuse-608.md', 'Owned docs-only probe\n', 'Owned docs-only change')
  doc_result = build('native-docs-only', True)
  assert docs != initial
  assert all(row['fresh'] and row['sha256'] == first[name]['sha256'] for name,row in doc_result.items())
  # Retain the honest compilation commit; never rewrite A's embedded identity as
  # B merely because B's actual code inputs and output bytes are identical.
  assert initial.encode() in Path(doc_result['openpilot-loggerd']['executable']).read_bytes()
  before = source_file.read_text()
  assert before.count('Continuous native route logger.') == 1
  changed = commit(source_file, before.replace('Continuous native route logger.', 'Owned incremental route logger probe.'), 'Owned loggerd-only source change')
  small = build('native-small-source', True)
  assert not small['openpilot-loggerd']['fresh'] and small['openpilot-loggerd']['sha256'] != first['openpilot-loggerd']['sha256']
  assert small['openpilot-logmessaged']['fresh'] and small['openpilot-logmessaged']['sha256'] == first['openpilot-logmessaged']['sha256']
  assert changed.encode() in Path(small['openpilot-loggerd']['executable']).read_bytes()
  legacy = build('legacy-initial', False)
  assert all(not row['fresh'] for row in legacy.values())
  legacy_commit = commit(checkout / 'docs/naver/owned-source-reuse-608.md', 'Second owned docs-only probe\n', 'Owned legacy stamp control')
  legacy_docs = build('legacy-docs-control', False)
  assert all(not row['fresh'] and row['sha256'] != legacy[name]['sha256'] for name,row in legacy_docs.items())
  assert legacy_commit.encode() in Path(legacy_docs['openpilot-loggerd']['executable']).read_bytes()
  summary = dict(source_commit=initial, host_target=host, cases=6,
                 native_docs_only_all_fresh=True, native_small_change_unrelated_fresh=True,
                 original_compilation_provenance_retained=True, legacy_exact_source_stamp_preserved=True,
                 scope='Actual loggerd/logmessaged Cargo inputs in this CI userspace; not full AGNOS generation/device acceptance')
  (evidence / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')
  print(json.dumps(summary))


if __name__ == '__main__':
  main()
