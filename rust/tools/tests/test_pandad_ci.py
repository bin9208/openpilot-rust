import json
from pathlib import Path
import sys
from types import SimpleNamespace

import pytest

from check_pandad_ci import zmq_output
import check_pandad_ci


def test_zmq_dependency_comes_from_executed_build_with_other_caches_present(tmp_path: Path) -> None:
  selected = tmp_path / 'executed output'
  stale = tmp_path / 'stale output'
  for root in (selected, stale):
    (root / 'source/include').mkdir(parents=True)
    (root / 'source/include/zmq.h').write_text('fixture header')
    (root / 'lib').mkdir()
    (root / 'lib/libzmq.a').write_bytes(b'fixture archive')
  messages = tmp_path / 'build.jsonl'
  messages.write_text('\n'.join(json.dumps(row) for row in (
    {'reason': 'build-script-executed', 'package_id': 'registry+example#zmq-sys@0.12.0', 'out_dir': str(selected)},
    {'reason': 'compiler-artifact', 'package_id': 'registry+example#zmq-sys@0.12.0', 'out_dir': str(stale)},
    {'reason': 'build-finished', 'success': True},
  )))
  assert zmq_output(messages) == selected.resolve()
  (selected / 'lib/libzmq.a').unlink()
  with pytest.raises(FileNotFoundError):
    zmq_output(messages)


def test_zmq_dependency_rejects_absent_or_ambiguous_build_receipt(tmp_path: Path) -> None:
  messages = tmp_path / 'build.jsonl'
  for directories in ([], ['first', 'second']):
    messages.write_text('\n'.join(json.dumps({'reason': 'build-script-executed',
      'package_id': 'registry+example#zmq-sys@0.12.0', 'out_dir': str(tmp_path / name)}) for name in directories))
    with pytest.raises(ValueError, match='expected one executed zmq-sys'):
      zmq_output(messages)


def test_spidev_comparison_has_a_fresh_output_separate_from_its_builder(tmp_path: Path, monkeypatch) -> None:
  class ReachedComparison(Exception):
    pass

  built = []
  native_builds = []
  checker = check_pandad_ci.PandaCheck(tmp_path, tmp_path / 'binaries')
  monkeypatch.setattr(check_pandad_ci, 'require_space', lambda *_: None)
  def reject_python_staging(*_):
    pytest.fail('Panda CI must use the native JSON11 staging command')

  monkeypatch.setattr(check_pandad_ci, 'stage_json11', reject_python_staging, raising=False)
  monkeypatch.setattr(checker, 'spidev', lambda: tmp_path / 'spidev-source')
  monkeypatch.setitem(sys.modules, 'usb1', SimpleNamespace(__file__=str(tmp_path / 'usb1.py')))

  def record(name: str, script: str, arguments: list[str]) -> None:
    if name == 'build-spidev':
      folder = Path(arguments[arguments.index('--output') + 1])
      folder.mkdir()
      built.append(folder)
    elif name == 'check-spidev':
      folder = Path(arguments[arguments.index('--output') + 1])
      folder.mkdir()
      assert Path(arguments[arguments.index('--library') + 1]).parent == built[0]
      raise ReachedComparison

  monkeypatch.setattr(checker, 'python', record)
  def record_native(name: str, arguments: list[str], growth_mib: int = 512) -> None:
    executable = Path(arguments[0])
    assert executable.parent == checker.binaries
    assert arguments[1:3] == ['--root', str(check_pandad_ci.ROOT)]
    native_builds.append(executable.name)
    if name == 'stage-json11':
      checker.commands.mkdir(parents=True)
      (checker.commands / f'{name}.stdout').write_text(json.dumps({
        'prefix': str(tmp_path / 'json11'), 'dependency': {'json11_architecture': 'x86_64'}}))

  monkeypatch.setattr(checker, 'command', record_native)
  with pytest.raises(ReachedComparison):
    checker.execute(tmp_path / 'zmq')
  assert native_builds == ['stage-json11'] + ['build-pandad-protocol-source'] * 5 + [
    'build-pandad-peripheral-source', 'build-pandad-spi-source', 'build-pandad-usb-source']
  assert json.loads((tmp_path / 'json11-dependency.json').read_text()) == {'json11_architecture': 'x86_64'}
