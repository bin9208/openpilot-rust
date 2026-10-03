import argparse
from contextlib import suppress
import json
import os
from pathlib import Path
import signal
import subprocess

from athena_fixture import daemon, wait_for


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--output', required=True, type=Path)
  args = parser.parse_args()
  output = args.output.resolve()
  output.mkdir(parents=True, exist_ok=False)
  binary = output / 'native-idle'
  subprocess.run(['cc', '-Wall', '-Wextra', '-Werror', str(Path(__file__).with_name('athena_idle_fixture.c')),
                  '-o', str(binary)], check=True)
  tracer = output / 'strace'
  tracer.write_bytes(Path(__file__).with_name('athena_tracer_fixture.py').read_bytes())
  tracer.chmod(0o755)
  pids = output / 'pids.json'
  environment = dict(os.environ, PATH=f'{output}:{os.environ["PATH"]}', ATHENA_TRACER_PIDS=str(pids),
                     ATHENA_TRACER_PUBLISH_DELAY=os.environ.get('ATHENA_TRACER_PUBLISH_DELAY', '0.2'))

  def published_pids():
    value = json.loads(pids.read_text())
    return value if 'native' in value else None

  try:
    with daemon(binary, output, environment, trace=True) as (process, pid):
      observed = wait_for(published_pids)
      assert pid == observed['native'] and pid != observed['helper'], observed
      assert Path(f'/proc/{pid}/exe').resolve() == binary
      os.kill(pid, signal.SIGTERM)
      assert process.wait(timeout=3) == 241
      result = {'pids': observed, 'selected': pid}
  finally:
    if pids.exists():
      tracer_pid = json.loads(pids.read_text())['tracer']
      with suppress(ProcessLookupError):
        os.killpg(tracer_pid, signal.SIGKILL)
  failed = output / 'early-exit'
  failed.mkdir()
  environment.update(ATHENA_TRACER_FAIL='1', ATHENA_TRACER_PIDS=str(failed / 'pids.json'))
  try:
    with daemon(binary, failed, environment, trace=True):
      raise AssertionError('an exited tracer cannot satisfy native readiness')
  except ChildProcessError as error:
    assert str(error) == 'owned Athena process exited before readiness: 7'
    result['early_exit'] = str(error)
  else:
    raise AssertionError('expected startup failure')
  result['pass'] = True
  (output / 'result.json').write_text(json.dumps(result, indent=2))
  print('PASS: transient trace helper is ignored; actual native executable is selected and owned processes exit')


if __name__ == '__main__':
  main()
