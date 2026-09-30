"""Production updater entrypoint with owned Git trees, signal requests and overlay boundary."""

import argparse
import json
import os
from pathlib import Path
import signal
import subprocess
import tempfile
import time
import uuid

import zmq
from updated_fixtures import git, seed, setup


def wait_for(predicate, process, log):
  deadline = time.monotonic() + 15
  while not predicate():
    assert process.poll() is None, (process.returncode, log.read_text())
    assert time.monotonic() < deadline, log.read_text()
    time.sleep(0.02)


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--launcher', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  with tempfile.TemporaryDirectory(prefix='updated-daemon-') as temporary:
    directory = Path(temporary)
    remote, previous = seed(directory)
    root = directory / 'runtime'
    config, env = setup(root, remote, previous, args.launcher.resolve())
    prefix = 'updated_' + uuid.uuid4().hex
    env.update(UPDATER_STAGING_ROOT=config['paths']['staging'], UPDATER_LOCK_FILE=config['paths']['lock'], OPENPILOT_PREFIX=prefix)
    params = root / 'data/params/d'
    command = [str(args.binary.resolve()), '--basedir', config['paths']['base'], '--system-root', str(root), '--launcher', str(args.launcher.resolve())]
    context = zmq.Context()
    collector = context.socket(zmq.PULL)
    collector.setsockopt(zmq.LINGER, 0)
    endpoint = '/tmp/logmessage' + prefix
    collector.bind('ipc://' + endpoint)
    process = None
    log = args.output / 'daemon.log'
    try:
      with log.open('w') as output:
        process = subprocess.Popen(command, env=env, stdout=output, stderr=output)
        wait_for(lambda: (params / 'UpdaterCurrentDescription').exists() and (params / 'UpdaterFetchAvailable').exists(), process, log)
        duplicate = subprocess.run(command, env=env, capture_output=True, text=True, timeout=5)
        assert duplicate.returncode == 1 and 'overlay lock' in duplicate.stderr
        (args.output / 'duplicate.log').write_text(duplicate.stderr)
        process.send_signal(signal.SIGUSR1)
        wait_for(lambda: (params / 'UpdaterFetchAvailable').read_text() == '1' and (params / 'UpdaterState').read_text() == 'idle', process, log)
        assert (params / 'UpdateAvailable').read_text() == '0'
        process.send_signal(signal.SIGHUP)
        wait_for(lambda: (params / 'UpdateAvailable').read_text() == '1' and (params / 'UpdaterState').read_text() == 'idle', process, log)
        finalized = Path(config['paths']['staging']) / 'finalized'
        assert (finalized / '.overlay_consistent').is_file()
        assert git(finalized, 'rev-parse', 'HEAD') == git(remote, 'rev-parse', 'HEAD')
        assert (finalized / 'common/version.h').read_text() == '#define COMMA_VERSION "2.0"\n'
        assert (finalized / 'version-link').is_symlink()
        assert os.access(finalized / 'launch_env.sh', os.X_OK)
        assert (params / 'UpdaterNewReleaseNotes').read_bytes().startswith(b'<h1>2.0</h1>')
        assert (params / 'UpdaterLastFetchTime').exists()
        process.send_signal(signal.SIGTERM)
        status = process.wait(timeout=4)
        assert status == 0
      if log.stat().st_size == 0:
        log.write_text('<empty>\n')
      records = []
      while collector.poll(100):
        raw = collector.recv()
        records.append(json.loads(raw[1:]))
      messages = [row['msg'] for row in records]
      assert 'caught SIGUSR1, checking for updates' in messages
      assert 'caught SIGHUP, attempting to downloading update' in messages
      (args.output / 'records.json').write_text(json.dumps(records, indent=2))
      (args.output / 'params.json').write_text(json.dumps({p.name: p.read_text() for p in params.iterdir() if p.is_file()}, indent=2))
      (args.output / 'overlay-commands.jsonl').write_bytes((root / 'overlay-commands.jsonl').read_bytes())
      (args.output / 'result.json').write_text(
        json.dumps(
          {
            'passed': True,
            'duplicate_lock_exit': duplicate.returncode,
            'sigterm_exit': status,
            'sigusr1_checked': True,
            'sighup_fetched': True,
            'finalized_commit': git(remote, 'rev-parse', 'HEAD'),
            'actual_mount_or_sudo': False,
            'device_access': False,
          },
          indent=2,
        )
        + '\n'
      )
      print('PASS production updated: exclusive lock, SIGUSR1 check, SIGHUP fetch/finalize, Params/logs, symlink/mode and SIGTERM lifecycle')
    finally:
      if process is not None and process.poll() is None:
        process.kill()
        process.wait(timeout=3)
      collector.close()
      context.term()
      Path(endpoint).unlink(missing_ok=True)


if __name__ == '__main__':
  main()
