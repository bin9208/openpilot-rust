"""Drive original save_bootlog and native snapshots through real threads and children."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import select
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]

CHILD = '''
import json, os, pathlib, sys
snapshot=pathlib.Path(os.environ['PARAMS_COPY_PATH'])
print(json.dumps({'phase':'child_ready','cwd':os.getcwd(),'snapshot':str(snapshot), 'marker':os.environ.get('BOOTLOG_TEST_MARKER'),
                  'before':(snapshot/'d/DongleId').read_bytes().hex(), 'link_is_symlink':(snapshot/'d/link').is_symlink(),
                  'linked':(snapshot/'d/link').read_bytes().hex(), 'xattr':os.getxattr(snapshot/'d/DongleId','user.fixture').hex(),
                  'mode':(snapshot/'d/DongleId').stat().st_mode&0o777, 'mtime':(snapshot/'d/DongleId').stat().st_mtime_ns}),flush=True)
sys.stdin.readline()
print(json.dumps({'phase':'child_done','snapshot_exists':snapshot.exists(), 'after':(snapshot/'d/DongleId').read_bytes().hex(),
                  'live':(pathlib.Path(os.environ['PARAMS_ROOT'])/'d/DongleId').read_bytes().hex()}),flush=True)
if os.environ.get('BOOTLOG_CLEANUP_FAILURE'):
  snapshot.chmod(0)
sys.exit(int(os.environ.get('BOOTLOG_CHILD_EXIT','0')))
'''


def line(process):
  if process.stdout is None or not select.select([process.stdout], [], [], 15)[0]:
    raise TimeoutError('snapshot phase acknowledgement')
  result = process.stdout.readline()
  if not result:
    raise EOFError('snapshot phase stream ended')
  return json.loads(result)


def run(args, side, name, config):
  output = args.output / name / side
  output.mkdir(parents=True)
  temp = output / 'temporary'
  temp.mkdir()
  params = output / 'params/d'
  params.mkdir(parents=True)
  (params / 'DongleId').write_bytes(b'before\x00snapshot')
  (params / 'DongleId').chmod(0o640)
  os.utime(params / 'DongleId', ns=(1_600_000_000_000_000_000, 1_600_000_000_000_000_000))
  os.setxattr(params / 'DongleId', 'user.fixture', b'attribute')
  (params / 'link').symlink_to('DongleId')
  if config.get('broken'):
    (params / 'broken').symlink_to('missing')
  if config.get('fifo'):
    os.mkfifo(params / 'fifo')
  if config.get('unreadable_directory'):
    params.chmod(0)
  basedir = output / 'basedir'
  loggerd = basedir / 'openpilot/system/loggerd'
  loggerd.mkdir(parents=True)
  if not config.get('missing_binary'):
    binary = loggerd / 'bootlog'
    binary.write_text(f'#!{sys.executable}\n' + CHILD)
    binary.chmod(0o644 if config.get('nonexec') else 0o755)
  environment = dict(
    os.environ,
    PARAMS_ROOT=str(params.parent),
    OPENPILOT_PREFIX='d',
    TMPDIR=str(temp),
    PARAMS_COPY_PATH='must-be-overridden',
    BOOTLOG_TEST_MARKER='inherited-marker',
    BOOTLOG_CHILD_EXIT=str(config.get('exit', 0)),
  )
  if config.get('cleanup_failure'):
    environment['BOOTLOG_CLEANUP_FAILURE'] = '1'
  mode = 'detached' if config.get('detached') else 'joined'
  command = (
    [sys.executable, str(ROOT / 'rust/tools/bootlog_snapshot_source.py'), str(args.binding), str(basedir), mode]
    if side == 'source'
    else [*args.runner, str(args.binary), str(params), str(loggerd), mode]
  )
  rows = []
  with (output / 'stderr.log').open('w') as stderr:
    process = subprocess.Popen(command, env=environment, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=stderr, bufsize=0)
    try:
      if config.get('broken') or config.get('fifo') or config.get('unreadable_directory'):
        rows.append(line(process))
        assert rows[-1]['phase'] == 'copy_failed'
      elif config.get('missing_binary') or config.get('nonexec'):
        rows.extend([line(process), line(process)])
      elif config.get('detached'):
        # An inherited pipe is held by the child if it was launched before parent exit.
        rows.append(line(process))
        assert rows[-1]['phase'] == 'returned'
        process.wait(timeout=5)
        assert list(temp.iterdir()), 'detached parent removed pending snapshot'
        try:
          process.stdin.write(b'finish\n')
          process.stdin.flush()
        except BrokenPipeError:
          pass
        process.communicate(timeout=5)
      else:
        while {row['phase'] for row in rows} != {'returned', 'child_ready'}:
          rows.append(line(process))
        assert process.poll() is None
        (params / 'DongleId').write_bytes(b'live-after-return')
        process.stdin.write(b'finish\n')
        process.stdin.flush()
        rows.extend([line(process), line(process)])
      assert process.wait(timeout=15) == 0
    finally:
      if process.poll() is None:
        process.kill()
        process.wait()
      process.stdin.close()
      process.stdout.close()
  if any(row.get('phase') == 'worker_done' and row.get('success') is False for row in rows):
    assert (output / 'stderr.log').stat().st_size > 0, 'worker exception was silent'
  params.chmod(0o700)
  snapshots = list(temp.iterdir())
  for snapshot in snapshots:
    snapshot.chmod(0o700)
  for row in rows:
    if row['phase'] == 'child_ready':
      assert Path(row.pop('cwd')) == loggerd
      assert Path(row.pop('snapshot')).parent == temp
  result = {
    'rows': sorted(rows, key=lambda row: row['phase']),
    'retained_snapshots': len(snapshots),
    'retained_tree': sorted(
      (str(path.relative_to(snapshot)), '<directory>' if path.is_dir() else path.read_bytes().hex()) for snapshot in snapshots for path in snapshot.rglob('*')
    ),
  }
  (output / 'result.json').write_text(json.dumps(result, indent=2))
  (output / 'invocation.json').write_text(
    json.dumps(
      {
        'argv': command,
        'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest() if side == 'native' else None,
        'exit': process.returncode,
        'mode': mode,
        'inputs': config,
      },
      indent=2,
    )
  )
  return result


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('binary', type=Path)
  parser.add_argument('binding', type=Path)
  parser.add_argument('output', type=Path)
  parser.add_argument('--runner', action='append', default=[])
  args = parser.parse_args()
  args.binary, args.binding, args.output = args.binary.resolve(), args.binding.resolve(), args.output.resolve()
  cases = [
    ('ordinary', {}),
    ('nonzero_child', {'exit': 7}),
    ('missing_binary', {'missing_binary': True}),
    ('nonexec_binary', {'nonexec': True}),
    ('broken_link', {'broken': True}),
    ('fifo', {'fifo': True}),
    ('cleanup_failure', {'cleanup_failure': True}),
    ('unreadable_directory', {'unreadable_directory': True}),
    ('detached_parent_exit', {'detached': True}),
  ]
  results = []
  for name, config in cases:
    source = run(args, 'source', name, config)
    native = run(args, 'native', name, config)
    assert source == native, (name, source, native)
    results.append({'scenario': name, 'result': 'PASS'})
    print(name, 'PASS', flush=True)
  (args.output / 'result.json').write_text(json.dumps({'result': 'PASS', 'cases': results}, indent=2))


if __name__ == '__main__':
  main()
