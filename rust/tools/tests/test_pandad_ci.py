import json
from pathlib import Path

import pytest

from check_pandad_ci import zmq_output


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
