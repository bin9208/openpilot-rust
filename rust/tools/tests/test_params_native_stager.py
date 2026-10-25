"""Retained Params builder contracts split from retired encoder CI tests."""
import json
import subprocess
from pathlib import Path

import pytest

import build_params_python


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


def test_params_stager_preserves_failure_diagnostic(tmp_path: Path, capsys) -> None:
  binary = tmp_path / 'failing-stager'
  binary.write_text('#!/bin/sh\nprintf "stager output\\n"\nprintf "reserved capacity unavailable\\n" >&2\nexit 9\n')
  binary.chmod(0o755)
  with pytest.raises(subprocess.CalledProcessError):
    build_params_python.stage_native_json11(binary, tmp_path / 'dependencies')
  captured = capsys.readouterr()
  assert captured.out == 'stager output\n'
  assert captured.err == 'reserved capacity unavailable\n'


@pytest.mark.parametrize('output,status,error', [('not-json', 0, json.JSONDecodeError), ('failure', 9, subprocess.CalledProcessError)])
def test_params_stager_failure_precedes_schema_and_cython(tmp_path: Path, output: str, status: int, error: type) -> None:
  binary = native_stager(tmp_path, output, status)
  destination = tmp_path / 'binding'
  with pytest.raises(error):
    build_params_python.build(destination, json11_stager=binary)
  assert not (destination / 'schema').exists()
  assert not (destination / 'params_pyx.cpp').exists()
