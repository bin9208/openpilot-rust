import json
from pathlib import Path
import subprocess
import sys

import pytest

import check_pandad_spi


def test_spi_timeout_preserves_partial_stdout_and_stderr(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
  source = tmp_path / 'source'
  source.write_text(f'#!{sys.executable}\nimport sys, time\nprint("partial result", flush=True)\n'
                    + 'print("partial diagnostic", file=sys.stderr, flush=True)\ntime.sleep(5)\n')
  source.chmod(0o700)
  output = tmp_path / 'output'
  monkeypatch.setattr(sys, 'argv', ['check_pandad_spi', '--source', str(source), '--native', str(tmp_path / 'unused'), '--output', str(output)])
  monkeypatch.setattr(check_pandad_spi, 'full_scenarios', list)
  original_run = subprocess.run

  def bounded_run(*args, **kwargs):
    kwargs['timeout'] = 2
    return original_run(*args, **kwargs)

  monkeypatch.setattr(check_pandad_spi.subprocess, 'run', bounded_run)
  with pytest.raises(subprocess.TimeoutExpired):
    check_pandad_spi.main()
  assert (output / 'source.jsonl').read_text() == 'partial result\n'
  assert (output / 'source.stderr').read_text() == 'partial diagnostic\n'
  result = json.loads((output / 'source.exit.json').read_text())
  assert result['status'] == 'TIMEOUT'
  assert result['returncode'] is None
