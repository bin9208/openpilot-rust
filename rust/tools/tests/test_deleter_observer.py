import json
from pathlib import Path
import sys
import textwrap

from check_deleter_daemon import run


def test_deletion_cadence_when_ready_and_first_delete_share_one_write(tmp_path: Path) -> None:
  trace = tmp_path / 'emitted.json'
  script = textwrap.dedent(r'''
    import json, os, sys, time
    from pathlib import Path
    emitted = [time.monotonic()]
    os.write(2, "deleter: ready\ndeleter: deleting 경로--0\n".encode())
    for index in (1, 2):
      time.sleep(.1)
      emitted.append(time.monotonic())
      os.write(2, f"deleter: deleting 경로--{index}\n".encode())
    Path(sys.argv[1]).write_text(json.dumps(emitted))
    time.sleep(.15)
  ''')
  result = run(sys.executable, tmp_path, ('-c', script, str(trace)))
  emitted = json.loads(trace.read_text())
  intervals = [right - left for left, right in zip(emitted, emitted[1:], strict=False)]
  print(json.dumps({'emitted_intervals': intervals, 'observed': result}, ensure_ascii=False))
  assert result['exit'] == 0 and result['ready']
  assert result['stderr'] == 'deleter: ready\n' + ''.join(f'deleter: deleting 경로--{index}\n' for index in range(3))
  assert len(result['deletion_intervals']) == 2
  assert all(.09 <= interval < .5 for interval in intervals)
  assert all(.09 <= interval < .5 for interval in result['deletion_intervals'])
