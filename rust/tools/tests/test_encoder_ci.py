import json
import subprocess
from pathlib import Path

import pytest

from check_encoder_ci import dependency_outputs

import build_params_python
import check_encoder_ci


def write_receipt(path: Path, outputs: list[tuple[str, Path]], success: bool = True) -> None:
  rows = [{'reason': 'build-script-executed', 'package_id': f'path+fixture#{name}@0.1.0', 'out_dir': str(output)}
          for name, output in outputs]
  rows.append({'reason': 'build-finished', 'success': success})
  path.write_text(''.join(json.dumps(row) + '\n' for row in rows))


def test_selects_only_current_jpeg_and_zmq_outputs(tmp_path: Path) -> None:
  jpeg, zmq, stale = (tmp_path / name for name in ('jpeg-current', 'zmq-current', 'stale'))
  for root, files in ((jpeg, ('jpeg/libjpeg.a', 'jpeg/jconfig.h')), (zmq, ('source/include/zmq.h', 'lib/libzmq.a')),
                      (stale, ('jpeg/libjpeg.a', 'jpeg/jconfig.h'))):
    for relative in files:
      path = root / relative
      path.parent.mkdir(parents=True, exist_ok=True)
      path.write_bytes(b'owned fixture')
  receipt = tmp_path / 'build.jsonl'
  write_receipt(receipt, [('openpilot-jpeg', jpeg), ('zmq-sys', zmq)])
  assert dependency_outputs(receipt) == {'jpeg': jpeg / 'jpeg', 'zmq': zmq}
  (jpeg / 'jpeg/libjpeg.a').unlink()
  with pytest.raises(FileNotFoundError):
    dependency_outputs(receipt)


@pytest.mark.parametrize('outputs,success', [([], True), ([('openpilot-jpeg', 'a'), ('openpilot-jpeg', 'b')], True), ([], False)])
def test_rejects_missing_ambiguous_or_failed_build(tmp_path: Path, outputs: list[tuple[str, str]], success: bool) -> None:
  receipt = tmp_path / 'build.jsonl'
  write_receipt(receipt, [(name, tmp_path / directory) for name, directory in outputs], success)
  with pytest.raises(ValueError):
    dependency_outputs(receipt)


def native_stager(tmp_path: Path, output: str, status: int = 0) -> Path:
  import shlex
  executable = tmp_path / 'native-stager'
  executable.write_text('#!/bin/sh\nprintf %s ' + shlex.quote(output) + f'\nexit {status}\n')
  executable.chmod(0o755)
  return executable


def test_params_consumes_explicit_native_stager_metadata(tmp_path: Path) -> None:
  prefix = tmp_path / 'native-prefix'
  dependency = {'json11_version': 'fixture', 'json11_architecture': 'aarch64', 'json11_sha256': 'a' * 64}
  binary = native_stager(tmp_path, json.dumps({'prefix': str(prefix), 'dependency': dependency}))
  assert build_params_python.stage_native_json11(binary, tmp_path / 'dependencies') == (prefix, dependency)


@pytest.mark.parametrize('output,status,error', [('not-json', 0, json.JSONDecodeError), ('failure', 9, subprocess.CalledProcessError)])
def test_params_stager_failure_precedes_schema_and_cython(tmp_path: Path, output: str, status: int, error: type) -> None:
  binary = native_stager(tmp_path, output, status)
  destination = tmp_path / 'binding'
  with pytest.raises(error):
    build_params_python.build(destination, json11_stager=binary)
  assert not (destination / 'schema').exists()
  assert not (destination / 'params_pyx.cpp').exists()


def test_encoder_stager_receipt_and_prestaged_bypass(tmp_path: Path, monkeypatch) -> None:
  monkeypatch.setattr(check_encoder_ci, 'require_space', lambda *_: None)
  prefix = tmp_path / 'native-prefix'
  dependency = {'json11_sha256': 'b' * 64}
  binary = native_stager(tmp_path, json.dumps({'prefix': str(prefix), 'dependency': dependency}))
  result = check_encoder_ci.json11_dependency(None, binary, tmp_path)
  assert result == prefix
  assert json.loads((tmp_path / 'json11.json').read_text()) == dependency
  invocation = json.loads((tmp_path / 'commands/stage-json11.command.json').read_text())
  assert invocation['argv'] == [str(binary), '--root', str(check_encoder_ci.ROOT), '--output', str(tmp_path / 'json11-dependency')]
  binary.unlink()
  assert check_encoder_ci.json11_dependency(prefix, binary, tmp_path) == prefix


@pytest.mark.parametrize('exit_code', [0, 9])
def test_native_encoder_builder_boundary_preserves_arguments_and_failures(tmp_path: Path, monkeypatch, exit_code: int) -> None:
  monkeypatch.setattr(check_encoder_ci, 'require_space', lambda *_: None)
  binaries = tmp_path / 'bin'
  binaries.mkdir()
  builder = binaries / 'build-encoder-source'
  builder.write_text(f'#!/bin/sh\nprintf "%s\\n" "$@"\nprintf diagnostic >&2\nexit {exit_code}\n')
  builder.chmod(0o755)
  monkeypatch.setenv('PATH', str(binaries))
  output = tmp_path / 'owned output'
  dependencies = {'zmq': tmp_path / 'current-zmq', 'jpeg': tmp_path / 'current-jpeg'}
  arguments = (binaries, output, tmp_path / 'native', tmp_path / 'capnp', tmp_path / 'json11', dependencies)
  if exit_code:
    with pytest.raises(subprocess.CalledProcessError):
      check_encoder_ci.build_source(*arguments)
  else:
    check_encoder_ci.build_source(*arguments)
  invocation = json.loads((output / 'commands/source-build.command.json').read_text())
  assert invocation['argv'] == [str(builder), '--root', str(check_encoder_ci.ROOT), '--output', str(output / 'source'),
    '--native-prefix', str(tmp_path / 'native/usr'), '--capnp-prefix', str(tmp_path / 'capnp'),
    '--json11-prefix', str(tmp_path / 'json11'), '--zmq-include', str(dependencies['zmq'] / 'source/include'),
    '--jpeg-build', str(dependencies['jpeg'])]
  assert (output / 'commands/source-build.stdout').read_text().splitlines() == invocation['argv'][1:]
  assert (output / 'commands/source-build.stderr').read_text() == 'diagnostic'
  assert json.loads((output / 'commands/source-build.exit.json').read_text()) == {'returncode': exit_code}
