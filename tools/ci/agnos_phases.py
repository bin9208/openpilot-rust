"""Phase evidence and one admitted independent bootstrap child in an AGNOS job."""
from contextlib import contextmanager
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import threading
import time


@contextmanager
def phase(evidence, name):
  row = dict(name=name, started_monotonic=time.monotonic(), state='failed')
  evidence.setdefault('phase_times', []).append(row)
  print('AGNOS phase '+name, flush=True)
  try:
    yield
    row['state'] = 'success'
  except BaseException as error:
    row['failure'] = str(error)
    raise
  finally:
    row['elapsed_seconds'] = time.monotonic()-row['started_monotonic']


def parallel_admission(source, work):
  # Same bootstrap store growth calculation as its guarded driver. Independent
  # component proofs retain all their own room() checks as well.
  rust = json.loads((source/'rust/native-rust-toolchain.json').read_text())
  cargo = json.loads((source/'rust/native-cargo-inputs.json').read_text())
  tree = subprocess.check_output(['git', '-C', str(source), 'ls-tree', '-r', '-l', '-z', 'HEAD'])
  source_bytes = sum(int(row.split(b'\t', 1)[0].split()[3]) for row in tree.split(b'\0')
      if row and row.split(b'\t', 1)[0].split()[3] != b'-')
  bootstrap_growth = (sum(r['maximum_unpacked_bytes'] for r in rust['components'].values())
      + 2*sum(r['unpacked_bytes'] for r in cargo['packages'])
      + 2*source_bytes+64*1024**2+2*1024**3+512*1024**2)
  # Remaining bridges, component assembly, source assets and two native Cargo
  # consumer builds. These are per-operation estimates, not a free-space floor.
  component_growth = 12*1024**3
  memory = int(next(line.split()[1] for line in Path('/proc/meminfo').read_text().splitlines()
                    if line.startswith('MemAvailable:')))*1024
  for root in (Path('/sys/fs/cgroup'),):
    if (root/'memory.max').exists() and (root/'memory.max').read_text().strip() != 'max':
      memory = min(memory, max(0, int((root/'memory.max').read_text())-int((root/'memory.current').read_text())))
  cpus, free = len(os.sched_getaffinity(0)), shutil.disk_usage(work).free
  required_memory = (6+4)*1024**3+128*1024**2
  required_growth = bootstrap_growth+component_growth+64*1024**2
  return dict(parallel=cpus >= 2 and memory >= required_memory and free >= required_growth,
      affinity_cpus=cpus, available_memory_bytes=memory, free_disk_bytes=free,
      bootstrap_memory_bytes=6*1024**3, component_memory_bytes=4*1024**3,
      bootstrap_growth_bytes=bootstrap_growth, component_growth_bytes=component_growth,
      required_growth_bytes=required_growth)


@contextmanager
def independent_bootstrap(callback, source, work, evidence, log):
  policy = parallel_admission(source, work)
  evidence['bootstrap_component_admission'] = policy
  (work/'native-bootstrap-component-admission.json').write_text(json.dumps(dict(
      schema=1, source_commit=evidence['source_commit'], **policy))+'\n')
  if not policy['parallel']:
    with phase(evidence, 'bootstrap-current-tools-sequential'):
      callback(evidence, log)
    yield
    return
  private = dict(commands=[])
  processes, errors = [], []
  started = time.monotonic()
  row = dict(name='bootstrap-current-tools-parallel', started_monotonic=started, state='failed')
  evidence.setdefault('phase_times', []).append(row)

  def runner(argv, output_log, *, stdout_path):
    command = list(map(str, argv))
    with output_log.open('ab') as stderr, stdout_path.open('wb') as stdout:
      stderr.write((json.dumps(command)+'\n').encode())
      stderr.flush()
      child = subprocess.Popen(command, stdout=stdout, stderr=stderr, start_new_session=True)
      processes.append(child)
      code = child.wait()
      record = dict(argv=command, exit_code=code, elapsed_seconds=time.monotonic()-started)
      if code: raise RuntimeError('bootstrap proof failed; inspect '+str(output_log))
      return record

  def execute():
    try:
      callback(private, work/'bootstrap-parallel.log', runner=runner)
      row['state'] = 'success'
    except BaseException as error:
      errors.append(error)
      row['failure'] = str(error)
    finally:
      row['elapsed_seconds'] = time.monotonic()-started

  thread = threading.Thread(target=execute, name='agnos-bootstrap-proof')
  thread.start()
  def stop(child, name):
    if child.poll() is not None:
      return
    try:
      os.killpg(child.pid, getattr(signal, 'SIG'+name))
    except ProcessLookupError:
      pass
    except PermissionError:
      # sudo can retain a root monitor before its scoped command drops uid.
      # This PID/group was just created here; no runner/service PID is used.
      subprocess.run(['sudo', '-n', '/bin/kill', '-'+name, '--', '-'+str(child.pid)], check=True)
  try:
    yield
    thread.join()
    if errors: raise errors[0]
    evidence['commands'].extend(private['commands'])
    evidence['native_bootstrap_current_tools'] = private['native_bootstrap_current_tools']
  finally:
    if thread.is_alive():
      # A component failure cancels only this newly owned bootstrap process
      # group; never signal a runner, another Action or pre-existing process.
      for child in processes:
        stop(child, 'TERM')
      thread.join(timeout=5)
      if thread.is_alive():
        for child in processes:
          stop(child, 'KILL')
        thread.join()
