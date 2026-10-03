import json
from pathlib import Path

import pytest

from check_encoder_ci import dependency_outputs


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
